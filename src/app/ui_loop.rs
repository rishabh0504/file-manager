//! Per-frame update: background polling and the window layout.
use super::*;

impl eframe::App for SortwiseApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(plan) = self.snapshot.as_mut() {
            // Let layout and fonts settle, then ask egui for its own rendered frame.
            plan.frames += 1;
            if plan.frames >= 40 && !plan.requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                plan.requested = true;
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                let [w, h] = image.size;
                match image::save_buffer(&plan.path, &bytes, w as u32, h as u32, image::ColorType::Rgba8) {
                    Ok(()) => eprintln!("snapshot written: {}", plan.path.display()),
                    Err(e) => eprintln!("snapshot failed: {e}"),
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
        // Persist scan preferences when they change (U16). A failed write is reported once,
        // and the attempt is not repeated every frame.
        let current = self.current_settings();
        if current != self.saved_settings {
            if let Some(path) = sortwise::settings::settings_path() {
                if let Err(e) = sortwise::settings::save_to(&path, &current) {
                    self.apply_errors = Some(format!("Could not save settings: {e}"));
                }
            }
            self.saved_settings = current;
        }

        // Poll background thumbnails (batched to max 16 per frame)
        let mut upload_count = 0;
        let mut got_any = false;
        let cur_epoch = self.navigation_epoch.load(std::sync::atomic::Ordering::Relaxed);
        while upload_count < 16 {
            if let Ok(resp) = self.thumbnail_response_rx.try_recv() {
                self.thumbnail_loading.remove(&resp.path);
                if resp.epoch == cur_epoch {
                    if let Some(img) = resp.image {
                        let id = format!("sfm_thumb_{}", resp.path.display());
                        let handle = ctx.load_texture(id, img, egui::TextureOptions::LINEAR);
                        // In-memory LRU cap: keep max 1,000 textures
                        if self.thumbnail_cache.len() >= 1000 {
                            let keys_to_remove: Vec<PathBuf> =
                                self.thumbnail_cache.keys().take(100).cloned().collect();
                            for k in keys_to_remove {
                                self.thumbnail_cache.remove(&k);
                            }
                        }
                        self.thumbnail_cache.insert(resp.path, handle);
                        upload_count += 1;
                        got_any = true;
                    } else {
                        self.thumbnail_failed.insert(resp.path);
                    }
                }
            } else {
                break;
            }
        }
        if got_any {
            ctx.request_repaint();
        }

        self.poll_archive_files_move_result();
        self.poll_queue_submit_result();

        // Keep repainting while a move is in progress so the progress bar updates
        if self.archive_files_moving || self.explorer_queue_submitting {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Poll the duplicate scan whenever it runs, even if the user has opened the
        // partial results view (P-6), so the final result is never missed.
        self.poll_live_groups();
        if self.result_rx.is_some() {
            self.poll_result();
        }
        if self.view == AppView::Scanning {
            self.poll_empty_folders_result();
            self.poll_archive_files_result();
        }
        if self.mode == AppMode::Explorer {
            self.explorer_ensure_loaded();
        }

        // Poll recursive search results
        if self.explorer_searching {
            let (got, lost) = match &self.explorer_search_rx {
                Some(rx) => match rx.try_recv() {
                    Ok(entries) => (Some(entries), false),
                    Err(std::sync::mpsc::TryRecvError::Empty) => (None, false),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => (None, true),
                },
                None => (None, true),
            };
            if lost {
                // Worker ended without results: stop the spinner instead of waiting forever.
                self.explorer_searching = false;
                self.explorer_search_rx = None;
            } else if let Some(entries) = got {
                self.explorer_entries = entries;
                self.explorer_sort_entries();
                self.explorer_searching = false;
                self.explorer_search_rx = None;
                self.explorer_search_select_all();
                ctx.request_repaint();
            } else {
                // Keep UI responsive while the worker runs.
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        // Debounced recursive search: fire only after 2s idle since last keystroke.
        if self.mode == AppMode::Explorer {
            let filter_changed = self.explorer_filter != self.explorer_last_filter;
            self.explorer_last_filter = self.explorer_filter.clone();
            if filter_changed {
                if self.explorer_filter.trim().is_empty() {
                    self.explorer_cancel_search();
                    self.explorer_reload();
                } else {
                    // Abort in-flight work; wait for idle before scanning again.
                    self.explorer_abort_in_flight_search();
                    self.explorer_search_pending = true;
                    self.explorer_search_typed_at = Some(Instant::now());
                }
            }

            if self.explorer_search_pending {
                if let Some(typed_at) = self.explorer_search_typed_at {
                    let elapsed = typed_at.elapsed();
                    if elapsed >= SEARCH_DEBOUNCE {
                        if !self.explorer_filter.trim().is_empty() {
                            self.explorer_search_recursive();
                        } else {
                            self.explorer_search_pending = false;
                            self.explorer_search_typed_at = None;
                        }
                    } else {
                        ctx.request_repaint_after(SEARCH_DEBOUNCE - elapsed);
                    }
                }
            }
        }

        let mode = self.mode;
        let p = crate::ui::style::pal();

        // Left navigation (modes + places). Icon rail when the window is narrow or staging is open.
        let staging_open = self.movable_queue_open || self.explorer_queue_view_open;
        let rail = crate::ui::style::is_narrow(ctx)
            || (staging_open && ctx.screen_rect().width() < 1200.0);
        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(if rail { crate::ui::shell::RAIL_W } else { crate::ui::shell::NAV_W })
            .frame(
                egui::Frame::none()
                    .fill(p.sidebar)
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .stroke(egui::Stroke::new(1.0, p.border)),
            )
            .show(ctx, |ui| crate::ui::shell::nav(self, ui, rail));

        egui::TopBottomPanel::top("top")
            .exact_height(crate::ui::style::H_TOOLBAR)
            .frame(
                egui::Frame::none()
                    .fill(p.surface)
                    .inner_margin(egui::Margin::symmetric(20.0, 0.0))
                    .stroke(egui::Stroke::new(1.0, p.border)),
            )
            .show(ctx, |ui| crate::ui::shell::toolbar(self, ui));

        // Status bar. Added before the side and central panels, as egui requires.
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(
                egui::Frame::none()
                    .fill(p.surface)
                    .inner_margin(egui::Margin::symmetric(14.0, 0.0))
                    .stroke(egui::Stroke::new(1.0, p.border)),
            )
            .show(ctx, |ui| crate::ui::status_bar::status_bar(self, ui));

        let results_ready =
            (mode == AppMode::Dedupe || mode == AppMode::CompareSourceDest)
                && matches!(self.view, AppView::Results)
                && self.last_result.is_some();

        // Duplicate groups column (results only).
        if results_ready {
            let narrow = crate::ui::style::is_narrow(ctx);
            // Narrow windows get a fixed slim column (a remembered wide width would crowd the
            // detail view); wide windows keep the resizable column.
            let groups = egui::SidePanel::left(if narrow { "groups_narrow" } else { "groups" });
            let groups = if narrow {
                groups.resizable(true).default_width(220.0).min_width(180.0).max_width(360.0)
            } else {
                groups.resizable(true).default_width(300.0).min_width(220.0).max_width(520.0)
            };
            groups
                .frame(
                    egui::Frame::none()
                        .fill(p.canvas)
                        .inner_margin(egui::Margin::same(12.0))
                        .stroke(egui::Stroke::new(1.0, p.border)),
                )
                .show(ctx, |ui| results_dup_list_panel(self, ui, ctx));
        }

        // Staging panel: a right side panel, so it must be added before the central panel.
        crate::ui::staging_panel(self, ctx);

        let central_margin = match mode {
            AppMode::Explorer | AppMode::AllFiles => egui::Margin::symmetric(16.0, 12.0),
            _ if crate::ui::style::is_narrow(ctx) => egui::Margin::symmetric(14.0, 12.0),
            _ => egui::Margin::symmetric(24.0, 18.0),
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(crate::ui::style::pal().canvas)
                    .inner_margin(central_margin),
            )
            .show(ctx, |ui| {
                match mode {
                    AppMode::Explorer => crate::ui::explorer_main_panel(self, ui),
                    AppMode::Dedupe | AppMode::CompareSourceDest => match self.view {
                        AppView::Scan => scan_panel(self, ui),
                        AppView::Scanning => progress_panel(self, ui, ctx),
                        AppView::Results => results_main_panel(self, ui, ctx),
                    },
                    AppMode::EmptyFolders => match self.view {
                        AppView::Scan => crate::ui::empty_folders_scan_panel(self, ui),
                        AppView::Scanning => crate::ui::empty_folders_progress_panel(self, ui, ctx),
                        AppView::Results => crate::ui::empty_folders_results_panel(self, ui),
                    },
                    AppMode::ArchiveFiles => match self.view {
                        AppView::Scan => crate::ui::archive_files_scan_panel(self, ui),
                        AppView::Scanning => crate::ui::archive_files_progress_panel(self, ui, ctx),
                        AppView::Results => crate::ui::archive_files_results_panel(self, ui),
                    },
                    AppMode::AllFiles => crate::ui::recursive_files_panel::recursive_files_page(self, ui),
                }
                // Render global player modal globally so it functions in all modes/views
                crate::ui::folder_contents_modal(self, ctx);
                crate::ui::player_window(self, ctx);
                crate::ui::confirm_pending_action(self, ctx);
            });
    }
}
