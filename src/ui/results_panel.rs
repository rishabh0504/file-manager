use crate::app::{DupGroupSortOrder, DupListCache, PendingAction, SortwiseApp};
use crate::ui::preview::{
    file_modified_label, format_bytes, is_privacy_denied, is_video_path, load_preview, open_in_default_app,
    open_privacy_settings, reveal_in_folder, PreviewKind,
};
use crate::ui::video_inline::InlineVideoDecoder;
use sortwise::heuristics::{media_kind, MediaKind};
use crate::ui::style::{
    self, muted, panel_card, path_monospace, primary_button, secondary_button, section_title,
    stat_chip, mode_pill, semantic_pill, PillVariant, icon_button,
};
use eframe::egui::{self, load::SizedTexture, RichText, ScrollArea, TextureOptions};
use eframe::egui::scroll_area::ScrollBarVisibility;
use sortwise::action_log::{self, ActionLogEntry};
use sortwise::action_plan::{self, ActionPlan};
use sortwise::resolver::{self, trash_batch};
use sortwise::summarize_folders;
use sortwise::types::{self, ActionManifest, ActionMode, DuplicateGroup, ManifestAction, ScanResult};
use std::fs;
use std::path::{Path, PathBuf};

/// Height of one group card in the list, including the gap below it (P-2).
/// Cards use a fixed content height so `show_rows` can position them without measuring.
const DUP_ROW_H: f32 = 54.0;
const DUP_CARD_CONTENT_H: f32 = 36.0;

pub fn results_dup_list_panel(
    app: &mut SortwiseApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
) {
    let Some(result) = app.last_result.clone() else {
        ui.label("No results.");
        return;
    };

    ui.horizontal(|ui| {
        ui.label(section_title("Groups"));
        style::pill(ui, &format!("{}", result.stats.duplicate_groups), style::pal().text_muted, style::pal().surface_raised);
    });
    ui.label(muted(format!("{} reclaimable", format_bytes(result.stats.total_wasted_bytes))));
    ui.add_space(10.0);
    ui.add(
        egui::TextEdit::singleline(&mut app.results_filter)
            .hint_text(format!("{}  Filter by path or hash", crate::ui::icons::Icons::SEARCH))
            .margin(egui::Margin::symmetric(8.0, 6.0))
            .desired_width(f32::INFINITY),
    );
    if let Some((fa, fb)) = app.results_filter_folders.clone() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let name_a = fa.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
            let name_b = fb.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".into());
            ui.label(RichText::new(format!("Showing mutual duplicates: {} ↔ {}", name_a, name_b)).size(11.0).color(crate::ui::style::pal().text_muted));
            if ui.small_button("Clear Filter").clicked() {
                app.results_filter_folders = None;
            }
        });
    }
    ui.add_space(8.0);
    // Wraps so the chips never push the sidebar wider than its slot.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.spacing_mut().button_padding = egui::vec2(7.0, 2.0);
        let kinds = [
            None,
            Some(MediaKind::Image),
            Some(MediaKind::Video),
            Some(MediaKind::Audio),
            Some(MediaKind::Archive),
        ];
        for kind in kinds {
            let label = kind.map_or("All", MediaKind::label);
            if style::chip(ui, label, app.results_kind_filter == kind).clicked() {
                app.results_kind_filter = kind;
            }
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(muted("Sort"));
        egui::ComboBox::new(egui::Id::new("dup_group_sort"), "")
            // Fixed width: available_width() here is the panel's max on the first frame and would widen it.
            .width(190.0)
            .selected_text(match app.dup_group_sort {
                DupGroupSortOrder::ScanOrder => "Scan order (#1, #2, …)",
                DupGroupSortOrder::WastedBytesDesc => "Reclaimable — largest first",
                DupGroupSortOrder::WastedBytesAsc => "Reclaimable — smallest first",
                DupGroupSortOrder::FileCountDesc => "Copy count — most first",
                DupGroupSortOrder::FileCountAsc => "Copy count — fewest first",
                DupGroupSortOrder::HashAsc => "Hash (A–Z)",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.dup_group_sort, DupGroupSortOrder::ScanOrder, "Scan order (#1, #2, …)");
                ui.selectable_value(&mut app.dup_group_sort, DupGroupSortOrder::WastedBytesDesc, "Reclaimable — largest first");
                ui.selectable_value(&mut app.dup_group_sort, DupGroupSortOrder::WastedBytesAsc, "Reclaimable — smallest first");
                ui.selectable_value(&mut app.dup_group_sort, DupGroupSortOrder::FileCountDesc, "Copy count — most first");
                ui.selectable_value(&mut app.dup_group_sort, DupGroupSortOrder::FileCountAsc, "Copy count — fewest first");
                ui.selectable_value(&mut app.dup_group_sort, DupGroupSortOrder::HashAsc, "Hash (A–Z)");
            });
    });

    // Memoized visible list (P-3). Taken out of `app` so the list can borrow it while
    // the rest of `app` is mutated (selection sync, keyboard, scroll).
    let mut cache = std::mem::take(&mut app.dup_list_cache);
    refresh_dup_list_cache(
        &mut cache,
        app.results_epoch,
        &app.results_filter,
        app.dup_group_sort,
        app.results_kind_filter,
        &result,
    );
    sync_selection_with_filter(app, &cache.visible, &result);
    apply_dup_list_keyboard_navigation(ctx, app, &cache.visible, &result);

    ui.add_space(4.0);
    ui.label(RichText::new("↑ ↓ or J / K to move between groups").size(11.0).color(style::pal().text_muted));
    ui.add_space(8.0);

    // Scroll requests (keyboard, selection) become an offset, since off-screen rows are not laid out.
    let mut scroll = ScrollArea::vertical()
        .id_salt("dup_group_list")
        .auto_shrink([false, false])
        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded);
    if let Some(row) = app
        .dup_list_scroll_to_group
        .and_then(|gi| cache.visible.iter().position(|&v| v == gi))
    {
        scroll = scroll.vertical_scroll_offset(row as f32 * DUP_ROW_H);
    }
    app.dup_list_scroll_to_group = None;

    if cache.visible.is_empty() {
        ui.label(muted("No groups match filter."));
    } else {
        scroll.show_rows(ui, DUP_ROW_H, cache.visible.len(), |ui, rows| {
            for row in rows {
                draw_dup_group_card(app, ui, &result, cache.visible[row]);
            }
        });
    }

    app.dup_list_cache = cache;
}

