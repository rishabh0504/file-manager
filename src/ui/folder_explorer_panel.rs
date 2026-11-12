//! Folder Explorer View: high-performance child folder grid browsing, rich folder contents
//! modal with image/video preview thumbnails, bottom smart breadcrumb traversal, instant next/prev folder carousel triage,
//! and queue/dequeue staging.

use crate::app::{ExplorerEntry, ExplorerViewMode, SortwiseApp};
use crate::ui::explorer_panel::draw_preview_or_icon;
use crate::ui::preview::{format_bytes, reveal_in_folder};
use crate::ui::style::{muted, secondary_button};
use eframe::egui::{
    self, scroll_area::ScrollBarVisibility, Color32, RichText, ScrollArea,
};
use sortwise::resolver::trash_single_path;
use std::path::PathBuf;

const FOLDER_TILE_W: f32 = 145.0;
const FOLDER_TILE_H: f32 = 125.0;

const MODAL_TILE_W: f32 = 142.0;
const MODAL_TILE_H: f32 = 160.0;

/// Render the child folder grid view inside Explorer when `explorer_view_mode == ExplorerViewMode::Folders`.
pub fn render_folder_grid(
    app: &mut SortwiseApp,
    ui: &mut egui::Ui,
    table_h: f32,
) {
    let filter = app.explorer_filter.trim().to_lowercase();
    // In Folder Explorer mode, list all child folders (including hidden, excluding system junk).
    let folder_indices: Vec<usize> = app
        .explorer_entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.is_dir && (filter.is_empty() || e.name.to_lowercase().contains(&filter))
        })
        .map(|(i, _)| i)
        .collect();

    if folder_indices.is_empty() {
        ScrollArea::vertical()
            .max_height(table_h)
            .show(ui, |ui| {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("\u{E24A}").size(44.0));
                    ui.add_space(8.0);
                    if !filter.is_empty() {
                        ui.label(
                            RichText::new(format!("No child folders matching “{filter}”"))
                                .color(crate::ui::style::pal().text_muted),
                        );
                    } else {
                        ui.label(
                            RichText::new("No child folders in this directory.")
                                .color(crate::ui::style::pal().text_muted),
                        );
                    }
                    ui.add_space(12.0);
                    if secondary_button(ui, "Switch to Files View").clicked() {
                        app.explorer_view_mode = ExplorerViewMode::Grid;
                    }
                });
            });
        return;
    }

    ScrollArea::vertical()
        .max_height(table_h)
        .auto_shrink([false, false])
        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
        .id_salt("folder_grid_scroll")
        .show(ui, |ui| {
            let avail = ui.available_width() - 32.0;
            let cell = FOLDER_TILE_W + 14.0;
            // No column cap: wide windows get more columns and the gaps absorb the rest, so no dead space on the right.
            let cols = ((avail / cell).floor() as usize).max(1);

            let total_card_width = cols as f32 * FOLDER_TILE_W;
            let remaining_width = avail - total_card_width;
            let spacing_x = if cols > 1 {
                (remaining_width / (cols - 1) as f32).max(12.0)
            } else {
                12.0
            };

            ui.add_space(10.0);
            for chunk in folder_indices.chunks(cols) {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.x = spacing_x;
                    for &idx in chunk {
                        folder_tile_ui(app, ui, idx);
                    }
                });
                ui.add_space(12.0);
            }
            ui.add_space(20.0);
        });
}

