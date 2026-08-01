use crate::app::{DupGroupSortOrder, SmartFileManagerApp};
use crate::ui::preview::{
    file_modified_label, format_bytes, is_video_path, load_preview, open_in_default_app,
    reveal_in_folder, PreviewKind,
};
use crate::ui::video_inline::InlineVideoDecoder;
use crate::ui::style::{
    self, muted, panel_card, path_monospace, primary_button, secondary_button, section_title,
    stat_chip, mode_pill, semantic_pill, PillVariant, icon_button, BORDER,
};
use eframe::egui::{self, load::SizedTexture, RichText, ScrollArea, TextureOptions};
use eframe::egui::scroll_area::ScrollBarVisibility;
use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use smart_file_manager::resolver::{self, trash_single_path};
use smart_file_manager::summarize_folders;
use smart_file_manager::types::{self, ActionManifest, ActionMode, DuplicateGroup, ManifestAction, ScanResult};
use std::fs;
use std::path::{Path, PathBuf};

pub fn results_dup_list_panel(
    app: &mut SmartFileManagerApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
) {
    let Some(result) = app.last_result.clone() else {
        ui.label("No results.");
        return;
    };

    ui.label(section_title("Duplicate groups"));
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        stat_chip(ui, "Groups", &format!("{}", result.stats.duplicate_groups));
        stat_chip(ui, "Reclaimable", &format_bytes(result.stats.total_wasted_bytes));
    });
    ui.add_space(8.0);
    ui.add(
        egui::TextEdit::singleline(&mut app.results_filter)
            .hint_text("Filter by path or hash prefix")
            .desired_width(f32::INFINITY),
    );
    if let Some((fa, fb)) = app.results_filter_folders.clone() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let name_a = fa.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
            let name_b = fb.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
            ui.label(RichText::new(format!("Showing mutual duplicates: {} ↔ {}", name_a, name_b)).size(11.0).color(style::TEXT_SEC));
            if ui.small_button("Clear Filter").clicked() {
                app.results_filter_folders = None;
            }
        });
    }
    ui.add_space(2.0);
    ui.label(muted("Up / Down or J / K to move between groups."));

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(muted("Sort:"));
        egui::ComboBox::new(egui::Id::new("dup_group_sort"), "")
            .width(ui.available_width().min(260.0))
            .selected_text(match app.dup_group_sort {
                DupGroupSortOrder::ScanOrder => "Scan order (#1, #2, …)",
                DupGroupSortOrder::WastedBytesDesc => "Reclaimable — largest first",
                DupGroupSortOrder::WastedBytesAsc => "Reclaimable — smallest first",
                DupGroupSortOrder::FileCountDesc => "Copy count — most first",
                DupGroupSortOrder::FileCountAsc => "Copy count — fewest first",
                DupGroupSortOrder::HashAsc => "Hash (A–Z)",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.dup_group_sort,
                    DupGroupSortOrder::ScanOrder,
                    "Scan order (#1, #2, …)",
                );
                ui.selectable_value(
                    &mut app.dup_group_sort,
                    DupGroupSortOrder::WastedBytesDesc,
                    "Reclaimable — largest first",
                );
                ui.selectable_value(
                    &mut app.dup_group_sort,
                    DupGroupSortOrder::WastedBytesAsc,
                    "Reclaimable — smallest first",
                );
                ui.selectable_value(
                    &mut app.dup_group_sort,
                    DupGroupSortOrder::FileCountDesc,
                    "Copy count — most first",
                );
                ui.selectable_value(
                    &mut app.dup_group_sort,
                    DupGroupSortOrder::FileCountAsc,
                    "Copy count — fewest first",
                );
                ui.selectable_value(&mut app.dup_group_sort, DupGroupSortOrder::HashAsc, "Hash (A–Z)");
            });
    });

    let visible = sort_visible_dup_groups(
        filtered_group_indices(&result, &app.results_filter),
        &result,
        app.dup_group_sort,
    );
    sync_selection_with_filter(app, &visible, &result);
    apply_dup_list_keyboard_navigation(ctx, app, &visible, &result);

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    // Find maximum wasted bytes in visible groups for gradient fill normalization
    let max_wasted = visible.iter()
        .map(|&gi| result.duplicate_groups[gi].total_wasted_bytes)
        .max()
        .unwrap_or(1)
        .max(1);

    ScrollArea::vertical()
        .id_salt("dup_group_list")
        .auto_shrink([false, false])
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            if visible.is_empty() {
                ui.label(muted("No groups match filter."));
                return;
            }
            for &gi in &visible {
                let g = &result.duplicate_groups[gi];
                let selected = app.results_selected_group == Some(gi);
                
                let fill_pct = g.total_wasted_bytes as f32 / max_wasted as f32;

                // Group card custom layout
                let card_bg = if selected { style::SURFACE_2 } else { style::SURFACE_1 };
                let stroke_color = if selected { style::ACCENT } else { egui::Color32::from_rgb(50, 50, 60) };

                let response = egui::Frame::none()
                    .fill(card_bg)
                    .stroke(egui::Stroke::new(1.0, stroke_color))
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("Group #{}", gi + 1)).strong().size(13.0).color(style::TEXT_PRI));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(muted(format!("{} files", g.files.len())));
                                });
                            });
                            ui.add_space(4.0);
                            style::gradient_badge(ui, "Reclaimable", &format_bytes(g.total_wasted_bytes), fill_pct);
                        });
                    })
                    .response;

                if app.dup_list_scroll_to_group == Some(gi) {
                    response.scroll_to_me(Some(egui::Align::Center));
                    app.dup_list_scroll_to_group = None;
                }

                let response = response.interact(egui::Sense::click());
                if response.clicked() {
                    app.dup_list_scroll_to_group = None;
                    app.results_selected_group = Some(gi);
                    app.results_selected_file_in_group = g.keep_index;
                    app.clear_preview_cache();
                }
                ui.add_space(6.0);
            }
        });
}