/// Rebuild the visible group list only when the result, filter or sort changed.
fn refresh_dup_list_cache(
    cache: &mut DupListCache,
    epoch: u64,
    filter: &str,
    sort: DupGroupSortOrder,
    kind: Option<MediaKind>,
    result: &ScanResult,
) {
    let fresh = cache
        .key
        .as_ref()
        .is_some_and(|(e, f, s, k)| *e == epoch && f == filter && *s == sort && *k == kind);
    if fresh {
        return;
    }
    let visible = sort_visible_dup_groups(filtered_group_indices(result, filter, kind), result, sort);
    cache.max_wasted = visible
        .iter()
        .map(|&gi| result.duplicate_groups[gi].total_wasted_bytes)
        .max()
        .unwrap_or(1)
        .max(1);
    cache.visible = visible;
    cache.key = Some((epoch, filter.to_string(), sort, kind));
}

fn draw_dup_group_card(
    app: &mut SortwiseApp,
    ui: &mut egui::Ui,
    result: &ScanResult,
    gi: usize,
) {
    let g = &result.duplicate_groups[gi];
    let selected = app.results_selected_group == Some(gi);
    let p = style::pal();

    let frame = egui::Frame::none()
        .fill(if selected { p.accent_soft } else { p.surface })
        .stroke(egui::Stroke::new(1.0, if selected { p.accent } else { p.border }))
        .rounding(egui::Rounding::same(style::R_MD))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0));
    let response = frame
        .show(ui, |ui| {
            ui.set_min_height(DUP_CARD_CONTENT_H);
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    let title = if selected { p.accent_on_soft } else { p.text };
                    ui.label(RichText::new(format!("Group #{}", gi + 1)).strong().size(13.0).color(title));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format_bytes(g.total_wasted_bytes)).strong().size(12.5).color(p.text));
                    });
                });
                ui.label(muted(format!("{} files", g.files.len())));
            });
        })
        .response;

    let response = response.interact(egui::Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        app.results_selected_group = Some(gi);
        app.results_selected_file_in_group = g.keep_index;
        app.clear_preview_cache();
    }
    ui.add_space(6.0);
}