fn folder_tile_ui(app: &mut SortwiseApp, ui: &mut egui::Ui, idx: usize) {
    let e = app.explorer_entries[idx].clone();
    let is_hidden = e.name.starts_with('.');
    let mut is_staged = false;
    let mut staged_queue_name = String::new();
    for q in &app.explorer_queues {
        if q.items.iter().any(|p| p == &e.path) {
            is_staged = true;
            staged_queue_name = q.name.clone();
            break;
        }
    }

    let fill = if is_staged {
        crate::ui::style::pal().success_soft
    } else {
        crate::ui::style::pal().surface
    };

    let stroke_color = if is_staged {
        crate::ui::style::pal().success
    } else if is_hidden {
        crate::ui::style::pal().border_strong
    } else {
        crate::ui::style::pal().border
    };

    let mut open_modal = false;
    let mut open_grid = false;
    let mut queue_action: Option<usize> = None;
    let mut trash_action = false;
    let mut reveal_action = false;

    let resp = egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.set_width(FOLDER_TILE_W);
            ui.set_height(FOLDER_TILE_H);
            ui.vertical(|ui| {
                // Top row: folder icon + badge
                ui.horizontal(|ui| {
                    let folder_icon = if is_hidden { "\u{E24A}." } else { "\u{E24A}" };
                    ui.label(
                        RichText::new(folder_icon).size(22.0).color(
                            if is_hidden {
                                crate::ui::style::pal().accent_on_soft
                            } else {
                                crate::ui::style::pal().accent_on_soft
                            },
                        ),
                    );
                    if is_staged {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            egui::Frame::none()
                                .fill(crate::ui::style::pal().success_soft)
                                .rounding(egui::Rounding::same(4.0))
                                .inner_margin(egui::Margin::symmetric(5.0, 2.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new("QUEUED").size(8.5).strong().color(crate::ui::style::pal().success_on_soft));
                                });
                        });
                    }
                });

                ui.add_space(6.0);

                // Folder Name (bold)
                ui.add(
                    egui::Label::new(
                        RichText::new(&e.name)
                            .strong()
                            .size(12.5)
                            .color(if is_hidden {
                                crate::ui::style::pal().text
                            } else {
                                crate::ui::style::pal().text
                            }),
                    )
                    .truncate(),
                );

                ui.add_space(4.0);

                if let Some(t) = e.modified {
                    let time_str = chrono::DateTime::<chrono::Local>::from(t)
                        .format("%Y-%m-%d")
                        .to_string();
                    ui.label(
                        RichText::new(time_str)
                            .size(10.0)
                            .color(crate::ui::style::pal().border_strong),
                    );
                } else {
                    ui.label(muted("folder"));
                }
            });
        })
        .response
        .interact(egui::Sense::click());

    // Left-click opens the Folder Modal; Double-click navigates inside.
    if resp.clicked() {
        open_modal = true;
    }
    if resp.double_clicked() {
        open_grid = true;
    }

    resp.context_menu(|ui| {
        if ui.button("\u{E30C} Open in Folder Modal").clicked() {
            open_modal = true;
            ui.close_menu();
        }
        if ui.button("\u{E256} Open in Grid View").clicked() {
            open_grid = true;
            ui.close_menu();
        }
        ui.separator();
        if !app.explorer_queues.is_empty() {
            let pick = app.explorer_preview_queue_pick.unwrap_or(0).min(app.explorer_queues.len() - 1);
            let qname = &app.explorer_queues[pick].name;
            if is_staged {
                if ui.button(format!("\u{E32A} Dequeue from “{qname}”")).clicked() {
                    queue_action = Some(pick);
                    ui.close_menu();
                }
            } else if ui.button(format!("\u{E3D4} Stage to “{qname}”")).clicked() {
                queue_action = Some(pick);
                ui.close_menu();
            }
        }
        if ui.button("Reveal in Finder / Explorer").clicked() {
            reveal_action = true;
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Move to Trash").clicked() {
            trash_action = true;
            ui.close_menu();
        }
    });

    let hover_text = if is_staged {
        format!(
            "\u{E24A} {}\n\u{E184} Staged in queue “{}”\n(Click to inspect contents, Double-click to browse)",
            e.name, staged_queue_name
        )
    } else {
        format!(
            "\u{E24A} {}\n(Click to inspect contents, Double-click to browse)",
            e.name
        )
    };
    resp.on_hover_text(hover_text);

    if open_modal {
        app.folder_modal_open_dir(e.path.clone());
    } else if open_grid {
        app.explorer_navigate(e.path.clone());
    } else if let Some(q_idx) = queue_action {
        app.explorer_toggle_queue(q_idx, e.path.clone());
    } else if reveal_action {
        reveal_in_folder(&e.path);
    } else if trash_action {
        let tname = e.name.clone();
        if trash_single_path(&e.path).is_ok() {
            app.explorer_status = Some(format!("Moved folder “{tname}” to Trash."));
            app.explorer_reload();
        }
    }
}