pub fn results_main_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    if app.results_selected_group != app.results_last_selected_group {
        app.results_last_selected_group = app.results_selected_group;
        app.results_selected_files.clear();
    }

    let Some(result) = app.last_result.clone() else {
        ui.label("No results.");
        return;
    };

    action_row(app, ui, &result);
    ui.add_space(10.0);

    if result.stats.partial {
        panel_card(ui, |ui| {
            ui.colored_label(
                style::WARNING,
                "⚠  Partial results — scan was cancelled before completion.",
            );
        });
        ui.add_space(10.0);
    }

    let Some(gi) = app.results_selected_group else {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(muted("Select a duplicate group from the list on the left."));
        });
        return;
    };

    if gi >= result.duplicate_groups.len() {
        ui.label("Invalid selection.");
        return;
    };

    let body_h = (ui.available_height() - 4.0).max(220.0);

    ScrollArea::vertical()
        .max_height(body_h)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
        .id_salt("results_main_scroll")
        .show(ui, |ui| {
            let g = &result.duplicate_groups[gi];
            
            // BIG Media Preview
            if let Some(f) = g.files.get(app.results_selected_file_in_group) {
                let path = &f.path;
                
                // Auto-start video player
                if is_video_path(path) && app.inline_video.is_none() && app.preview_path.as_deref() != Some(path) {
                    update_preview(ctx, app, path, 0);
                } else if app.preview_path.as_deref() != Some(path) {
                    update_preview(ctx, app, path, 0);
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(section_title(&format!("Group #{} — Media Preview", gi + 1)));
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if g.files.len() >= 2 {
                            let best_file = &g.files[g.keep_index];
                            let tooltip = format!("Keep: {} (Score: {})", 
                                best_file.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                                best_file.score);
                                
                            if primary_button(ui, "✨ Accept Suggestion").on_hover_text(tooltip).clicked() {
                                for (idx, f) in g.files.iter().enumerate() {
                                    if idx != g.keep_index {
                                        app.results_selected_files.insert(f.path.clone());
                                    } else {
                                        app.results_selected_files.remove(&f.path);
                                    }
                                }
                            }
                        }
                        
                        ui.add_space(8.0);
                        stat_chip(ui, "Reclaimable", &format_bytes(g.total_wasted_bytes));
                        ui.add_space(4.0);
                        stat_chip(ui, "Copies", &format!("{}", g.files.len()));
                    });
                });
                ui.add_space(6.0);

                // Render custom video controls bar above preview if video is active
                if is_video_path(path) {
                    video_controls_bar(ui, app, path, ctx);
                    ui.add_space(6.0);
                }

                preview_body_ui(ui, app, path, f.size, ctx, 380.0);
            }

            // Folder-grouped File Rows
            ui.add_space(12.0);
            ui.label(section_title("Copies in Group"));
            ui.add_space(6.0);

            // Group files by parent folder
            let mut by_folder: std::collections::BTreeMap<PathBuf, Vec<(usize, &types::FileEntrySnapshot)>> = std::collections::BTreeMap::new();
            for (fi, f) in g.files.iter().enumerate() {
                if let Some(parent) = f.path.parent() {
                    by_folder.entry(parent.to_path_buf()).or_default().push((fi, f));
                }
            }

            for (folder_path, files) in by_folder {
                let display_name = folder_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
                let files_count = files.len();

                // Folder header row
                ui.horizontal(|ui: &mut egui::Ui| {
                    style::folder_chip(ui, &display_name);
                    ui.label(RichText::new(format!("({})", files_count)).size(11.5).color(style::TEXT_TER));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui: &mut egui::Ui| {
                        if ui.small_button("Delete all here").clicked() {
                            let mut errors = Vec::new();
                            let mut moved_count = 0;
                            let mut paths_to_remove = std::collections::HashSet::new();

                            let yes = MessageDialog::new()
                                .set_level(MessageLevel::Warning)
                                .set_title("Delete folder copies")
                                .set_description(&format!("Move all duplicate copies in {} to trash?", folder_path.display()))
                                .set_buttons(MessageButtons::YesNo)
                                .show();

                            if yes == MessageDialogResult::Yes {
                                for (_, f) in &files {
                                    match trash_single_path(&f.path) {
                                        Ok(()) => {
                                            paths_to_remove.insert(f.path.clone());
                                            moved_count += 1;
                                        }
                                        Err(e) => { errors.push(e.to_string()); }
                                    }
                                }
                                if moved_count > 0 {
                                    prune_paths_from_results(app, &paths_to_remove);
                                    app.clear_preview_cache();
                                    app.apply_message = Some(format!("Removed {} file(s) from folder.", moved_count));
                                }
                                if !errors.is_empty() {
                                    app.apply_errors = Some(errors.join("; "));
                                }
                            }
                        }
                    });
                });
                ui.add_space(4.0);

                // File rows for this folder
                for (fi, f) in files {
                    let sel = app.results_selected_file_in_group == fi;
                    let row_bg = if sel { style::SURFACE_2 } else { egui::Color32::from_rgb(20, 20, 24) };

                    let clicked = egui::Frame::none()
                        .fill(row_bg)
                        .stroke(egui::Stroke::new(1.0, BORDER))
                        .rounding(egui::Rounding::same(6.0))
                        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                        .show(ui, |ui: &mut egui::Ui| {
                            ui.horizontal(|ui: &mut egui::Ui| {
                                let mut checked = app.results_selected_files.contains(&f.path);
                                if ui.checkbox(&mut checked, "").changed() {
                                    if checked {
                                        app.results_selected_files.insert(f.path.clone());
                                    } else {
                                        app.results_selected_files.remove(&f.path);
                                    }
                                }

                                if f.is_keep {
                                    semantic_pill(ui, PillVariant::Keep);
                                } else {
                                    semantic_pill(ui, PillVariant::Dup);
                                }

                                let name = f.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                let path_clicked = ui.selectable_label(sel, RichText::new(name).size(12.0).color(style::TEXT_PRI))
                                    .on_hover_text(f.path.display().to_string())
                                    .clicked();

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui: &mut egui::Ui| {
                                    if ui.small_button("Reveal").clicked() {
                                        reveal_in_folder(&f.path);
                                    }
                                    ui.label(muted(format!("score {}", f.score)));
                                });

                                path_clicked
                            })
                            .inner
                        })
                        .inner;

                    if clicked {
                        app.results_selected_file_in_group = fi;
                        app.clear_preview_cache();
                    }
                    ui.add_space(2.0);
                }
                ui.add_space(4.0);
            }

            // Collapsible folder overlap intelligence
            crate::ui::folder_intelligence::folder_intel_section(ui, app, &result, gi);
        });

    // Modal view triggers
    crate::ui::folder_intelligence::folder_twins_modal(ui, ctx, app, &result);

    if app.show_error_log {
        egui::Window::new("Error log")
            .open(&mut app.show_error_log)
            .default_size([560.0, 400.0])
            .show(ctx, |ui| {
                ScrollArea::both()
                    .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (p, e) in &result.error_log {
                            ui.monospace(format!("{} — {}", p.display(), e));
                        }
                    });
            });
    }

    if app.apply_message.is_some() || app.apply_errors.is_some() {
        ui.add_space(8.0);
        panel_card(ui, |ui| {
            if let Some(ref m) = app.apply_message {
                ui.label(m);
            }
            if let Some(ref e) = app.apply_errors {
                ui.colored_label(style::DANGER, e);
            }
        });
    }
}