pub fn results_main_panel(app: &mut SortwiseApp, ui: &mut egui::Ui, ctx: &egui::Context) {
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

    let denied = result.error_log.iter().filter(|(_, e)| is_privacy_denied(e)).count();
    if denied > 0 {
        panel_card(ui, |ui| {
            ui.colored_label(
                crate::ui::style::pal().warning,
                format!("\u{E4E0}  {denied} path(s) were blocked by macOS privacy settings, so duplicates there may be missed."),
            );
            if secondary_button(ui, "Open Privacy Settings").clicked() {
                if let Err(e) = open_privacy_settings() {
                    app.apply_errors = Some(format!("Could not open Privacy Settings: {e}"));
                }
            }
        });
        ui.add_space(10.0);
    }

    if app.result_rx.is_some() {
        panel_card(ui, |ui| {
            ui.colored_label(crate::ui::style::pal().accent, "Scan still running. Showing groups found so far; the full list appears when it finishes.");
        });
        ui.add_space(10.0);
    }

    if result.stats.partial {
        panel_card(ui, |ui| {
            ui.colored_label(
                crate::ui::style::pal().warning,
                "\u{E4E0}  Partial results — scan was cancelled before completion.",
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

    group_keyboard_shortcuts(app, ctx, gi, &result);
    shortcuts_window(app, ctx);
    let compare_mode = app.mode == crate::app::AppMode::CompareSourceDest;

    // Room for the fixed bottom action bar (56 px + gap) below the scroll body.
    let body_h = (ui.available_height() - 4.0 - GROUP_ACTION_BAR_H).max(220.0);

    ScrollArea::vertical()
        .max_height(body_h)
        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
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
                ui.horizontal_wrapped(|ui| {
                    ui.label(section_title(&format!("Group #{}", gi + 1)));
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(section_title("Copies in Group"));
                if !g.keep_reason.is_empty() {
                    ui.label(muted(format!("Keep: {}", g.keep_reason)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if secondary_button(ui, "Trash all copies except keep").on_hover_text("Move every non-keep copy in this group to Trash. The keep file stays.").clicked() {
                        let plan = action_plan::for_group(gi, g);
                        if plan.is_empty() {
                            app.apply_message = Some("No other copies to delete.".into());
                        } else {
                            app.pending_action = Some(PendingAction::Trash {
                                title: "Delete duplicates in group".into(),
                                action: "delete_group",
                                plan,
                            });
                        }
                    }
                });
            });
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
                    ui.label(RichText::new(format!("({})", files_count)).size(11.5).color(crate::ui::style::pal().text_muted));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui: &mut egui::Ui| {
                        if ui.small_button("Delete all here").on_hover_text("Move non-keep copies in this folder to Trash").clicked() {
                            let plan = action_plan::for_folder(gi, g, &folder_path);
                            if plan.is_empty() {
                                app.apply_message = Some("No other copies in this folder.".into());
                            } else {
                                app.pending_action = Some(PendingAction::Trash {
                                    title: format!("Delete copies in {}", folder_path.display()),
                                    action: "delete_folder",
                                    plan,
                                });
                            }
                        }
                    });
                });
                ui.add_space(4.0);

                // File rows for this folder
                for (fi, f) in files {
                    let sel = app.results_selected_file_in_group == fi;
                    let row_bg = if sel { crate::ui::style::pal().surface_raised } else { crate::ui::style::pal().canvas };

                    let mut file_to_trash_now: Option<PathBuf> = None;

                    let clicked = egui::Frame::none()
                        .fill(row_bg)
                        .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
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
                                let path_clicked = ui.selectable_label(sel, RichText::new(name).size(12.0).color(crate::ui::style::pal().text))
                                    .on_hover_text(f.path.display().to_string())
                                    .clicked();

                                // Compare: every B copy names the A reference it matches (the keep file).
                                if compare_mode && !f.is_keep {
                                    let reference = &g.files[g.keep_index].path;
                                    let reference_name = reference.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                    ui.label(muted(format!("In A: {reference_name}")))
                                        .on_hover_text(reference.display().to_string());
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui: &mut egui::Ui| {
                                    if !f.is_keep
                                        && ui.small_button("\u{E4A6}").on_hover_text("Move this file to Trash").clicked() {
                                            file_to_trash_now = Some(f.path.clone());
                                        }
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

                    if let Some(target) = file_to_trash_now {
                        // Single file goes through the same confirm dialog as bulk actions (U15).
                        let plan = action_plan::for_selected(gi, g, &std::collections::HashSet::from([target]));
                        app.pending_action = Some(PendingAction::Trash {
                            title: "Move file to Trash".into(),
                            action: "trash_file",
                            plan,
                        });
                    }

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

    group_action_bar(app, ui, gi, &result);

    // Modal view triggers
    crate::ui::folder_intelligence::folder_twins_modal(ui, ctx, app, &result);

    if app.show_error_log {
        let mut close_clicked = false;
        let closed = crate::ui::modal::show(
            ctx,
            "error_log",
            "Error log",
            crate::ui::modal::ModalSize::Standard,
            |ui| {
                ui.label(muted(format!("{} error(s)", result.error_log.len())));
                ui.add_space(6.0);
                ScrollArea::both()
                    .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("error_log_grid").striped(true).num_columns(3).show(ui, |ui| {
                            ui.label(RichText::new("Path").strong());
                            ui.label(RichText::new("Reason").strong());
                            ui.label("");
                            ui.end_row();
                            for (p, e) in &result.error_log {
                                ui.label(RichText::new(p.display().to_string()).monospace().size(11.0));
                                ui.label(RichText::new(e).size(11.0));
                                if small_copy(ui) {
                                    ui.output_mut(|o| o.copied_text = p.display().to_string());
                                }
                                ui.end_row();
                            }
                        });
                    });
            },
            // Footer runs right to left: Close (primary) first, then Copy all on its left.
            |ui| {
                if primary_button(ui, "Close").clicked() {
                    close_clicked = true;
                }
                if secondary_button(ui, "Copy all").clicked() {
                    let all: String = result
                        .error_log
                        .iter()
                        .map(|(p, e)| format!("{}\t{}\n", p.display(), e))
                        .collect();
                    ui.output_mut(|o| o.copied_text = all);
                }
            },
        );
        if closed || close_clicked {
            app.show_error_log = false;
        }
    }

    if app.apply_message.is_some() || app.apply_errors.is_some() {
        ui.add_space(8.0);
        panel_card(ui, |ui| {
            if let Some(ref m) = app.apply_message {
                ui.label(m);
            }
            if let Some(ref e) = app.apply_errors {
                ui.colored_label(crate::ui::style::pal().danger, e);
            }
        });
    }
}

fn video_controls_bar(ui: &mut egui::Ui, app: &mut SortwiseApp, path: &Path, _ctx: &egui::Context) {
    ui.horizontal(|ui| {
        // Play / Pause
        let is_playing = app.inline_video.as_ref().map(|v| !v.is_paused()).unwrap_or(false);
        let icon = if is_playing { "\u{E39E}" } else { "\u{E3D0}" };
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
        ui.label(RichText::new(format!("{} / {}", fmt_secs(pos), fmt_secs(dur))).size(11.0).color(crate::ui::style::pal().text_muted));

        // Volume control trigger (persisted state value)
        let vol_icon = if app.video_volume == 0.0 { "\u{E45C}" } else { "\u{E44A}" };
        if icon_button(ui, vol_icon, app.video_volume > 0.0).clicked() {
            app.video_volume = if app.video_volume == 0.0 { 100.0 } else { 0.0 };
        }

        // Fullscreen Modal trigger
        if icon_button(ui, "↗", false).clicked() {
            app.explorer_open_preview(path.to_path_buf());
        }
    });
}

fn action_row(app: &mut SortwiseApp, ui: &mut egui::Ui, result: &ScanResult) {
    // One toolbar card: everyday actions on the left, mode and the primary action on the right.
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(0.0, 2.0))
        .show(ui, |ui| {
            // Wraps onto a second line when the right column is narrow.
            ui.horizontal_wrapped(|ui| {
                if secondary_button(ui, "+ New scan").clicked() {
                    app.view = crate::app::AppView::Scan;
                    app.set_last_result(None);
                    app.results_selected_group = None;
                    app.clear_preview_cache();
                }
                if secondary_button(ui, "Refresh findings").clicked() {
                    app.start_refresh_findings();
                }

                egui::menu::menu_button(ui, "Export ▾", |ui| {
                    if ui.button("Export JSON").clicked() {
                        export_json(result);
                        ui.close_menu();
                    }
                    if ui.button("Export manifest").clicked() {
                        export_manifest(app, result);
                        ui.close_menu();
                    }
                });

                egui::menu::menu_button(ui, "Clean up ▾", |ui| {
                    if ui.button("Clean same-folder duplicates").clicked() {
                        clean_all_same_folder_duplicates(app, result);
                        ui.close_menu();
                    }
                    if ui.button("Folder twins…").clicked() {
                        app.dedupe_twins_modal_open = true;
                        ui.close_menu();
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if app.action_mode != ActionMode::DryRun {
                        if primary_button(ui, "Apply actions").clicked() {
                            apply_all(app, result);
                        }
                        mode_pill(ui, app.action_mode);
                    }
                });
            });
        });

    // Report-only state. The error count lives in the status bar.
    if app.action_mode == ActionMode::DryRun {
        ui.add_space(8.0);
        let p = style::pal();
        egui::Frame::none()
            .fill(p.warning_soft)
            .rounding(egui::Rounding::same(style::R_MD))
            .inner_margin(egui::Margin::symmetric(12.0, 6.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    // U9: say plainly that nothing is changing, and offer the way to change it.
                    ui.label(RichText::new(format!("{}  Report only", crate::ui::icons::Icons::SEARCH)).strong().color(p.warning_on_soft));
                    ui.label(RichText::new("Nothing is moved or deleted.").color(p.warning_on_soft));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if secondary_button(ui, "Switch to Trash mode").clicked() {
                            app.action_mode = ActionMode::Delete;
                            app.apply_message = Some("Mode set to Trash. Apply actions still asks before it runs.".into());
                        }
                    });
                });
            });
    }
}