/// The Folder Contents Modal overlay window.
pub fn folder_contents_modal(app: &mut SortwiseApp, ctx: &egui::Context) {
    if !app.folder_modal_open {
        return;
    }
    let Some(path) = app.folder_modal_path.clone() else {
        return;
    };

    // Dark translucent backdrop overlay behind the modal
    egui::Area::new(egui::Id::new("folder_modal_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(true)
        .show(ctx, |ui| {
            let screen_rect = ui.ctx().screen_rect();
            ui.painter().rect_filled(screen_rect, 0.0, Color32::from_black_alpha(110));
        });

    let folder_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let mut close = false;
    let mut open_in_grid = false;
    let mut nav_modal: Option<PathBuf> = None;
    let mut preview_file: Option<PathBuf> = None;

    // Determine carousel position among child folders in explorer_entries
    let folders: Vec<PathBuf> = app
        .explorer_entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| e.path.clone())
        .collect();
    let cur_pos = folders.iter().position(|p| p == &path);
    let can_prev = cur_pos.map(|i| i > 0).unwrap_or(false);
    let can_next = cur_pos.map(|i| i + 1 < folders.len()).unwrap_or(false);

    // Keyboard navigation when preview modal is not open
    if !app.explorer_preview_open && !ctx.wants_keyboard_input() {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if can_next && ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            app.folder_modal_next();
        }
        if can_prev && ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            app.folder_modal_prev();
        }
    }

    let mut trash_folder = false;
    let mut reveal_folder = false;
    let mut to_grid = false;
    let closed = crate::ui::modal::show_fill(
        ctx,
        "folder_contents",
        &folder_name,
        crate::ui::modal::ModalSize::Large,
        |ui| {
            // 2. Carousel Triage & Queue Controls Toolbar
            egui::Frame::none()
                .fill(crate::ui::style::pal().surface)
                .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().surface_raised))
                .inner_margin(egui::Margin::symmetric(18.0, 8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Prev / Next Navigation Buttons
                        let prev_btn = egui::Button::new(RichText::new("\u{E5A4} Prev").strong().size(13.0))
                            .fill(crate::ui::style::pal().surface_raised)
                            .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border_strong))
                            .min_size(egui::vec2(68.0, 28.0));
                        if ui.add_enabled(can_prev, prev_btn).on_hover_text("Previous folder (Left Arrow ←)").clicked() {
                            app.folder_modal_prev();
                        }

                        let next_btn = egui::Button::new(RichText::new("Next \u{E5A6}").strong().size(13.0))
                            .fill(crate::ui::style::pal().surface_raised)
                            .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border_strong))
                            .min_size(egui::vec2(68.0, 28.0));
                        if ui.add_enabled(can_next, next_btn).on_hover_text("Next folder (Right Arrow →)").clicked() {
                            app.folder_modal_next();
                        }

                        if let Some(i) = cur_pos {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("Folder {} of {}", i + 1, folders.len()))
                                    .color(crate::ui::style::pal().text_muted)
                                    .size(12.5),
                            );
                        }

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(16.0);

                        // Queue & Dequeue Controls for this Folder
                        modal_folder_queue_controls(app, ui, &path);

                        // Search inside this folder
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut app.folder_modal_filter)
                                    .hint_text("Filter contents…")
                                    .desired_width(160.0),
                            );
                        });
                    });
                });

            // 3. Folder Contents Stage (Rich Visual Grid with Image/Video Previews & Thumbnails)
            // Leave 38px at bottom for smart breadcrumbs bar
            let modal_body_h = (ui.available_height() - 40.0).max(180.0);
            let filter = app.folder_modal_filter.trim().to_lowercase();
            let filtered_entries: Vec<ExplorerEntry> = app
                .folder_modal_entries
                .iter()
                .filter(|e| {
                    filter.is_empty() || e.name.to_lowercase().contains(&filter)
                })
                .cloned()
                .collect();

            if filtered_entries.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(48.0);
                    ui.label(RichText::new("\u{E256}").size(48.0));
                    ui.add_space(8.0);
                    if !filter.is_empty() {
                        ui.label(muted(format!("No items matching “{filter}”")));
                    } else {
                        ui.label(muted("This folder is empty."));
                    }
                });
            } else {
                ScrollArea::vertical()
                    .max_height(modal_body_h)
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                    .id_salt("modal_folder_grid_scroll")
                    .show(ui, |ui| {
                        let avail = ui.available_width() - 40.0;
                        // Rendered card = tile + frame padding (18 px); use it so columns never overflow.
                        let card_w = MODAL_TILE_W + 18.0;
                        let cell = card_w + 12.0;
                        // No column cap, so wide modals fill the width instead of leaving dead space on the right.
                        let cols = ((avail / cell).floor() as usize).max(1);

                        let total_card_width = cols as f32 * card_w;
                        let remaining_width = avail - total_card_width;
                        let spacing_x = if cols > 1 {
                            (remaining_width / (cols - 1) as f32).max(10.0)
                        } else {
                            10.0
                        };

                        ui.add_space(12.0);
                        for chunk in filtered_entries.chunks(cols) {
                            ui.horizontal(|ui| {
                                ui.add_space(16.0);
                                ui.spacing_mut().item_spacing.x = spacing_x;
                                for entry in chunk {
                                    modal_item_card(app, ui, entry, &mut preview_file, &mut nav_modal);
                                }
                            });
                            ui.add_space(12.0);
                        }
                        ui.add_space(16.0);
                    });
            }

            // 4. Bottom Smart Breadcrumb Bar (Elided start with '...' and last 3-4 folders)
            egui::Frame::none()
                .fill(crate::ui::style::pal().surface)
                .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().surface_raised))
                .inner_margin(egui::Margin::symmetric(18.0, 8.0))
                .show(ui, |ui| {
                    bottom_breadcrumb_bar(ui, &path, &mut nav_modal, &mut open_in_grid);
                });
        },
        // Footer runs right to left: Trash on the far right is kept apart from the safe actions.
        |ui| {
            if crate::ui::style::danger_button(ui, "Trash folder")
                .on_hover_text("Move this entire folder to Trash")
                .clicked()
            {
                trash_folder = true;
            }
            if crate::ui::style::secondary_button(ui, "Open in grid")
                .on_hover_text("Open this folder in the main Explorer grid view")
                .clicked()
            {
                to_grid = true;
            }
            if crate::ui::style::secondary_button(ui, "Reveal")
                .on_hover_text("Reveal in Finder / Explorer")
                .clicked()
            {
                reveal_folder = true;
            }
        },
    );
    // Esc belongs to the file preview while it is open.
    if closed && !app.explorer_preview_open {
        close = true;
    }
    if to_grid {
        open_in_grid = true;
    }
    if reveal_folder {
        reveal_in_folder(&path);
    }
    if trash_folder {
        crate::ui::explorer_panel::queue_trash(app, vec![(path.clone(), 0)]);
    }

    if let Some(next_p) = nav_modal {
        app.folder_modal_open_dir(next_p);
    } else if open_in_grid {
        app.folder_modal_open_in_grid();
    } else if let Some(file_p) = preview_file {
        app.explorer_open_preview(file_p);
    } else if close {
        app.folder_modal_close();
    }
}