fn video_controls_bar(ui: &mut egui::Ui, app: &mut SmartFileManagerApp, path: &Path, _ctx: &egui::Context) {
    ui.horizontal(|ui| {
        // Play / Pause
        let is_playing = app.inline_video.as_ref().map(|v| !v.is_paused()).unwrap_or(false);
        let icon = if is_playing { "⏸" } else { "▶" };
        if icon_button(ui, icon, is_playing).clicked() {
            if let Some(ref v) = app.inline_video {
                v.toggle_pause();
            }
        }

        // Scrub Slider
        let pos = app.inline_video.as_ref().map(|v| v.position()).unwrap_or(0.0);
        let dur = app.inline_video.as_ref().map(|v| v.duration()).unwrap_or(0.0);
        
        let mut scrub_val = pos as f32;
        let slider = egui::Slider::new(&mut scrub_val, 0.0..=(dur as f32))
            .show_value(false)
            .trailing_fill(true);
            
        let slider_resp = ui.add(slider);
        if slider_resp.drag_stopped() {
            // Seek trigger via spawn_at
            if let Ok(new_dec) = InlineVideoDecoder::spawn_at(path, scrub_val as f64) {
                app.inline_video = Some(new_dec);
            }
        }

        // Timer label
        let fmt_secs = |s: f64| {
            let m = (s / 60.0) as u32;
            let sec = (s % 60.0) as u32;
            format!("{m:02}:{sec:02}")
        };
        ui.label(RichText::new(format!("{} / {}", fmt_secs(pos), fmt_secs(dur))).size(11.0).color(style::TEXT_SEC));

        // Volume control trigger (persisted state value)
        let vol_icon = if app.video_volume == 0.0 { "🔇" } else { "🔊" };
        if icon_button(ui, vol_icon, app.video_volume > 0.0).clicked() {
            app.video_volume = if app.video_volume == 0.0 { 100.0 } else { 0.0 };
        }

        // Fullscreen Modal trigger
        if icon_button(ui, "⤢", false).clicked() {
            app.explorer_open_preview(path.to_path_buf());
        }
    });
}