/// Height of the fixed action bar under the selected group (plan §6.4).
const GROUP_ACTION_BAR_H: f32 = 56.0;

/// Bottom bar for the selected group: Trash the checked copies, or uncheck them all.
/// Trash goes through the same `PendingAction::Trash` confirm as every other trash action.
fn group_action_bar(app: &mut SortwiseApp, ui: &mut egui::Ui, gi: usize, result: &ScanResult) {
    let Some(g) = result.duplicate_groups.get(gi) else {
        return;
    };
    let n = g.files.iter().filter(|f| app.results_selected_files.contains(&f.path)).count();
    let trash_label = if app.mode == crate::app::AppMode::CompareSourceDest {
        format!("\u{E4A6} Trash {n} copies in B")
    } else {
        format!("\u{E4A6} Trash {n} selected")
    };

    ui.add_space(6.0);
    panel_card(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            let trash = egui::Button::new(
                RichText::new(trash_label)
                    .strong()
                    .color(crate::ui::style::pal().on_accent),
            )
            .fill(crate::ui::style::pal().danger)
            .stroke(egui::Stroke::NONE)
            .rounding(egui::Rounding::same(style::R_SM))
            .min_size(egui::vec2(0.0, style::H_CONTROL));
            let hover = "Move the checked copies in this group to Trash. The keep file is never trashed.";
            if ui.add_enabled(n > 0, trash).on_hover_text(hover).clicked() {
                let plan = action_plan::for_selected(gi, g, &app.results_selected_files);
                if plan.is_empty() {
                    app.apply_message = Some("Nothing to trash: the keep file is never trashed.".into());
                } else {
                    app.pending_action = Some(PendingAction::Trash {
                        title: "Trash selected duplicates".into(),
                        action: "trash_selected",
                        plan,
                    });
                }
            }
            if secondary_button(ui, "Keep all").on_hover_text("Uncheck every copy in this group.").clicked() {
                for f in &g.files {
                    app.results_selected_files.remove(&f.path);
                }
            }
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
    app: &mut SortwiseApp,
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

fn filtered_group_indices(result: &ScanResult, filter: &str, kind: Option<MediaKind>) -> Vec<usize> {
    let t = filter.trim().to_lowercase();
    result
        .duplicate_groups
        .iter()
        .enumerate()
        .filter(|(_, g)| kind.is_none_or(|k| g.files.iter().any(|f| media_kind(&f.path) == k)))
        .filter_map(|(i, g)| {
            if t.is_empty() {
                return Some(i);
            }
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
    app: &mut SortwiseApp,
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

pub(crate) fn update_preview(ctx: &egui::Context, app: &mut SortwiseApp, path: &Path, rotation: i32) {
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
            app.preview_subtitle = None;
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
    app: &mut SortwiseApp,
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
            .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
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
                let target = path.to_path_buf();
                match app.last_result.clone().zip(app.results_selected_group) {
                    Some((res, gi)) if gi < res.duplicate_groups.len() => {
                        let selected = std::collections::HashSet::from([target]);
                        let plan = action_plan::for_selected(gi, &res.duplicate_groups[gi], &selected);
                        if plan.is_empty() {
                            app.apply_message = Some("The keep file is never trashed. Pick another copy.".into());
                        } else {
                            app.pending_action = Some(PendingAction::Trash {
                                title: "Move file to Trash".into(),
                                action: "trash_preview_file",
                                plan,
                            });
                        }
                    }
                    _ => app.apply_errors = Some("Select a duplicate group first.".into()),
                }
            }
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
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
                // Leave room for the 6 px frame margin on each side, or the frame runs past the card.
                let avail_w = (ui.available_width() - 4.0 - 12.0).max(120.0);
                let [tw, th] = tex.size();
                let (tw, th) = (tw as f32, th as f32);
                let scale = (avail_w / tw)
                    .min(preview_media_max_h / th)
                    .min(1.5);
                let size = egui::vec2(tw * scale, th * scale);
                egui::Frame::none()
                    .fill(crate::ui::style::pal().canvas)
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(6.0))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            let img_widget = egui::Image::from_texture(SizedTexture::from_handle(&tex))
                                .fit_to_exact_size(size)
                                .sense(egui::Sense::click());
                            let img_resp = ui.add(img_widget);
                            if img_resp.on_hover_text("\u{E30C} Click to open in full-screen player modal").clicked() {
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
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
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

pub(crate) fn prune_paths_from_results(
    app: &mut SortwiseApp, removed_paths: &std::collections::HashSet<std::path::PathBuf>) {
    let Some(res) = app.last_result.as_mut().map(std::sync::Arc::make_mut) else {
        return;
    };
    app.results_epoch = app.results_epoch.wrapping_add(1);

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

fn export_manifest(app: &SortwiseApp, result: &ScanResult) {
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

/// Stage the global Apply action. Nothing changes until the confirm dialog runs it (U1).
fn apply_all(app: &mut SortwiseApp, result: &ScanResult) {
    app.apply_message = None;
    app.apply_errors = None;
    let mode = app.action_mode;
    if mode == ActionMode::DryRun {
        app.apply_message = Some("Dry run: no changes applied.".into());
        return;
    }
    let plan = action_plan::for_apply(result, mode);
    if plan.is_empty() {
        app.apply_message = Some("Nothing to apply: no duplicate copies.".into());
        return;
    }
    app.pending_action = Some(PendingAction::Apply { mode, plan });
}

/// Stage "Clean same-folder duplicates" for confirmation (U3).
fn clean_all_same_folder_duplicates(app: &mut SortwiseApp, result: &ScanResult) {
    app.apply_message = None;
    app.apply_errors = None;
    let plan = action_plan::for_same_folder(result);
    if plan.is_empty() {
        app.apply_message = Some("No same-folder duplicates found.".into());
        return;
    }
    app.pending_action = Some(PendingAction::Trash {
        title: "Clean same-folder duplicates".into(),
        action: "clean_same_folder",
        plan,
    });
}

/// Run a bulk action the user confirmed. Only the confirm dialog calls this.
pub(crate) fn execute_pending_action(app: &mut SortwiseApp, pending: PendingAction) {
    app.apply_message = None;
    app.apply_errors = None;
    match pending {
        PendingAction::Trash { action, plan, .. } => {
            trash_plan(app, action, &plan);
            if action == "explorer_trash" {
                crate::ui::explorer_panel::explorer_after_trash(app);
            }
            if action == "extracted_archive" {
                app.extract_banner = None;
                app.explorer_status = Some(match app.apply_errors.clone() {
                    Some(e) => e,
                    None => "Moved the archive to Trash.".to_string(),
                });
                app.explorer_reload();
            }
            if action == "empty_folders" {
                // Folders that were trashed drop off the list; any that failed stay listed.
                app.empty_folders_results.retain(|p| p.exists());
                app.empty_folders_selected.clear();
            }
        }
        PendingAction::Apply { mode: ActionMode::Delete, plan } => {
            trash_plan(app, "apply_delete", &plan);
        }
        PendingAction::Apply { mode: ActionMode::HardLink, plan } => {
            apply_hardlinks(app, &plan);
        }
        PendingAction::Apply { mode: ActionMode::DryRun, .. } => {}
    }
}

/// Move a plan to Trash, prune the results, and write the action log.
fn trash_plan(app: &mut SortwiseApp, action: &str, plan: &ActionPlan) {
    match trash_batch(&plan.paths) {
        Ok(trashed) => {
            let done: std::collections::HashSet<PathBuf> = trashed.into_iter().collect();
            let completed: Vec<PathBuf> = plan.paths.iter().filter(|p| done.contains(*p)).cloned().collect();
            for p in &completed {
                app.results_selected_files.remove(p);
            }
            prune_paths_from_results(app, &done);
            app.clear_preview_cache();
            let log = record_action(action, &plan.paths, &completed, None);
            app.apply_message = Some(format!(
                "Moved {} file(s) to Trash, freeing {}. {}",
                completed.len(),
                format_bytes(plan.total_bytes),
                log.message
            ));
            app.apply_errors = log.error;
        }
        Err(e) => {
            let log = record_action(action, &plan.paths, &[], Some(&e));
            app.apply_errors = Some(format!("Trash failed: {e}. {}", log.message));
        }
    }
}

/// Replace every duplicate in the plan with a hard link to its group's keep file.
fn apply_hardlinks(app: &mut SortwiseApp, plan: &ActionPlan) {
    let Some(result) = app.last_result.clone() else {
        return;
    };
    let mut ok = 0usize;
    let mut errs = Vec::new();
    for g in &result.duplicate_groups {
        for r in resolver::apply_hardlink_group(g, ActionMode::HardLink) {
            match r {
                Ok(()) => ok += 1,
                Err(e) => errs.push(e.to_string()),
            }
        }
    }
    let note = format!("replaced {ok} of {} planned; {} error(s)", plan.paths.len(), errs.len());
    let log = record_action("apply_hardlink", &plan.paths, &[], Some(&note));
    app.apply_message = Some(format!("Replaced {ok} copies with hard links. {}", log.message));
    if !errs.is_empty() {
        app.apply_errors = Some(errs.join("\n"));
    } else {
        app.apply_errors = log.error;
    }
}

/// Outcome of writing an action log entry, shown to the user after the action.
struct LogOutcome {
    message: String,
    error: Option<String>,
}

fn record_action(action: &str, planned: &[PathBuf], completed: &[PathBuf], note: Option<&str>) -> LogOutcome {
    let entry = ActionLogEntry::new(action, planned, completed, note);
    match action_log::write(&entry) {
        Ok(path) => LogOutcome {
            message: format!("Log: {}", path.display()),
            error: None,
        },
        Err(e) => LogOutcome {
            message: String::new(),
            error: Some(format!("Action log not written: {e}")),
        },
    }
}

/// R3 keyboard actions for the selected group. Typing in a text field disables them.
/// Delete trashes checked copies (through the confirm dialog), Ctrl/Cmd+A checks every
/// non-keep copy, and the question mark key opens the shortcut list.
fn group_keyboard_shortcuts(app: &mut SortwiseApp, ctx: &egui::Context, gi: usize, result: &ScanResult) {
    if ctx.wants_keyboard_input() {
        return;
    }
    let (delete, select_all, help) = ctx.input(|i| {
        let help = i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == "?"));
        (
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            i.modifiers.command && i.key_pressed(egui::Key::A),
            help,
        )
    });
    if help {
        app.shortcuts_open = !app.shortcuts_open;
    }
    let g = &result.duplicate_groups[gi];
    if select_all {
        for (i, f) in g.files.iter().enumerate() {
            if i != g.keep_index {
                app.results_selected_files.insert(f.path.clone());
            }
        }
    }
    if delete {
        let plan = action_plan::for_selected(gi, g, &app.results_selected_files);
        if plan.is_empty() {
            app.apply_message = Some("Check copies first (the checkboxes). The keep file is never trashed.".into());
        } else {
            app.pending_action = Some(PendingAction::Trash {
                title: "Trash selected duplicates".into(),
                action: "trash_selected_key",
                plan,
            });
        }
    }
}

fn shortcuts_window(app: &mut SortwiseApp, ctx: &egui::Context) {
    if !app.shortcuts_open {
        return;
    }
    let mut close_clicked = false;
    let closed = crate::ui::modal::show(
        ctx,
        "shortcuts",
        "Keyboard shortcuts",
        crate::ui::modal::ModalSize::Standard,
        |ui| {
            let rows = [
                ("Up / Down or J / K", "Move between duplicate groups"),
                ("Delete / Backspace", "Trash checked copies (asks first)"),
                ("Ctrl / Cmd + A", "Check every non-keep copy in this group"),
                ("?", "Show or hide this list"),
            ];
            for (keys, what) in rows {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(keys).monospace().strong());
                    ui.label(muted(what));
                });
            }
        },
        |ui| {
            if primary_button(ui, "Close").clicked() {
                close_clicked = true;
            }
        },
    );
    if closed || close_clicked {
        app.shortcuts_open = false;
    }
}

/// Small "Copy" button for one table row.
fn small_copy(ui: &mut egui::Ui) -> bool {
    ui.small_button("Copy").clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sortwise::types::{FileEntrySnapshot, ScanStats};

    fn group(hash: &str, path: &str) -> DuplicateGroup {
        DuplicateGroup {
            hash: hash.into(),
            files: vec![FileEntrySnapshot {
                path: PathBuf::from(path),
                size: 10,
                score: 0,
                is_keep: true,
                root_index: 0,
            }],
            total_wasted_bytes: 10,
            keep_index: 0,
            keep_reason: String::new(),
        }
    }

    fn result() -> ScanResult {
        ScanResult {
            duplicate_groups: vec![group("aa", "/x/a.jpg"), group("bb", "/x/b.mp4")],
            hard_link_groups: vec![],
            stats: ScanStats {
                files_scanned: 2,
                files_skipped_error: 0,
                duplicate_groups: 2,
                total_wasted_bytes: 20,
                partial: false,
                hard_link_groups: 0,
            },
            error_log: vec![],
            folder_summaries: vec![],
            cross_folder_conflicts: vec![],
        }
    }

    #[test]
    fn group_list_memo_rebuilds_only_on_key_change() {
        let r = result();
        let mut cache = DupListCache::default();
        refresh_dup_list_cache(&mut cache, 1, "", DupGroupSortOrder::ScanOrder, None, &r);
        assert_eq!(cache.visible, vec![0, 1]);

        // Same key: the cached list is kept even if its contents were replaced.
        cache.visible = vec![42];
        refresh_dup_list_cache(&mut cache, 1, "", DupGroupSortOrder::ScanOrder, None, &r);
        assert_eq!(cache.visible, vec![42]);

        // New epoch (result changed): rebuilt.
        refresh_dup_list_cache(&mut cache, 2, "", DupGroupSortOrder::ScanOrder, None, &r);
        assert_eq!(cache.visible, vec![0, 1]);
    }

    #[test]
    fn group_list_memo_rebuilds_on_filter_sort_and_kind() {
        let r = result();
        let mut cache = DupListCache::default();
        refresh_dup_list_cache(&mut cache, 1, "", DupGroupSortOrder::ScanOrder, None, &r);

        refresh_dup_list_cache(&mut cache, 1, "mp4", DupGroupSortOrder::ScanOrder, None, &r);
        assert_eq!(cache.visible, vec![1]);

        refresh_dup_list_cache(&mut cache, 1, "", DupGroupSortOrder::ScanOrder, Some(MediaKind::Image), &r);
        assert_eq!(cache.visible, vec![0]);

        refresh_dup_list_cache(&mut cache, 1, "", DupGroupSortOrder::HashAsc, Some(MediaKind::Image), &r);
        assert_eq!(cache.visible, vec![0]);
    }
}