/// Smart bottom breadcrumb bar displaying `...` and the last 3-4 parent directories.
fn bottom_breadcrumb_bar(
    ui: &mut egui::Ui,
    path: &std::path::Path,
    nav_modal: &mut Option<PathBuf>,
    open_in_grid: &mut bool,
) {
    let mut crumbs: Vec<PathBuf> = path.ancestors().map(|p| p.to_path_buf()).collect();
    crumbs.reverse();

    let max_visible = 4;
    let (has_ellipsis, visible_crumbs) = if crumbs.len() > max_visible {
        (true, &crumbs[crumbs.len() - max_visible..])
    } else {
        (false, &crumbs[..])
    };

    ui.horizontal(|ui| {
        ui.label(RichText::new("\u{E316} Path:").size(12.0).color(crate::ui::style::pal().border_strong));
        ui.add_space(4.0);

        if has_ellipsis {
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("…")
                            .size(12.5)
                            .strong()
                            .color(crate::ui::style::pal().accent),
                    )
                    .frame(false),
                )
                .on_hover_text("Go to root directory")
                .clicked()
            {
                if let Some(first) = crumbs.first() {
                    *nav_modal = Some(first.clone());
                }
            }
            ui.label(RichText::new("›").color(crate::ui::style::pal().border_strong).size(13.0));
        }

        for (i, c) in visible_crumbs.iter().enumerate() {
            if i > 0 {
                ui.label(RichText::new("›").color(crate::ui::style::pal().border_strong).size(13.0));
            }
            let name = if c.parent().is_none() {
                "Root".to_string()
            } else {
                c.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            };
            let is_leaf = c == path;
            let text_color = if is_leaf {
                crate::ui::style::pal().text
            } else {
                crate::ui::style::pal().accent_on_soft
            };

            let btn = egui::Button::new(
                RichText::new(name)
                    .color(text_color)
                    .strong()
                    .size(12.0),
            )
            .frame(false);

            if ui
                .add(btn)
                .on_hover_text(if is_leaf {
                    "Click to open in main grid view"
                } else {
                    "Navigate to this folder"
                })
                .clicked()
            {
                if is_leaf {
                    *open_in_grid = true;
                } else {
                    *nav_modal = Some(c.clone());
                }
            }
        }
    });
}