fn action_row(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, result: &ScanResult) {
    ui.horizontal(|ui| {
        if primary_button(ui, "＋ New scan").clicked() {
            app.view = crate::app::AppView::Scan;
            app.last_result = None;
            app.results_selected_group = None;
            app.clear_preview_cache();
        }
        if app.action_mode != ActionMode::DryRun
            && primary_button(ui, "Apply actions").clicked()
        {
            apply_all(app, result);
        }
        
        if secondary_button(ui, "Refresh findings").clicked() {
            app.start_refresh_findings();
        }

        // Export Dropdown
        egui::menu::menu_button(ui, "📤 Export ▾", |ui| {
            if ui.button("Export JSON").clicked() {
                export_json(result);
                ui.close_menu();
            }
            if ui.button("Export manifest").clicked() {
                export_manifest(app, result);
                ui.close_menu();
            }
        });

        // Smart Clean Dropdown
        egui::menu::menu_button(ui, "🗑 Smart Clean ▾", |ui| {
            if ui.button("Clean Same-Folder Duplicates").clicked() {
                clean_all_same_folder_duplicates(app, result);
                ui.close_menu();
            }
            if ui.button("Folder Twins Analysis…").clicked() {
                app.dedupe_twins_modal_open = true;
                ui.close_menu();
            }
        });

        if !result.error_log.is_empty()
            && secondary_button(ui, &format!("Errors ({})", result.error_log.len())).clicked()
        {
            app.show_error_log = !app.show_error_log;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            mode_pill(ui, app.action_mode);
        });
    });
}

fn sort_visible_dup_groups(
    mut indices: Vec<usize>,
    result: &ScanResult,
    order: DupGroupSortOrder,
) -> Vec<usize> {
    match order {
        DupGroupSortOrder::ScanOrder => {
            indices.sort_by_key(|&i| i);
        }
        DupGroupSortOrder::WastedBytesDesc => {
            indices.sort_by(|&a, &b| {
                result.duplicate_groups[b]
                    .total_wasted_bytes
                    .cmp(&result.duplicate_groups[a].total_wasted_bytes)
                    .then_with(|| a.cmp(&b))
            });
        }
        DupGroupSortOrder::WastedBytesAsc => {
            indices.sort_by(|&a, &b| {
                result.duplicate_groups[a]
                    .total_wasted_bytes
                    .cmp(&result.duplicate_groups[b].total_wasted_bytes)
                    .then_with(|| a.cmp(&b))
            });
        }
        DupGroupSortOrder::FileCountDesc => {
            indices.sort_by(|&a, &b| {
                result.duplicate_groups[b]
                    .files
                    .len()
                    .cmp(&result.duplicate_groups[a].files.len())
                    .then_with(|| a.cmp(&b))
            });
        }
        DupGroupSortOrder::FileCountAsc => {
            indices.sort_by(|&a, &b| {
                result.duplicate_groups[a]
                    .files
                    .len()
                    .cmp(&result.duplicate_groups[b].files.len())
                    .then_with(|| a.cmp(&b))
            });
        }
        DupGroupSortOrder::HashAsc => {
            indices.sort_by(|&a, &b| {
                result.duplicate_groups[a]
                    .hash
                    .cmp(&result.duplicate_groups[b].hash)
                    .then_with(|| a.cmp(&b))
            });
        }
    }
    indices
}

fn apply_dup_list_keyboard_navigation(
    ctx: &egui::Context,
    app: &mut SmartFileManagerApp,
    visible: &[usize],
    result: &ScanResult,
) {
    if visible.is_empty() {
        return;
    }
    if ctx.wants_keyboard_input() {
        return;
    }
    let down =
        ctx.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J));
    let up = ctx.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K));
    if !down && !up {
        return;
    }
    let cur_pos = visible
        .iter()
        .position(|&gi| app.results_selected_group == Some(gi));
    let idx = cur_pos.unwrap_or(0);
    let new_pos = if down {
        (idx + 1).min(visible.len() - 1)
    } else {
        idx.saturating_sub(1)
    };
    if new_pos == idx {
        return;
    }
    let gi = visible[new_pos];
    app.results_selected_group = Some(gi);
    app.results_selected_file_in_group = result.duplicate_groups[gi].keep_index;
    app.dup_list_scroll_to_group = Some(gi);
    app.clear_preview_cache();
}

fn filtered_group_indices(result: &ScanResult, filter: &str) -> Vec<usize> {
    let t = filter.trim().to_lowercase();
    if t.is_empty() {
        return (0..result.duplicate_groups.len()).collect();
    }
    result
        .duplicate_groups
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            if g.hash.to_lowercase().starts_with(&t) || g.hash.to_lowercase().contains(&t) {
                return Some(i);
            }
            if g
                .files
                .iter()
                .any(|f| f.path.display().to_string().to_lowercase().contains(&t))
            {
                return Some(i);
            }
            None
        })
        .collect()
}

fn sync_selection_with_filter(
    app: &mut SmartFileManagerApp,
    visible: &[usize],
    result: &ScanResult,
) {
    if visible.is_empty() {
        app.results_selected_group = None;
        app.clear_preview_cache();
        return;
    }
    let sel = app.results_selected_group;
    let ok = sel.map(|s| visible.contains(&s)).unwrap_or(false);
    if !ok {
        app.results_selected_group = Some(visible[0]);
        if let Some(g) = result.duplicate_groups.get(visible[0]) {
            app.results_selected_file_in_group = g.keep_index;
        }
        app.clear_preview_cache();
    }
}

pub(crate) fn update_preview(ctx: &egui::Context, app: &mut SmartFileManagerApp, path: &Path, rotation: i32) {
    if app.preview_path.as_deref() == Some(path) && app.preview_rotation == rotation {
        return;
    }
    app.preview_path = None;
    app.preview_texture = None;
    app.preview_scroll_text = None;
    app.preview_subtitle = None;
    app.preview_path = Some(path.to_path_buf());
    app.preview_rotation = rotation;

    match load_preview(path) {
        PreviewKind::Empty => {
            app.preview_subtitle = Some("Empty file".into());
        }
        PreviewKind::Text { content, truncated } => {
            let mut s = content;
            if truncated {
                s.push_str("\n\n… Truncated (first 64 KiB) …");
            }
            app.preview_scroll_text = Some(s);
            app.preview_subtitle = Some("Text preview".into());
        }
        PreviewKind::Hex(h) => {
            app.preview_scroll_text = Some(h);
            app.preview_subtitle = Some("Hex (first 256 bytes)".into());
        }
        PreviewKind::Image(ci) => {
            let id = format!("sfm_preview_{}", path.display());
            app.preview_texture = Some(ctx.load_texture(id, ci, TextureOptions::LINEAR));
            app.preview_subtitle = Some(
                "Image preview (scroll if larger than the viewport).".into(),
            );
        }
        PreviewKind::VideoPlaceholder => {
            app.preview_subtitle = Some("Starting inline video (ffmpeg, muted)…".into());
            match InlineVideoDecoder::try_spawn(path) {
                Ok(dec) => {
                    let (w, h) = dec.dimensions();
                    app.inline_video = Some(dec);
                    app.preview_subtitle =
                        Some(format!("Inline video {w}×{h} (muted, ~30 fps). Stop ends playback."));
                }
                Err(e) => {
                    app.preview_scroll_text = Some(format!(
                        "{e}\n\nTip: install ffmpeg and ensure it is on PATH, or use “Open in default app”."
                    ));
                    app.preview_subtitle = Some("Inline video unavailable.".into());
                }
            }
        }
        PreviewKind::SkippedLarge { size, reason } => {
            app.preview_scroll_text = Some(format!(
                "{reason}\n\nFile size: {}",
                format_bytes(size)
            ));
            app.preview_subtitle = Some("Preview skipped".into());
        }
        PreviewKind::IoError(e) => {
            app.preview_scroll_text = Some(e);
            app.preview_subtitle = Some("Could not read file".into());
        }
    }
}