/// Queue and Dequeue controls for the Folder Modal.
fn modal_folder_queue_controls(app: &mut SortwiseApp, ui: &mut egui::Ui, folder_path: &PathBuf) {
    let queue_names: Vec<String> = app
        .explorer_queues
        .iter()
        .map(|q| format!("{} ({})", q.name, q.items.len()))
        .collect();

    // Always offered, so more queues can be added from here once the first exists.
    if secondary_button(ui, "+ New Queue").clicked() {
        // queue_names is stale after creating a queue; return and let the next frame rebuild the dropdown.
        app.explorer_new_queue();
        return;
    }
    if queue_names.is_empty() {
        return;
    }

    // Same searchable dropdown and auto-detect as the preview modal, so both behave identically.
    if let Some(existing_idx) = app.explorer_find_queue_for_path(folder_path) {
        app.explorer_preview_queue_pick = Some(existing_idx);
    }

    let mut pick = app
        .explorer_preview_queue_pick
        .filter(|&i| i < queue_names.len())
        .or(Some(0));

    crate::ui::explorer_panel::searchable_queue_combo(
        ui,
        "modal_folder_queue_pick",
        180.0,
        &mut pick,
        &queue_names,
        "Choose a queue…",
    );
    app.explorer_preview_queue_pick = pick;

    if let Some(q_idx) = pick {
        let is_staged = app.explorer_is_in_queue(q_idx, folder_path);
        if is_staged {
            let dequeue_btn = egui::Button::new(
                RichText::new("\u{E32A} Dequeue Folder")
                    .strong()
                    .color(crate::ui::style::pal().danger_on_soft),
            )
            .fill(crate::ui::style::pal().danger_soft)
            .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().danger))
            .min_size(egui::vec2(0.0, 26.0));

            if ui.add(dequeue_btn).on_hover_text("Remove this folder from the staging queue").clicked() {
                app.explorer_toggle_queue(q_idx, folder_path.clone());
            }

            ui.add_space(4.0);
            ui.label(RichText::new("\u{E184} Staged").color(crate::ui::style::pal().success).size(11.5));
        } else {
            let queue_btn = egui::Button::new(
                RichText::new("\u{E3D4} Stage Folder to Queue")
                    .strong()
                    .color(crate::ui::style::pal().on_accent),
            )
            .fill(crate::ui::style::pal().accent)
            .min_size(egui::vec2(0.0, 26.0));

            if ui.add(queue_btn).on_hover_text("Stage this entire folder into the selected queue (moves on Submit)").clicked() {
                app.explorer_toggle_queue(q_idx, folder_path.clone());
            }
        }
    }
}

/// Render a single item card inside the Folder Contents Modal grid with preview/thumbnail.
fn modal_item_card(
    app: &mut SortwiseApp,
    ui: &mut egui::Ui,
    e: &ExplorerEntry,
    preview_file: &mut Option<PathBuf>,
    nav_modal: &mut Option<PathBuf>,
) {
    let mut is_staged = false;
    for q in &app.explorer_queues {
        if q.items.iter().any(|p| p == &e.path) {
            is_staged = true;
            break;
        }
    }

    let fill = if is_staged {
        crate::ui::style::pal().success_soft
    } else {
        crate::ui::style::pal().surface
    };

    let stroke_color = if is_staged {
        crate::ui::style::pal().success
    } else {
        crate::ui::style::pal().border
    };

    let resp = egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke_color))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(0.0))
        .show(ui, |ui| {
            ui.set_width(MODAL_TILE_W);
            ui.set_height(MODAL_TILE_H);
            ui.vertical(|ui| {
                // Top area: image/video thumbnail or type icon
                ui.allocate_ui(egui::vec2(MODAL_TILE_W, MODAL_TILE_H * 0.68), |ui| {
                    draw_preview_or_icon(app, ui, egui::vec2(MODAL_TILE_W, MODAL_TILE_H * 0.68), e);
                });

                // Bottom area: File name + size/kind + staged badge
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.set_width(MODAL_TILE_W);
                        ui.vertical_centered(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&e.name)
                                        .color(if e.is_dir {
                                            crate::ui::style::pal().accent_on_soft
                                        } else {
                                            crate::ui::style::pal().text
                                        })
                                        .strong()
                                        .size(11.5),
                                )
                                .truncate(),
                            );
                            if !e.is_dir {
                                ui.label(muted(format_bytes(e.size)));
                            } else {
                                ui.label(muted("folder"));
                            }
                        });
                    });
            });
        })
        .response
        .interact(egui::Sense::click());

    if resp.clicked() {
        if e.is_dir {
            *nav_modal = Some(e.path.clone());
        } else {
            *preview_file = Some(e.path.clone());
        }
    }

    let hover_text = if is_staged {
        format!("{}\n\u{E184} Staged in Queue\n(Click to preview)", e.name)
    } else if e.is_dir {
        format!("\u{E24A} {}\n(Click to open subfolder)", e.name)
    } else {
        format!("{}\n(Click to preview)", e.name)
    };
    resp.on_hover_text(hover_text);
}