pub(crate) fn preview_body_ui(
    ui: &mut egui::Ui,
    app: &mut SmartFileManagerApp,
    path: &Path,
    size: u64,
    ctx: &egui::Context,
    preview_media_max_h: f32,
) {
    panel_card(ui, |ui| {
        let path_w = (ui.available_width() - 8.0).max(120.0);
        ScrollArea::horizontal()
            .max_width(path_w)
            .min_scrolled_height(22.0)
            .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
            .id_salt("preview_path_bar")
            .show(ui, |ui| {
                ui.label(path_monospace(path));
            });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if secondary_button(ui, "Reveal in folder").clicked() {
                reveal_in_folder(path);
            }
            if secondary_button(ui, "Open in default app").clicked() {
                open_in_default_app(path);
            }
            if is_video_path(path) {
                if app.inline_video.is_some() {
                    if secondary_button(ui, "Stop playback").clicked() {
                        app.inline_video = None;
                        app.preview_texture = None;
                        app.preview_subtitle = Some(
                            "Video playback stopped. Use “Start inline preview” to replay.".into(),
                        );
                    }
                } else if secondary_button(ui, "Start inline preview").clicked() {
                    app.preview_path = None;
                }
            }
            if secondary_button(ui, "Move to Trash…").clicked() {
                let path_owned = path.to_path_buf();
                let yes = MessageDialog::new()
                    .set_level(MessageLevel::Warning)
                    .set_title("Move to Trash")
                    .set_description(format!(
                        "Move this file to Trash?\n\n{}",
                        path.display()
                    ))
                    .set_buttons(MessageButtons::YesNo)
                    .show();
                if yes == MessageDialogResult::Yes {
                    match trash_single_path(&path_owned) {
                        Ok(()) => {
                            prune_path_from_results(app, &path_owned);
                            app.clear_preview_cache();
                            app.apply_message =
                                Some("Selected file moved to Trash. Duplicate list updated.".into());
                            app.apply_errors = None;
                        }
                        Err(e) => {
                            app.apply_errors = Some(e.to_string());
                        }
                    }
                }
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            stat_chip(ui, "Size", &format_bytes(size));
            ui.add_space(8.0);
            stat_chip(ui, "Modified", &file_modified_label(path));
        });

        if let Some(ref sub) = app.preview_subtitle {
            ui.add_space(6.0);
            ui.label(muted(sub.clone()));
        }

        if app.inline_video.is_some() || app.preview_texture.is_some() {
            ui.add_space(8.0);
            if let Some(dec) = app.inline_video.as_ref() {
                let mut latest: Option<eframe::egui::ColorImage> = None;
                while let Some(f) = dec.pop_frame() {
                    latest = Some(f);
                }
                if let Some(img) = latest {
                    match app.preview_texture.as_mut() {
                        None => {
                            app.preview_texture = Some(
                                ctx.load_texture("sfm_inline_video", img, TextureOptions::LINEAR),
                            );
                        }
                        Some(t) => t.set(img, TextureOptions::LINEAR),
                    }
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(32));
            }
            if let Some(tex) = app.preview_texture.clone() {
                let avail_w = (ui.available_width() - 4.0).max(120.0);
                let [tw, th] = tex.size();
                let (tw, th) = (tw as f32, th as f32);
                let scale = (avail_w / tw)
                    .min(preview_media_max_h / th)
                    .min(1.5);
                let size = egui::vec2(tw * scale, th * scale);
                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(12, 12, 16))
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(6.0))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            let img_widget = egui::Image::from_texture(SizedTexture::from_handle(&tex))
                                .fit_to_exact_size(size)
                                .sense(egui::Sense::click());
                            let img_resp = ui.add(img_widget);
                            if img_resp.on_hover_text("🔍 Click to open in full-screen player modal").clicked() {
                                app.explorer_open_preview(path.to_path_buf());
                            }
                        });
                    });
            }
        }

        if let Some(text) = &app.preview_scroll_text {
            ui.add_space(8.0);
            ScrollArea::both()
                .max_height(220.0)
                .max_width(ui.available_width())
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                .auto_shrink([false, false])
                .id_salt("preview_text_scroll")
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(text.as_str()).font(egui::FontId::monospace(12.0)),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                });
        }
    });
}

fn prune_path_from_results(app: &mut SmartFileManagerApp, removed: &Path) {
    let mut removed_paths = std::collections::HashSet::new();
    removed_paths.insert(removed.to_path_buf());
    prune_paths_from_results(app, &removed_paths);
}

pub(crate) fn prune_paths_from_results(
    app: &mut SmartFileManagerApp, removed_paths: &std::collections::HashSet<std::path::PathBuf>) {
    let Some(res) = app.last_result.as_mut() else {
        return;
    };

    let pointed_at_removed = app
        .results_selected_group
        .and_then(|gi| res.duplicate_groups.get(gi))
        .and_then(|g| g.files.get(app.results_selected_file_in_group))
        .map(|f| removed_paths.contains(&f.path))
        .unwrap_or(false);

    let mut next: Vec<DuplicateGroup> = Vec::new();
    for mut g in std::mem::take(&mut res.duplicate_groups) {
        g.files.retain(|f| !removed_paths.contains(&f.path));
        if g.files.len() < 2 {
            continue;
        }
        if g.keep_index >= g.files.len() {
            g.keep_index = 0;
        }
        for (i, f) in g.files.iter_mut().enumerate() {
            f.is_keep = i == g.keep_index;
        }
        let total: u64 = g.files.iter().map(|f| f.size).sum();
        let keep_sz = g.files[g.keep_index].size;
        g.total_wasted_bytes = total.saturating_sub(keep_sz);
        next.push(g);
    }
    res.duplicate_groups = next;
    res.stats.duplicate_groups = res.duplicate_groups.len() as u64;
    res.stats.total_wasted_bytes = res
        .duplicate_groups
        .iter()
        .map(|g| g.total_wasted_bytes)
        .sum();
    res.cross_folder_conflicts = resolver::detect_cross_folder_conflicts(&res.duplicate_groups);
    res.folder_summaries = summarize_folders(&res.duplicate_groups, &app.roots);

    if let Some(r) = app.last_result.as_ref() {
        if r.duplicate_groups.is_empty() {
            app.results_selected_group = None;
            app.results_selected_file_in_group = 0;
        } else if pointed_at_removed {
            app.results_selected_group = Some(0);
            let g0 = &r.duplicate_groups[0];
            app.results_selected_file_in_group = g0.keep_index.min(g0.files.len().saturating_sub(1));
        } else {
            let mut gi = app.results_selected_group.unwrap_or(0);
            if gi >= r.duplicate_groups.len() {
                gi = 0;
                app.results_selected_group = Some(0);
            } else {
                app.results_selected_group = Some(gi);
            }
            let g = &r.duplicate_groups[gi];
            let mut fi = app.results_selected_file_in_group;
            if fi >= g.files.len() {
                fi = g.keep_index.min(g.files.len().saturating_sub(1));
            }
            app.results_selected_file_in_group = fi;
        }

        if let Some(r) = app.last_result.as_ref() {
            app.folder_overlap = crate::ui::folder_intelligence::compute_folder_overlap(r);
        } else {
            app.folder_overlap.clear();
        }
    }
}

fn export_json(result: &ScanResult) {
    if let Some(path) = rfd::FileDialog::new().save_file() {
        let errors: Vec<_> = result
            .error_log
            .iter()
            .take(500)
            .map(|(p, e)| serde_json::json!({ "path": p.display().to_string(), "error": e }))
            .collect();
        let report = serde_json::json!({
            "stats": result.stats,
            "duplicate_groups": result.duplicate_groups,
            "folder_summaries": result.folder_summaries,
            "errors": errors,
        });
        let _ = fs::write(path, serde_json::to_string_pretty(&report).unwrap_or_default());
    }
}

fn export_manifest(app: &SmartFileManagerApp, result: &ScanResult) {
    if let Some(path) = rfd::FileDialog::new().save_file() {
        let mut actions = Vec::new();
        for g in &result.duplicate_groups {
            let keep = &g.files[g.keep_index].path;
            for (i, f) in g.files.iter().enumerate() {
                if i == g.keep_index {
                    continue;
                }
                let op = match app.action_mode {
                    ActionMode::DryRun => "dry_run_skip",
                    ActionMode::Delete => "delete",
                    ActionMode::HardLink => "hardlink",
                };
                actions.push(ManifestAction {
                    op: op.to_string(),
                    path: f.path.clone(),
                    keep_path: Some(keep.clone()),
                });
            }
        }
        let manifest = ActionManifest { actions };
        let _ = fs::write(
            path,
            serde_json::to_string_pretty(&manifest).unwrap_or_default(),
        );
    }
}

fn apply_all(app: &mut SmartFileManagerApp, result: &ScanResult) {
    app.apply_message = None;
    app.apply_errors = None;
    let mut errs = Vec::new();
    let mut ok = 0usize;
    match app.action_mode {
        ActionMode::DryRun => {
            app.apply_message = Some("Dry run: no changes applied.".into());
            return;
        }
        ActionMode::Delete => {
            for g in &result.duplicate_groups {
                for r in resolver::apply_delete_group(g, ActionMode::Delete) {
                    match r {
                        Ok(()) => ok += 1,
                        Err(e) => errs.push(e.to_string()),
                    }
                }
            }
        }
        ActionMode::HardLink => {
            for g in &result.duplicate_groups {
                for r in resolver::apply_hardlink_group(g, ActionMode::HardLink) {
                    match r {
                        Ok(()) => ok += 1,
                        Err(e) => errs.push(e.to_string()),
                    }
                }
            }
        }
    }
    app.apply_message = Some(format!("Applied {ok} operations."));
    if !errs.is_empty() {
        app.apply_errors = Some(errs.join("\n"));
    }
}

fn clean_all_same_folder_duplicates(app: &mut SmartFileManagerApp, result: &ScanResult) {
    let mut files_to_trash = Vec::new();

    // First pass: identify exactly which files will be trashed
    for g in &result.duplicate_groups {
        let mut by_folder: std::collections::HashMap<std::path::PathBuf, Vec<&smart_file_manager::types::FileEntrySnapshot>> = std::collections::HashMap::new();
        for f in &g.files {
            if let Some(parent) = f.path.parent() {
                by_folder.entry(parent.to_path_buf()).or_default().push(f);
            }
        }

        for (_, mut files_in_folder) in by_folder {
            if files_in_folder.len() > 1 {
                files_in_folder.sort_by_key(|f| {
                    let is_keep_score = if f.is_keep { 0 } else { 1 };
                    let name_len = f.path.file_name().map(|n| n.len()).unwrap_or(usize::MAX);
                    (is_keep_score, name_len)
                });

                for f in files_in_folder.into_iter().skip(1) {
                    files_to_trash.push(f.clone());
                }
            }
        }
    }

    if files_to_trash.is_empty() {
        app.apply_message = Some("No same-folder duplicates found.".to_string());
        return;
    }

    let total_freed_bytes: u64 = files_to_trash.iter().map(|f| f.size).sum();

    let yes = MessageDialog::new()
        .set_level(MessageLevel::Warning)
        .set_title("Clean Same-Folder Duplicates")
        .set_description(&format!("This will move {} duplicate files to the Trash, freeing approximately {}.\n\nIt keeps exactly one copy per folder, leaving cross-folder duplicates untouched.\n\nProceed?", files_to_trash.len(), format_bytes(total_freed_bytes)))
        .set_buttons(MessageButtons::YesNo)
        .show();

    if yes == MessageDialogResult::Yes {
        let mut paths_to_remove = std::collections::HashSet::new();
        let mut moved_count = 0;
        let mut errors = Vec::new();

        for f in files_to_trash {
            match smart_file_manager::resolver::trash_single_path(&f.path) {
                Ok(()) => {
                    paths_to_remove.insert(f.path.clone());
                    moved_count += 1;
                }
                Err(e) => {
                    errors.push(e.to_string());
                }
            }
        }

        if moved_count > 0 {
            prune_paths_from_results(app, &paths_to_remove);
            app.clear_preview_cache();
            app.apply_message = Some(format!("{} same-folder duplicate(s) moved to Trash.", moved_count));
        }
        
        if !errors.is_empty() {
            app.apply_errors = Some(format!("Failed to trash some files: {}", errors.join("; ")));
        } else if moved_count > 0 {
            app.apply_errors = None;
        }
    }
}
