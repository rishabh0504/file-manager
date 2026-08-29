//! Windows-Explorer-style file browser: places sidebar, breadcrumb, sortable +
//! groupable file table, left-click preview modal, and right-click actions
//! (open, reveal, extract archives, move to Trash, new folder).
use crate::app::{
    ExplorerEntry, ExplorerSortKey, ExplorerTypeFilter, ExplorerViewMode, SmartFileManagerApp,
};
use crate::ui::preview::{format_bytes, open_in_default_app, reveal_in_folder};
use crate::ui::style::{self, muted, primary_button, secondary_button};
use eframe::egui::{
    self, load::SizedTexture, scroll_area::ScrollBarVisibility, Color32, RichText, ScrollArea,
    TextureOptions,
};
use smart_file_manager::resolver::trash_single_path;
use std::path::PathBuf;
use std::time::SystemTime;

/// Collected from clicks / menus and applied after rendering to avoid borrow clashes.
enum Act {
    Nav(PathBuf),
    Preview(PathBuf),
    OpenDefault(PathBuf),
    Reveal(PathBuf),
    Trash(PathBuf),
    Extract(PathBuf),
    NewFolder,
    Refresh,
    Sort(ExplorerSortKey),
    // Queue (staged move) actions.
    NewQueue,
    OpenQueue(usize),
    RemoveFromQueue(usize, PathBuf),
    SubmitAllQueues,
    DeleteQueue(usize),
    ChangeQueueTarget(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cat {
    Folder,
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Other,
}

const CAT_ORDER: [(Cat, &str); 7] = [
    (Cat::Folder, "Folders"),
    (Cat::Image, "Images"),
    (Cat::Video, "Videos"),
    (Cat::Audio, "Audio"),
    (Cat::Document, "Documents"),
    (Cat::Archive, "Archives"),
    (Cat::Other, "Other"),
];

// ---------------------------------------------------------------------------
// Sidebar (places)
// ---------------------------------------------------------------------------

pub fn explorer_sidebar(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    ui.label(muted("PLACES"));
    ui.add_space(6.0);

    let mut nav: Option<PathBuf> = None;
    for (icon, label, path) in places() {
        let selected = app.explorer_dir == path;
        if place_row(ui, icon, &label, selected).clicked() {
            nav = Some(path);
        }
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);
    if secondary_button(ui, "Open folder…").clicked() {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            nav = Some(dir);
        }
    }

    if let Some(p) = nav {
        app.explorer_navigate(p);
    }
}

fn place_row(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let bg = if selected {
        Color32::from_rgb(38, 56, 98)
    } else {
        Color32::TRANSPARENT
    };
    egui::Frame::none()
        .fill(bg)
        .rounding(egui::Rounding::same(7.0))
        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).size(15.0));
                ui.add_space(4.0);
                ui.label(RichText::new(label).color(Color32::from_rgb(214, 219, 236)));
                ui.allocate_space(egui::vec2(ui.available_width(), 0.0));
            });
        })
        .response
        .interact(egui::Sense::click())
}

fn places() -> Vec<(&'static str, String, PathBuf)> {
    let mut out: Vec<(&'static str, String, PathBuf)> = Vec::new();
    let mut add = |icon, label: &str, p: Option<PathBuf>| {
        if let Some(p) = p {
            if p.exists() {
                out.push((icon, label.to_string(), p));
            }
        }
    };
    add("🏠", "Home", dirs::home_dir());
    add("🖥", "Desktop", dirs::desktop_dir());
    add("📄", "Documents", dirs::document_dir());
    add("⬇", "Downloads", dirs::download_dir());
    add("🖼", "Pictures", dirs::picture_dir());
    add("🎬", "Movies", dirs::video_dir());
    add("🎵", "Music", dirs::audio_dir());

    #[cfg(target_os = "macos")]
    {
        if let Ok(rd) = std::fs::read_dir("/Volumes") {
            for e in rd.flatten() {
                out.push(("💽", e.file_name().to_string_lossy().to_string(), e.path()));
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        out.push(("💽", "Root".to_string(), PathBuf::from("/")));
    }
    out
}

// ---------------------------------------------------------------------------
// Main panel
// ---------------------------------------------------------------------------

pub fn explorer_main_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    // Guarantee the listing matches the current directory regardless of when
    // navigation happened this frame.
    app.explorer_ensure_loaded();

    let mut act: Option<Act> = None;

    toolbar(app, ui, &mut act);
    ui.add_space(4.0);
    breadcrumb_bar(app, ui, &mut act);
    ui.add_space(6.0);

    if let Some(err) = app.explorer_error.clone() {
        banner(ui, Color32::from_rgb(60, 36, 36), Color32::from_rgb(255, 160, 160), &format!("⚠  {err}"));
        ui.add_space(4.0);
    }
    if let Some(msg) = app.explorer_status.clone() {
        banner(ui, Color32::from_rgb(34, 48, 40), Color32::from_rgb(150, 220, 180), &msg);
        ui.add_space(4.0);
    }

    // Footer-first: reserve status strip, then give the table the remaining height.
    let footer_h = 36.0;
    let table_h = (ui.available_height() - footer_h).max(160.0);
    file_table(app, ui, &mut act, table_h);

    ui.add_space(4.0);
    selection_bar(app, ui, &mut act);

    // Modals.
    delete_zip_prompt(app, ctx);

    apply_action(app, act);
}

/// Select All + queue picker for collapsed recursive-search results.
fn search_selection_bar(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    egui::Frame::none()
        .fill(Color32::from_rgb(32, 36, 48))
        .rounding(egui::Rounding::same(7.0))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let total = app.explorer_entries.len();
                let folders = app.explorer_entries.iter().filter(|e| e.is_dir).count();
                let files = total.saturating_sub(folders);
                let all_checked = total > 0
                    && app
                        .explorer_entries
                        .iter()
                        .all(|e| app.explorer_search_checked.contains(&e.path));
                let mut select_all = all_checked;
                if ui
                    .checkbox(&mut select_all, "Select all")
                    .on_hover_text("Check or uncheck every search result")
                    .changed()
                {
                    if select_all {
                        app.explorer_search_select_all();
                    } else {
                        app.explorer_search_clear_checks();
                    }
                }

                let selected = app.explorer_search_checked.len();
                let sel_folders = app
                    .explorer_entries
                    .iter()
                    .filter(|e| e.is_dir && app.explorer_search_checked.contains(&e.path))
                    .count();
                let sel_files = selected.saturating_sub(sel_folders);
                ui.label(muted(format!(
                    "{selected} selected · {sel_folders} folders · {sel_files} files  (of {folders} folders · {files} files)"
                )));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let queue_names: Vec<String> = app
                        .explorer_queues
                        .iter()
                        .map(|q| format!("{} ({})", q.name, q.items.len()))
                        .collect();
                    if queue_names.is_empty() {
                        if secondary_button(ui, "＋ New Queue").clicked() {
                            app.explorer_new_queue();
                        }
                    } else {
                        let mut pick = app
                            .explorer_preview_queue_pick
                            .filter(|&i| i < queue_names.len());
                        let selected_text = pick
                            .map(|i| queue_names[i].clone())
                            .unwrap_or_else(|| "Choose queue…".to_string());
                        egui::ComboBox::new(egui::Id::new("search_queue_pick"), "")
                            .width(150.0)
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                for (i, name) in queue_names.iter().enumerate() {
                                    ui.selectable_value(&mut pick, Some(i), name);
                                }
                            });
                        app.explorer_preview_queue_pick = pick;

                        let add_enabled = pick.is_some() && !app.explorer_search_checked.is_empty();
                        if ui
                            .add_enabled(
                                add_enabled,
                                egui::Button::new(
                                    RichText::new("➕  Add to Queue")
                                        .strong()
                                        .color(Color32::from_rgb(248, 250, 255)),
                                )
                                .fill(style::ACCENT),
                            )
                            .on_hover_text("Stage checked files (folders expand to media files)")
                            .clicked()
                        {
                            if let Some(i) = pick {
                                app.explorer_add_checked_to_queue(i);
                            }
                        }
                    }
                });
            });
        });
}

fn toolbar(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, act: &mut Option<Act>) {
    ui.horizontal(|ui| {
        let can_back = !app.explorer_back.is_empty();
        let can_fwd = !app.explorer_fwd.is_empty();
        let can_up = app.explorer_dir.parent().is_some();
        if ui.add_enabled(can_back, egui::Button::new("←")).on_hover_text("Back").clicked() {
            app.explorer_go_back();
        }
        if ui.add_enabled(can_fwd, egui::Button::new("→")).on_hover_text("Forward").clicked() {
            app.explorer_go_forward();
        }
        if ui.add_enabled(can_up, egui::Button::new("↑")).on_hover_text("Up one level").clicked() {
            app.explorer_go_up();
        }
        if ui.button("⟳").on_hover_text("Refresh").clicked() {
            app.explorer_reload();
        }
        ui.add_space(8.0);
        ui.add(
            egui::TextEdit::singleline(&mut app.explorer_filter)
                .hint_text("Search (recursive)…")
                .desired_width(170.0),
        );
        // Recursive search status indicator
        if app.explorer_searching {
            ui.spinner();
            ui.label(muted("Searching…"));
        } else if app.explorer_search_pending {
            ui.label(
                RichText::new("⏱ Waiting…")
                    .color(Color32::from_rgb(180, 170, 120))
                    .size(11.5),
            );
        } else if app.explorer_is_recursive_search {
            ui.label(
                RichText::new("🔍 Recursive results")
                    .color(Color32::from_rgb(120, 170, 255))
                    .size(11.5),
            );
        }
    });

    if app.explorer_is_recursive_search && !app.explorer_searching {
        ui.add_space(4.0);
        search_selection_bar(app, ui);
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(muted("Sort:"));
        let mut key = app.explorer_sort_key;
        egui::ComboBox::new(egui::Id::new("explorer_sort"), "")
            .width(130.0)
            .selected_text(match key {
                ExplorerSortKey::Name => "Name",
                ExplorerSortKey::Size => "Size",
                ExplorerSortKey::Kind => "Kind",
                ExplorerSortKey::Modified => "Modified",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut key, ExplorerSortKey::Name, "Name");
                ui.selectable_value(&mut key, ExplorerSortKey::Size, "Size");
                ui.selectable_value(&mut key, ExplorerSortKey::Kind, "Kind");
                ui.selectable_value(&mut key, ExplorerSortKey::Modified, "Modified");
            });
        if key != app.explorer_sort_key {
            app.explorer_sort_key = key;
            app.explorer_sort_entries();
        }
        let arrow = if app.explorer_sort_asc { "↑ Asc" } else { "↓ Desc" };
        if ui.button(arrow).on_hover_text("Toggle sort direction").clicked() {
            app.explorer_sort_asc = !app.explorer_sort_asc;
            app.explorer_sort_entries();
        }
        ui.add_space(6.0);
        ui.label(muted("Filter:"));
        let mut type_filter = app.explorer_type_filter;
        egui::ComboBox::new(egui::Id::new("explorer_type_filter"), "")
            .width(110.0)
            .selected_text(match type_filter {
                ExplorerTypeFilter::All => "All",
                ExplorerTypeFilter::Folders => "Folders",
                ExplorerTypeFilter::Image => "Images",
                ExplorerTypeFilter::Video => "Videos",
                ExplorerTypeFilter::Archive => "Zips",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut type_filter, ExplorerTypeFilter::All, "All");
                ui.selectable_value(&mut type_filter, ExplorerTypeFilter::Folders, "Folders");
                ui.selectable_value(&mut type_filter, ExplorerTypeFilter::Image, "Images");
                ui.selectable_value(&mut type_filter, ExplorerTypeFilter::Video, "Videos");
                ui.selectable_value(&mut type_filter, ExplorerTypeFilter::Archive, "Zips");
            });
        app.explorer_type_filter = type_filter;

        ui.add_space(6.0);
        ui.checkbox(&mut app.explorer_group_by_kind, "Group by type");

        ui.add_space(6.0);
        view_toggle(app, ui);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if secondary_button(ui, "＋ New folder").clicked() {
                *act = Some(Act::NewFolder);
            }
        });
    });
}

fn view_toggle(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    let seg = |ui: &mut egui::Ui, label: &str, active: bool| -> bool {
        let (fg, bg) = if active {
            (Color32::from_rgb(248, 250, 255), style::ACCENT)
        } else {
            (Color32::from_rgb(196, 202, 222), Color32::from_rgb(40, 42, 52))
        };
        ui.add(egui::Button::new(RichText::new(label).color(fg)).fill(bg)).clicked()
    };
    ui.spacing_mut().item_spacing.x = 4.0;
    if seg(ui, "☰ List", app.explorer_view_mode == ExplorerViewMode::List) {
        app.explorer_view_mode = ExplorerViewMode::List;
    }
    if seg(ui, "▦ Grid", app.explorer_view_mode == ExplorerViewMode::Grid) {
        app.explorer_view_mode = ExplorerViewMode::Grid;
    }
}

fn breadcrumb_bar(app: &SmartFileManagerApp, ui: &mut egui::Ui, act: &mut Option<Act>) {
    egui::Frame::none()
        .fill(Color32::from_rgb(32, 32, 40))
        .rounding(egui::Rounding::same(7.0))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            // Thin horizontal scrollbar so long paths don't dominate the bar height.
            ui.spacing_mut().scroll.bar_width = 4.0;
            ui.spacing_mut().scroll.handle_min_length = 24.0;
            ui.set_min_height(32.0);
            ScrollArea::horizontal()
                .auto_shrink([false, true])
                .max_width(ui.available_width())
                .id_salt("breadcrumb_scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_min_height(28.0);
                        let mut crumbs: Vec<PathBuf> =
                            app.explorer_dir.ancestors().map(|p| p.to_path_buf()).collect();
                        crumbs.reverse();
                        for (i, c) in crumbs.iter().enumerate() {
                            if i > 0 {
                                ui.label(
                                    RichText::new("›")
                                        .size(14.0)
                                        .color(Color32::from_rgb(90, 94, 112)),
                                );
                            }
                            let name = c
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "/".to_string());
                            let last = i + 1 == crumbs.len();
                            let color = if last {
                                Color32::from_rgb(236, 240, 255)
                            } else {
                                Color32::from_rgb(150, 175, 230)
                            };
                            if ui
                                .add(
                                    egui::Label::new(
                                        RichText::new(name).size(14.0).color(color),
                                    )
                                    .sense(egui::Sense::click()),
                                )
                                .clicked()
                            {
                                *act = Some(Act::Nav(c.clone()));
                            }
                        }
                    });
                });
        });
}

fn file_table(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, act: &mut Option<Act>, table_h: f32) {
    let filter = app.explorer_filter.trim().to_lowercase();
    let type_filter = app.explorer_type_filter;
    // When in recursive-search mode, explorer_entries already holds globally-filtered
    // results — skip the second in-frame name filter to prevent double-filtering.
    let visible: Vec<usize> = app
        .explorer_entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            if !app.explorer_is_recursive_search
                && !filter.is_empty()
                && !e.name.to_lowercase().contains(&filter)
            {
                return false;
            }
            matches_type_filter(e, type_filter)
        })
        .map(|(i, _)| i)
        .collect();

    let key = app.explorer_sort_key;
    let asc = app.explorer_sort_asc;
    let arrow = |k: ExplorerSortKey| -> &'static str {
        if k == key {
            if asc { " ▲" } else { " ▼" }
        } else {
            ""
        }
    };

    let is_list = app.explorer_view_mode == ExplorerViewMode::List;
    let padding = if is_list {
        egui::Margin::same(2.0)
    } else {
        egui::Margin::symmetric(12.0, 8.0)
    };
    egui::Frame::none()
        .fill(Color32::from_rgb(24, 24, 30))
        .stroke(egui::Stroke::new(1.0, style::BORDER))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(padding)
        .show(ui, |ui| {
            // Column header (List view only — Grid sorts via the toolbar control).
            if is_list {
                egui::Frame::none()
                    .fill(Color32::from_rgb(34, 34, 44))
                    .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let full = ui.available_width();
                            if header(ui, &format!("Name{}", arrow(ExplorerSortKey::Name)), full * 0.46) {
                                *act = Some(Act::Sort(ExplorerSortKey::Name));
                            }
                            if header(ui, &format!("Size{}", arrow(ExplorerSortKey::Size)), full * 0.14) {
                                *act = Some(Act::Sort(ExplorerSortKey::Size));
                            }
                            if header(ui, &format!("Kind{}", arrow(ExplorerSortKey::Kind)), full * 0.16) {
                                *act = Some(Act::Sort(ExplorerSortKey::Kind));
                            }
                            if header(
                                ui,
                                &format!("Modified{}", arrow(ExplorerSortKey::Modified)),
                                full * 0.20,
                            ) {
                                *act = Some(Act::Sort(ExplorerSortKey::Modified));
                            }
                        });
                    });
            }

            ScrollArea::vertical()
                .max_height(table_h)
                .auto_shrink([false, false])
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                .id_salt("explorer_files")
                .show(ui, |ui| {
                    if visible.is_empty() {
                        empty_state(app, ui);
                    } else if app.explorer_group_by_kind {
                        for (cat, label) in CAT_ORDER {
                            let group: Vec<usize> = visible
                                .iter()
                                .copied()
                                .filter(|&i| category(&app.explorer_entries[i]) == cat)
                                .collect();
                            if group.is_empty() {
                                continue;
                            }
                            group_header(ui, label, group.len());
                            render_items(app, ui, &group, is_list, act);
                        }
                    } else {
                        render_items(app, ui, &visible, is_list, act);
                    }
                });
        });
}

fn render_items(
    app: &mut SmartFileManagerApp,
    ui: &mut egui::Ui,
    indices: &[usize],
    is_list: bool,
    act: &mut Option<Act>,
) {
    if is_list {
        for (row, &idx) in indices.iter().enumerate() {
            row_ui(app, ui, idx, row, act);
        }
    } else {
        // Explicit rows so the grid wraps and the vertical ScrollArea scrolls.
        let avail = ui.available_width() - 24.0;
        let cell = TILE_W + 12.0;
        let cols = ((avail / cell).floor() as usize).max(1).min(6);
        
        let total_card_width = cols as f32 * TILE_W;
        let remaining_width = avail - total_card_width;
        let spacing_x = if cols > 1 {
            (remaining_width / (cols - 1) as f32).clamp(12.0, 48.0)
        } else {
            12.0
        };
        
        ui.add_space(8.0);
        for chunk in indices.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = spacing_x;
                for &idx in chunk {
                    tile_ui(app, ui, idx, act);
                }
            });
            ui.add_space(12.0);
        }
    }
}

const TILE_W: f32 = 150.0;
const TILE_H: f32 = 180.0;

fn tile_ui(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, idx: usize, act: &mut Option<Act>) {
    let e = app.explorer_entries[idx].clone();
    let selected = app.explorer_selected == Some(idx);
    let checked = app.explorer_is_recursive_search && app.explorer_search_checked.contains(&e.path);
    let fill = if selected || checked {
        Color32::from_rgb(40, 58, 96)
    } else {
        Color32::from_rgb(30, 30, 38)
    };
    let mut checkbox_clicked = false;
    let resp = egui::Frame::none()
        .fill(fill)
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(0.0))
        .show(ui, |ui| {
            ui.set_width(TILE_W);
            ui.set_height(TILE_H);
            ui.vertical(|ui| {
                if app.explorer_is_recursive_search {
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        let mut is_on = app.explorer_search_checked.contains(&e.path);
                        let cb = ui.checkbox(&mut is_on, "");
                        if cb.changed() {
                            if is_on {
                                app.explorer_search_checked.insert(e.path.clone());
                            } else {
                                app.explorer_search_checked.remove(&e.path);
                            }
                            checkbox_clicked = true;
                        } else if cb.clicked() {
                            checkbox_clicked = true;
                        }
                    });
                }
                ui.allocate_ui(egui::vec2(TILE_W, TILE_H * 0.7), |ui| {
                    draw_preview_or_icon(app, ui, egui::vec2(TILE_W, TILE_H * 0.7), &e);
                });

                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .show(ui, |ui| {
                        ui.set_width(TILE_W);
                        ui.vertical_centered(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(elide(&e.name, 22))
                                        .color(Color32::from_rgb(224, 228, 244))
                                        .size(11.5),
                                )
                                .truncate(),
                            );
                            if !e.is_dir {
                                ui.add_space(2.0);
                                ui.label(muted(format_bytes(e.size)));
                            }
                        });
                    });
            });
        })
        .response
        .interact(egui::Sense::click());

    if !checkbox_clicked {
        item_interactions(&resp, &e, act);
    }
    // Show file name + relative path (in recursive mode) in tooltip
    let hover_text = if app.explorer_is_recursive_search {
        if let Some(rel) = e.path
            .parent()
            .and_then(|p| p.strip_prefix(&app.explorer_dir).ok())
            .filter(|p| p.components().next().is_some())
            .map(|p| p.to_string_lossy().to_string())
        {
            format!("{} ({})", e.name, rel)
        } else {
            e.name.clone()
        }
    } else {
        e.name.clone()
    };
    resp.on_hover_text(&hover_text);
}

fn elide(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max - 1).collect();
        format!("{head}…")
    }
}

fn empty_state(app: &SmartFileManagerApp, ui: &mut egui::Ui) {
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        let filtering = !app.explorer_filter.trim().is_empty();
        if filtering {
            ui.label(RichText::new("🔍").size(34.0));
            ui.add_space(6.0);
            ui.label(muted("Nothing matches the current filter."));
            return;
        }
        ui.label(RichText::new("🚫").size(34.0));
        ui.add_space(8.0);
        ui.label(
            RichText::new("No items to show")
                .strong()
                .size(15.0)
                .color(Color32::from_rgb(214, 219, 236)),
        );
        ui.add_space(6.0);
        // Protected user folders return empty under macOS privacy controls.
        if is_protected_dir(&app.explorer_dir) {
            ui.label(muted(
                "This may be empty, or macOS is blocking access to this folder.",
            ));
            ui.add_space(2.0);
            ui.label(muted(
                "Grant access under Privacy & Security → Files and Folders / Full Disk Access, then Refresh.",
            ));
            ui.add_space(12.0);
            #[cfg(target_os = "macos")]
            if secondary_button(ui, "Open Privacy Settings").clicked() {
                let _ = std::process::Command::new("open")
                    .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
                    .spawn();
            }
        } else {
            ui.label(muted("This folder is empty."));
        }
    });
}

fn is_protected_dir(dir: &std::path::Path) -> bool {
    [
        dirs::desktop_dir(),
        dirs::document_dir(),
        dirs::download_dir(),
    ]
    .into_iter()
    .flatten()
    .any(|p| dir.starts_with(&p))
}

fn header(ui: &mut egui::Ui, label: &str, width: f32) -> bool {
    let mut clicked = false;
    ui.allocate_ui(egui::vec2(width, 20.0), |ui| {
        if ui
            .add(
                egui::Label::new(
                    RichText::new(label)
                        .strong()
                        .color(Color32::from_rgb(186, 192, 214)),
                )
                .sense(egui::Sense::click()),
            )
            .clicked()
        {
            clicked = true;
        }
    });
    clicked
}

fn group_header(ui: &mut egui::Ui, label: &str, count: usize) {
    egui::Frame::none()
        .fill(Color32::from_rgb(30, 30, 38))
        .inner_margin(egui::Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{label}  ({count})"))
                        .strong()
                        .size(12.0)
                        .color(Color32::from_rgb(150, 180, 240)),
                );
            });
        });
}

fn row_ui(
    app: &mut SmartFileManagerApp,
    ui: &mut egui::Ui,
    idx: usize,
    row: usize,
    act: &mut Option<Act>,
) {
    let e = app.explorer_entries[idx].clone();
    let selected = app.explorer_selected == Some(idx);
    let checked = app.explorer_is_recursive_search && app.explorer_search_checked.contains(&e.path);
    let fill = if selected || checked {
        Color32::from_rgb(40, 58, 96)
    } else if row % 2 == 0 {
        Color32::from_rgb(27, 27, 34)
    } else {
        Color32::from_rgb(24, 24, 30)
    };
    // Compute relative path for recursive search display
    let rel_path: Option<String> = if app.explorer_is_recursive_search {
        e.path
            .parent()
            .and_then(|p| p.strip_prefix(&app.explorer_dir).ok())
            .filter(|p| p.components().next().is_some())
            .map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    let mut checkbox_clicked = false;
    let resp = egui::Frame::none()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if app.explorer_is_recursive_search {
                    let mut is_on = app.explorer_search_checked.contains(&e.path);
                    let cb = ui.checkbox(&mut is_on, "");
                    if cb.changed() {
                        if is_on {
                            app.explorer_search_checked.insert(e.path.clone());
                        } else {
                            app.explorer_search_checked.remove(&e.path);
                        }
                        checkbox_clicked = true;
                    } else if cb.clicked() {
                        checkbox_clicked = true;
                    }
                }
                let full = ui.available_width();
                ui.allocate_ui(egui::vec2(full * 0.46, 30.0), |ui| {
                    ui.horizontal(|ui| {
                        draw_preview_or_icon(app, ui, egui::vec2(16.0, 16.0), &e);
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&e.name).color(Color32::from_rgb(224, 228, 244)));
                            if let Some(ref rel) = rel_path {
                                ui.label(
                                    RichText::new(rel)
                                        .color(Color32::from_rgb(110, 125, 160))
                                        .size(10.5),
                                );
                            }
                        });
                    });
                });
                ui.allocate_ui(egui::vec2(full * 0.14, 30.0), |ui| {
                    ui.label(muted(if e.is_dir { "—".to_string() } else { format_bytes(e.size) }));
                });
                ui.allocate_ui(egui::vec2(full * 0.16, 30.0), |ui| {
                    ui.label(muted(entry_kind(&e)));
                });
                ui.allocate_ui(egui::vec2(full * 0.20, 30.0), |ui| {
                    ui.label(muted(fmt_time(e.modified)));
                });
            });
        })
        .response
        .interact(egui::Sense::click());

    if !checkbox_clicked {
        item_interactions(&resp, &e, act);
    }
}

/// Shared click / double-click / right-click behaviour for list rows and grid tiles.
fn item_interactions(resp: &egui::Response, e: &ExplorerEntry, act: &mut Option<Act>) {
    // Left click: folders open, files preview.
    if resp.clicked() {
        *act = Some(if e.is_dir {
            Act::Nav(e.path.clone())
        } else {
            Act::Preview(e.path.clone())
        });
    }
    if resp.double_clicked() {
        *act = Some(if e.is_dir {
            Act::Nav(e.path.clone())
        } else if crate::ui::preview::is_video_path(&e.path) || crate::ui::preview::is_image_path(&e.path) {
            Act::Preview(e.path.clone())
        } else {
            Act::OpenDefault(e.path.clone())
        });
    }

    resp.context_menu(|ui| {
        if e.is_dir {
            if ui.button("Open").clicked() {
                *act = Some(Act::Nav(e.path.clone()));
                ui.close_menu();
            }
            if ui.button("Open in player").clicked() {
                *act = Some(Act::Preview(e.path.clone()));
                ui.close_menu();
            }
        } else {
            if ui.button("Open in player").clicked() {
                *act = Some(Act::Preview(e.path.clone()));
                ui.close_menu();
            }
            if ui.button("Open in default app").clicked() {
                *act = Some(Act::OpenDefault(e.path.clone()));
                ui.close_menu();
            }
        }
        if is_zip(e) && ui.button("Extract here").clicked() {
            *act = Some(Act::Extract(e.path.clone()));
            ui.close_menu();
        }
        if ui.button("Reveal in Finder / Explorer").clicked() {
            *act = Some(Act::Reveal(e.path.clone()));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Move to Trash").clicked() {
            *act = Some(Act::Trash(e.path.clone()));
            ui.close_menu();
        }
        ui.separator();
        if ui.button("New folder here").clicked() {
            *act = Some(Act::NewFolder);
            ui.close_menu();
        }
        if ui.button("Refresh").clicked() {
            *act = Some(Act::Refresh);
            ui.close_menu();
        }
    });
}

fn selection_bar(app: &SmartFileManagerApp, ui: &mut egui::Ui, _act: &mut Option<Act>) {
    egui::Frame::none()
        .fill(Color32::from_rgb(30, 30, 38))
        .rounding(egui::Rounding::same(7.0))
        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
        .show(ui, |ui| {
            let total = app.explorer_entries.len();
            let folders = app.explorer_entries.iter().filter(|e| e.is_dir).count();
            ui.horizontal(|ui| {
                ui.label(muted(format!(
                    "{total} items · {folders} folders · {} files",
                    total - folders
                )));
            });
        });
}

// ---------------------------------------------------------------------------
// Modals
// ---------------------------------------------------------------------------

// Removed CPU rotate_color_image in favor of GPU-accelerated egui::Image::rotate rendering.

/// Full media player overlay: large centered image/video with ◀ / ▶ navigation
/// across the folder's files (current filter + sort order). Arrow keys also work.
pub fn player_window(app: &mut SmartFileManagerApp, ctx: &egui::Context) {
    if !app.explorer_preview_open {
        return;
    }
    let Some(path) = app.explorer_preview_path.clone() else {
        return;
    };

    // 1. Draw a dark translucent modal backdrop overlay to dim the background and block clicks
    egui::Area::new(egui::Id::new("player_modal_backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(true)
        .show(ctx, |ui| {
            let screen_rect = ui.ctx().screen_rect();
            ui.painter().rect_filled(screen_rect, 0.0, Color32::from_black_alpha(85));
        });

    let filter = app.explorer_filter.trim().to_lowercase();
    let type_filter = app.explorer_type_filter;
    // Include folders so prev/next can land on them and “➕ Queue” can stage a folder.
    let slides: Vec<(PathBuf, bool)> = app
        .explorer_entries
        .iter()
        .filter(|e| {
            matches_type_filter(e, type_filter)
                && (filter.is_empty() || e.name.to_lowercase().contains(&filter))
        })
        .map(|e| (e.path.clone(), e.is_dir))
        .collect();
    let cur = slides.iter().position(|(p, _)| p == &path);
    let can_prev = cur.map(|i| i > 0).unwrap_or(false);
    let can_next = cur.map(|i| i + 1 < slides.len()).unwrap_or(false);
    let path_is_dir = slides
        .iter()
        .find(|(p, _)| p == &path)
        .map(|(_, d)| *d)
        .unwrap_or_else(|| path.is_dir());

    let mut goto: Option<PathBuf> = None;
    if !ctx.wants_keyboard_input() {
        if can_next && ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            goto = Some(slides[cur.unwrap() + 1].0.clone());
        }
        if can_prev && ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            goto = Some(slides[cur.unwrap() - 1].0.clone());
        }
    }

    #[cfg(feature = "mpv")]
    let is_video = !path_is_dir && crate::ui::preview::is_video_path(&path);

    // Use embedded libmpv for video when available; otherwise the image/text loader.
    #[cfg(feature = "mpv")]
    let mpv_active = is_video && ensure_mpv(app, ctx, &path);
    #[cfg(not(feature = "mpv"))]
    let mpv_active = false;

    #[cfg(feature = "mpv")]
    if !mpv_active && app.mpv.is_some() {
        if let Some(p) = app.mpv.as_ref() {
            p.stop(); // stop playback/audio when leaving a video without dropping context
        }
        app.mpv_loaded_path = None;
    }

    if path_is_dir {
        // Avoid stale media from the previous slide.
        app.preview_texture = None;
        app.preview_scroll_text = None;
        app.inline_video = None;
        app.preview_path = None;
    } else if !mpv_active {
        super::results_panel::update_preview(ctx, app, &path, app.explorer_preview_rotation);
    }

    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let mut close = false;
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    let mut seek_to: Option<f64> = None;

    // Fixed window size: ~82% w × 86% h, larger caps so media can fill big displays.
    let screen = ctx.screen_rect().size();
    let win_w = (screen.x * 0.82).clamp(520.0, 1200.0);
    let win_h = (screen.y * 0.86).clamp(380.0, 900.0);
    let win_x = (screen.x - win_w) / 2.0;
    let win_y = (screen.y - win_h) / 2.0;

    let frame = egui::Frame::window(&ctx.style())
        .fill(Color32::from_rgb(20, 20, 26))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(55, 58, 74)))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(0.0)); // 0 margin for edge-to-edge full width layout!

    egui::Window::new("Media Player")
        .frame(frame)
        .title_bar(false) // Disable default title bar since we render a gorgeous custom one inside
        .collapsible(false)
        .resizable(false)
        .fixed_pos([win_x, win_y])
        .fixed_size([win_w, win_h])
        .show(ctx, |ui| {
            // Header: fixed RTL order so Trash never jumps —
            // Close · Trash · Reveal · Open · Folder · Rotate · N/total
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&name)
                                .strong()
                                .size(15.0)
                                .color(Color32::from_rgb(232, 236, 250)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("✕  Close")
                                            .strong()
                                            .color(Color32::from_rgb(240, 200, 200)),
                                    )
                                    .fill(Color32::from_rgb(50, 30, 34))
                                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(90, 50, 56))),
                                )
                                .clicked()
                            {
                                close = true;
                            }
                            ui.add_space(4.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("🗑")
                                            .size(12.0)
                                            .color(Color32::from_rgb(255, 120, 120)),
                                    )
                                    .fill(Color32::from_rgb(45, 25, 28))
                                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(90, 50, 56)))
                                    .min_size(egui::vec2(28.0, 24.0)),
                                )
                                .on_hover_text(if path_is_dir {
                                    "Move this folder to Trash / Recycle Bin"
                                } else {
                                    "Move this file to Trash / Recycle Bin"
                                })
                                .clicked()
                            {
                                let trash_name =
                                    path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                let next_preview = if let Some(i) = cur {
                                    if i + 1 < slides.len() {
                                        Some(slides[i + 1].0.clone())
                                    } else if i > 0 {
                                        Some(slides[i - 1].0.clone())
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };
                                match trash_single_path(&path) {
                                    Ok(()) => {
                                        app.explorer_status =
                                            Some(format!("Moved “{trash_name}” to Trash."));
                                        app.explorer_selected = None;
                                        app.explorer_reload();
                                        if let Some(next_p) = next_preview {
                                            goto = Some(next_p);
                                        } else {
                                            close = true;
                                        }
                                    }
                                    Err(e) => {
                                        app.explorer_status = Some(format!("Trash failed: {e}"));
                                    }
                                }
                            }
                            ui.add_space(2.0);
                            if ui
                                .add(preview_header_btn("📂 Reveal"))
                                .on_hover_text("Reveal in Finder / Explorer")
                                .clicked()
                            {
                                reveal_in_folder(&path);
                            }
                            ui.add_space(2.0);
                            let open_tip = if app.inline_video.is_some() || mpv_active {
                                "Open in system player (with audio)"
                            } else {
                                "Open in default app"
                            };
                            if ui
                                .add(preview_header_btn("↗ Open"))
                                .on_hover_text(open_tip)
                                .clicked()
                            {
                                open_in_default_app(&path);
                            }
                            ui.add_space(2.0);
                            // Folder slide → queue this folder; file → queue its parent.
                            let queue_folder_path = if path_is_dir {
                                Some(path.clone())
                            } else {
                                path.parent().map(|p| p.to_path_buf())
                            };
                            let folder_tip = queue_folder_path
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .map(|n| {
                                    format!(
                                        "Queue folder “{}” (moves on Submit)",
                                        n.to_string_lossy()
                                    )
                                })
                                .unwrap_or_else(|| "Queue folder (moves on Submit)".into());
                            if ui
                                .add(preview_header_btn("📁"))
                                .on_hover_text(folder_tip)
                                .clicked()
                            {
                                if let Some(folder) = queue_folder_path {
                                    let before = app.explorer_queues.len();
                                    if before == 0 {
                                        app.explorer_new_queue();
                                    }
                                    if !app.explorer_queues.is_empty() {
                                        if before == 0 && app.explorer_queues.len() > before {
                                            app.explorer_preview_queue_pick =
                                                Some(app.explorer_queues.len() - 1);
                                        }
                                        if let Some(i) = app
                                            .explorer_preview_queue_pick
                                            .filter(|&i| i < app.explorer_queues.len())
                                        {
                                            app.explorer_add_to_queue(i, folder);
                                        } else {
                                            app.explorer_status =
                                                Some("Choose a queue first".to_string());
                                        }
                                    }
                                }
                            }
                            ui.add_space(2.0);
                            if ui
                                .add(preview_header_btn("🔄"))
                                .on_hover_text("Rotate")
                                .clicked()
                            {
                                app.explorer_preview_rotation =
                                    (app.explorer_preview_rotation + 90) % 360;
                                // Keep paint/update paths in sync without wiping the modal.
                                app.preview_rotation = app.explorer_preview_rotation;
                                #[cfg(feature = "mpv")]
                                if mpv_active {
                                    if let Some(p) = app.mpv.as_ref() {
                                        p.set_rotation(app.explorer_preview_rotation as i64);
                                    }
                                }
                            }
                            if let Some(i) = cur {
                                ui.add_space(8.0);
                                ui.label(muted(format!("{} / {}", i + 1, slides.len())));
                            }
                        });
                    });
                });

            // Media stage — reserve timeline + one control row (transport+queue or image+queue).
            let has_video_transport = mpv_active || app.inline_video.is_some();
            let footer_reserve = if has_video_transport { 72.0 } else { 52.0 };
            let media_w = ui.available_width();
            let row_h = (ui.available_height() - footer_reserve).max(180.0);
            ui.allocate_ui_with_layout(
                egui::vec2(media_w, row_h),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.set_min_size(egui::vec2(media_w, row_h));
                    ui.set_max_size(egui::vec2(media_w, row_h));
                    if path_is_dir {
                        player_folder_slide(ui, &name, egui::vec2(media_w, row_h));
                    } else {
                        #[cfg(feature = "mpv")]
                        if mpv_active {
                            mpv_media(app, ui, ctx);
                        } else {
                            player_media(app, ui, ctx, egui::vec2(media_w, row_h));
                        }
                        #[cfg(not(feature = "mpv"))]
                        player_media(app, ui, ctx, egui::vec2(media_w, row_h));
                    }
                },
            );

            // Video: timeline + ONE row (transport + queue + volume). No second footer.
            #[cfg(feature = "mpv")]
            if mpv_active {
                if let Some(next_path) =
                    mpv_transport(app, ui, ctx, can_prev, &slides, cur, can_next, &path)
                {
                    goto = Some(next_path);
                }
            }

            if !mpv_active && app.inline_video.is_some() {
                let (dur, pos, is_paused) = {
                    let dec = app.inline_video.as_ref().unwrap();
                    let dur = dec.duration();
                    let pos = dec.position().min(if dur > 0.0 { dur } else { f64::MAX });
                    let is_paused = dec.is_paused();
                    (dur, pos, is_paused)
                };
                let mut toggle = false;

                ui.vertical(|ui| {
                    if dur > 0.0 {
                        let mut v = pos as f32;
                        let slider_w = ui.available_width();
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = slider_w;
                            let slider = ui.add_sized(
                                [slider_w, 6.0],
                                egui::Slider::new(&mut v, 0.0..=dur as f32).show_value(false),
                            );
                            if slider.drag_stopped() || slider.lost_focus() {
                                seek_to = Some(v as f64);
                            }
                        });
                    }

                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let prev_btn = egui::Button::new(RichText::new("⏮").size(15.0).strong())
                                    .fill(Color32::from_rgb(28, 30, 40))
                                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(50, 52, 68)))
                                    .min_size(egui::vec2(32.0, 26.0));
                                if ui.add_enabled(can_prev, prev_btn).clicked() {
                                    goto = Some(slides[cur.unwrap() - 1].0.clone());
                                }

                                let icon = if is_paused { "▶" } else { "⏸" };
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(icon).size(16.0))
                                            .min_size(egui::vec2(36.0, 26.0)),
                                    )
                                    .on_hover_text("Play / Pause (Space)")
                                    .clicked()
                                {
                                    toggle = true;
                                }
                                if !ctx.wants_keyboard_input()
                                    && ctx.input(|i| i.key_pressed(egui::Key::Space))
                                {
                                    toggle = true;
                                }

                                let next_btn = egui::Button::new(RichText::new("⏭").size(15.0).strong())
                                    .fill(Color32::from_rgb(28, 30, 40))
                                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(50, 52, 68)))
                                    .min_size(egui::vec2(32.0, 26.0));
                                if ui.add_enabled(can_next, next_btn).clicked() {
                                    goto = Some(slides[cur.unwrap() + 1].0.clone());
                                }

                                ui.add_space(6.0);
                                if dur > 0.0 {
                                    ui.label(
                                        RichText::new(format!(
                                            "{}  /  {}",
                                            fmt_clock(pos),
                                            fmt_clock(dur)
                                        ))
                                        .color(Color32::from_rgb(180, 185, 200))
                                        .size(13.0),
                                    );
                                } else {
                                    ui.label(muted("live"));
                                }

                                ui.add_space(10.0);
                                preview_queue_controls(app, ui, &path, path_is_dir);
                            });
                        });
                });

                if toggle {
                    if let Some(dec) = app.inline_video.as_ref() {
                        dec.toggle_pause();
                    }
                }
            }

            // Images / folders / other: single row — prev/next + queue.
            if !has_video_transport {
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let prev_btn = egui::Button::new(RichText::new("⏮").size(14.0).strong())
                                .fill(Color32::from_rgb(28, 30, 40))
                                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(50, 52, 68)))
                                .min_size(egui::vec2(32.0, 26.0));
                            if ui
                                .add_enabled(can_prev, prev_btn)
                                .on_hover_text("Previous")
                                .clicked()
                            {
                                goto = Some(slides[cur.unwrap() - 1].0.clone());
                            }
                            let next_btn = egui::Button::new(RichText::new("⏭").size(14.0).strong())
                                .fill(Color32::from_rgb(28, 30, 40))
                                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(50, 52, 68)))
                                .min_size(egui::vec2(32.0, 26.0));
                            if ui
                                .add_enabled(can_next, next_btn)
                                .on_hover_text("Next")
                                .clicked()
                            {
                                goto = Some(slides[cur.unwrap() + 1].0.clone());
                            }
                            ui.add_space(8.0);
                            preview_queue_controls(app, ui, &path, path_is_dir);
                        });
                    });
            }
        });

    if let Some(secs) = seek_to {
        // Restart the decoder at the requested offset.
        app.preview_texture = None;
        app.inline_video = crate::ui::video_inline::InlineVideoDecoder::spawn_at(&path, secs).ok();
    }

    if let Some(next) = goto {
        app.explorer_open_preview(next);
    } else if close {
        app.close_explorer_preview();
    }
}

/// Queue detail modal: shows the active queue's staged files, its destination
/// folder, and a Submit button that moves everything into that folder.
pub fn queue_window(app: &mut SmartFileManagerApp, ctx: &egui::Context) {
    if !app.explorer_queue_view_open {
        return;
    }

    // Backdrop that dims the page and swallows background clicks.
    egui::Area::new(egui::Id::new("queue_modal_backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(true)
        .show(ctx, |ui| {
            let screen_rect = ui.ctx().screen_rect();
            ui.painter().rect_filled(screen_rect, 0.0, Color32::from_black_alpha(85));
        });

    let screen = ctx.screen_rect().size();
    let win_w = (screen.x * 0.56).clamp(420.0, 760.0);
    let win_h = (screen.y * 0.68).clamp(340.0, 680.0);
    let win_x = (screen.x - win_w) / 2.0;
    let win_y = (screen.y - win_h) / 2.0;

    let frame = egui::Frame::window(&ctx.style())
        .fill(Color32::from_rgb(20, 20, 26))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(55, 58, 74)))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(0.0));

    let mut close = false;
    let mut act: Option<Act> = None;
    let submitting = app.explorer_queue_submitting;

    egui::Window::new("Queue")
        .frame(frame)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_pos([win_x, win_y])
        .fixed_size([win_w, win_h])
        .show(ctx, |ui| {
            // Header row.
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("🗂  Queues")
                                .strong()
                                .size(15.0)
                                .color(Color32::from_rgb(232, 236, 250)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("✕  Close")
                                            .strong()
                                            .color(Color32::from_rgb(240, 200, 200)),
                                    )
                                    .fill(Color32::from_rgb(50, 30, 34))
                                    .stroke(egui::Stroke::new(1.0, Color32::from_rgb(90, 50, 56))),
                                )
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
                });

            ui.separator();

            if app.explorer_queues.is_empty() {
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(16.0, 16.0))
                    .show(ui, |ui| {
                        ui.label(muted("No queues yet. Create one to start staging files."));
                        ui.add_space(10.0);
                        if primary_button(ui, "＋ New Queue").clicked() {
                            act = Some(Act::NewQueue);
                        }
                    });
            } else {
                // Queue selector tabs.
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (i, q) in app.explorer_queues.iter().enumerate() {
                                let active = app.explorer_active_queue == Some(i);
                                let (fg, bg) = if active {
                                    (Color32::from_rgb(248, 250, 255), style::ACCENT)
                                } else {
                                    (Color32::from_rgb(196, 202, 222), Color32::from_rgb(40, 42, 52))
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new(format!("{} ({})", q.name, q.items.len()))
                                                .color(fg),
                                        )
                                        .fill(bg),
                                    )
                                    .clicked()
                                {
                                    act = Some(Act::OpenQueue(i));
                                }
                            }
                            if secondary_button(ui, "＋ New").clicked() {
                                act = Some(Act::NewQueue);
                            }
                        });
                    });

                let idx = app.explorer_active_queue.unwrap_or(0).min(app.explorer_queues.len() - 1);
                let q = &app.explorer_queues[idx];
                let target = q.target.clone();
                let qname = q.name.clone();
                let items: Vec<PathBuf> = q.items.clone();

                // Target folder row.
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(16.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(muted("Folder:"));
                            ui.label(
                                RichText::new(target.display().to_string())
                                    .color(Color32::from_rgb(150, 175, 230))
                                    .size(12.5),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if !submitting && secondary_button(ui, "Change…").clicked() {
                                    act = Some(Act::ChangeQueueTarget(idx));
                                }
                            });
                        });
                    });

                ui.add_space(4.0);

                // Staged file list.
                let list_h = (win_h - 240.0).max(120.0);
                egui::Frame::none()
                    .fill(Color32::from_rgb(24, 24, 30))
                    .stroke(egui::Stroke::new(1.0, style::BORDER))
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(6.0))
                    .outer_margin(egui::Margin::symmetric(16.0, 4.0))
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .max_height(list_h)
                            .auto_shrink([false, false])
                            .id_salt("queue_items")
                            .show(ui, |ui| {
                                if items.is_empty() {
                                    ui.add_space(8.0);
                                    ui.label(muted("No items staged. Add files or folders from the player."));
                                    ui.add_space(8.0);
                                } else {
                                    for p in &items {
                                        let is_dir = p.is_dir();
                                        let name = p
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .to_string();
                                        let label = if is_dir {
                                            format!("📁 {name}/")
                                        } else {
                                            name
                                        };
                                        ui.horizontal(|ui| {
                                            if ui
                                                .add(egui::Button::new(
                                                    RichText::new("✕")
                                                        .color(Color32::from_rgb(230, 150, 150)),
                                                ).frame(false))
                                                .on_hover_text("Remove from queue")
                                                .clicked()
                                            {
                                                act = Some(Act::RemoveFromQueue(idx, p.clone()));
                                            }
                                            ui.label(
                                                RichText::new(label)
                                                    .color(if is_dir {
                                                        Color32::from_rgb(160, 195, 240)
                                                    } else {
                                                        Color32::from_rgb(214, 219, 236)
                                                    }),
                                            )
                                            .on_hover_text(p.display().to_string());
                                        });
                                    }
                                }
                            });
                    });

                // Submit progress.
                if submitting {
                    let (done, total, _fname) = app
                        .explorer_queue_submit_progress
                        .lock()
                        .map(|g| g.clone())
                        .unwrap_or((0, 0, String::new()));
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(16.0, 6.0))
                        .show(ui, |ui| {
                            ui.label(muted(format!("Moving… {done}/{total}")));
                        });
                }

                // Footer actions.
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(16.0, 10.0))
                    .show(ui, |ui| {
                        ui.separator();
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let total_all_queues: usize = app.explorer_queues.iter().map(|q| q.items.len()).sum();
                            let can_submit = !submitting && total_all_queues > 0;
                            let submit_label = format!("📦 Move All ({total_all_queues} files)");
                            
                            if ui
                                .add_enabled(can_submit, {
                                    egui::Button::new(
                                        RichText::new(submit_label)
                                            .strong()
                                            .color(Color32::from_rgb(248, 250, 255)),
                                    )
                                    .fill(style::ACCENT)
                                })
                                .on_hover_text("Move all staged items to the selected destination folder")
                                .clicked()
                            {
                                act = Some(Act::SubmitAllQueues);
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if !submitting
                                    && ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("🗑  Delete queue")
                                                    .color(Color32::from_rgb(255, 120, 120)),
                                            )
                                            .fill(Color32::from_rgb(45, 25, 28))
                                            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(90, 50, 56))),
                                        )
                                        .on_hover_text(format!("Delete “{qname}” (files are not touched)"))
                                        .clicked()
                                {
                                    act = Some(Act::DeleteQueue(idx));
                                }
                            });
                        });
                    });
            }
        });

    // Esc closes when not typing elsewhere.
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }

    apply_action(app, act);
    if close {
        app.explorer_queue_view_open = false;
    }
}

fn preview_header_btn(label: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.to_owned()).size(12.0))
        .fill(Color32::from_rgb(34, 36, 48))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(60, 64, 80)))
        .min_size(egui::vec2(28.0, 24.0))
}

/// Queue combo / New Queue + Add — meant to sit on the same row as transport/nav.
fn preview_queue_controls(
    app: &mut SmartFileManagerApp,
    ui: &mut egui::Ui,
    path: &PathBuf,
    is_dir: bool,
) {
    let queue_names: Vec<String> = app
        .explorer_queues
        .iter()
        .map(|q| format!("{} ({})", q.name, q.items.len()))
        .collect();
    if queue_names.is_empty() {
        if secondary_button(ui, "＋ New Queue").clicked() {
            app.explorer_new_queue();
        }
        return;
    }
    let mut pick = app
        .explorer_preview_queue_pick
        .filter(|&i| i < queue_names.len());
    let selected_text = pick
        .map(|i| queue_names[i].clone())
        .unwrap_or_else(|| "Choose a queue…".to_string());
    egui::ComboBox::new(egui::Id::new("preview_queue_pick"), "")
        .width(180.0)
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            for (i, name) in queue_names.iter().enumerate() {
                ui.selectable_value(&mut pick, Some(i), name);
            }
        });
    app.explorer_preview_queue_pick = pick;
    let tip = if is_dir {
        "Stage this folder (moves on Submit)"
    } else {
        "Stage this file (moves on Submit)"
    };
    if ui
        .add_enabled(
            pick.is_some(),
            egui::Button::new(
                RichText::new("➕ Queue")
                    .strong()
                    .color(Color32::from_rgb(248, 250, 255)),
            )
            .fill(style::ACCENT),
        )
        .on_hover_text(tip)
        .clicked()
    {
        if let Some(i) = pick {
            app.explorer_add_to_queue(i, path.clone());
        }
    }
}

fn player_folder_slide(ui: &mut egui::Ui, name: &str, area: egui::Vec2) {
    let stage = egui::vec2(
        area.x.min(ui.available_width()).max(120.0),
        area.y.min(ui.available_height()).max(120.0),
    );
    egui::Frame::none()
        .fill(Color32::from_rgb(10, 10, 14))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.set_min_size(stage);
            ui.set_max_size(stage);
            let inner = ui.available_size();
            ui.allocate_ui_with_layout(inner, egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.add_space((inner.y / 2.0 - 60.0).max(10.0));
                ui.label(
                    RichText::new("📁")
                        .size(56.0)
                        .color(Color32::from_rgb(160, 195, 240)),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!("{name}/"))
                        .strong()
                        .size(18.0)
                        .color(Color32::from_rgb(232, 236, 250)),
                );
                ui.add_space(6.0);
                ui.label(muted("Folder — use ➕ Queue to stage the whole folder"));
            });
        });
}

fn fmt_clock(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "0:00".to_string();
    }
    let total = secs as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn player_media(
    app: &mut SmartFileManagerApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    area: egui::Vec2,
) {
    let stage = egui::vec2(
        area.x.min(ui.available_width()).max(120.0),
        area.y.min(ui.available_height()).max(120.0),
    );

    let has_visual = app.preview_texture.is_some() || app.inline_video.is_some();
    let media_margin = if has_visual {
        egui::Margin::same(0.0)
    } else {
        egui::Margin::symmetric(12.0, 8.0)
    };

    egui::Frame::none()
        .fill(Color32::from_rgb(10, 10, 14))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(media_margin)
        .show(ui, |ui| {
            // Lock the stage so large textures cannot expand the modal.
            ui.set_min_size(stage);
            ui.set_max_size(stage);
            let inner = ui.available_size();

            // Pull the freshest decoded video frame, if any.
            if let Some(dec) = app.inline_video.as_ref() {
                let mut latest = None;
                while let Some(f) = dec.pop_frame() {
                    latest = Some(f);
                }
                if let Some(img) = latest {
                    match app.preview_texture.as_mut() {
                        None => {
                            app.preview_texture =
                                Some(ctx.load_texture("sfm_player_video", img, TextureOptions::LINEAR));
                        }
                        Some(t) => t.set(img, TextureOptions::LINEAR),
                    }
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(32));
            }

            if let Some(tex) = &app.preview_texture {
                let [tw, th] = tex.size();
                let (tw, th) = (tw as f32, th as f32);
                let rotation = app.explorer_preview_rotation;
                let is_rotated_90_or_270 = rotation == 90 || rotation == 270;
                let (display_w, display_h) = if is_rotated_90_or_270 {
                    (th, tw)
                } else {
                    (tw, th)
                };

                // Fit inside stage (never allocate larger than the viewport).
                let scale = if display_w > 0.0 && display_h > 0.0 {
                    (inner.x / display_w).min(inner.y / display_h)
                } else {
                    1.0
                };

                let (rect, _response) = ui.allocate_exact_size(inner, egui::Sense::hover());
                let tex_size = egui::vec2(tw * scale, th * scale);
                let image_rect = egui::Rect::from_center_size(rect.center(), tex_size);

                let angle_rad = (rotation as f32).to_radians();
                let image_widget = egui::Image::from_texture(SizedTexture::from_handle(tex))
                    .rotate(angle_rad, egui::Vec2::splat(0.5));
                image_widget.paint_at(ui, image_rect);
            } else if let Some(text) = app.preview_scroll_text.clone() {
                ScrollArea::both()
                    .auto_shrink([false, false])
                    .max_width(inner.x)
                    .max_height(inner.y)
                    .id_salt("player_text")
                    .show(ui, |ui| {
                        ui.set_max_width(inner.x);
                        ui.add(
                            egui::Label::new(
                                RichText::new(text).font(egui::FontId::monospace(12.0)),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
            } else {
                ui.allocate_ui_with_layout(inner, egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.add_space((inner.y / 2.0 - 40.0).max(10.0));
                    ui.label(
                        RichText::new("🔍")
                            .size(34.0)
                            .color(Color32::from_rgb(150, 156, 176)),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            app.preview_subtitle.clone().unwrap_or_else(|| {
                                "No inline preview — use “Open in default app”.".into()
                            }),
                        )
                        .color(Color32::from_rgb(180, 185, 200)),
                    );
                });
            }
        });
}

#[cfg(feature = "mpv")]
struct PlayerPtr(*mut crate::ui::mpv_player::MpvPlayer);
#[cfg(feature = "mpv")]
impl PlayerPtr {
    fn get(&self) -> *mut crate::ui::mpv_player::MpvPlayer {
        self.0
    }
}
#[cfg(feature = "mpv")]
unsafe impl Send for PlayerPtr {}
#[cfg(feature = "mpv")]
unsafe impl Sync for PlayerPtr {}

#[cfg(feature = "mpv")]
fn ensure_mpv(app: &mut SmartFileManagerApp, _ctx: &egui::Context, path: &std::path::Path) -> bool {
    if app.mpv.is_none() {
        match crate::ui::mpv_player::MpvPlayer::new() {
            Ok(p) => app.mpv = Some(p),
            Err(e) => {
                app.explorer_status = Some(format!("Failed to initialize MPV player: {}", e));
                return false;
            }
        }
    }

    if app.mpv_loaded_path.as_deref() != Some(path) {
        // Diagnostic: can the Rust app read the file at all?
        match std::fs::File::open(path) {
            Ok(_) => {
                app.explorer_status = Some(format!("Diagnostic: Rust successfully read file '{}'", path.file_name().unwrap_or_default().to_string_lossy()));
            }
            Err(err) => {
                app.explorer_status = Some(format!("Permission/Read Error (Rust): Cannot open file '{}': {}", path.file_name().unwrap_or_default().to_string_lossy(), err));
                return false;
            }
        }

        if let Some(p) = app.mpv.as_ref() {
            if let Err(e) = p.load(&path.to_string_lossy()) {
                app.explorer_status = Some(format!("MPV Load Error: Failed to play '{}': {}", path.file_name().unwrap_or_default().to_string_lossy(), e));
                return false;
            }
        }
        app.mpv_loaded_path = Some(path.to_path_buf());
    }
    true
}

#[cfg(feature = "mpv")]
fn mpv_media(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    egui::Frame::none()
        .fill(Color32::from_rgb(8, 8, 10))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(0.0))
        .show(ui, |ui| {
            let avail = ui.available_size();
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(avail.x.max(120.0), avail.y.max(120.0)),
                egui::Sense::hover(),
            );
            let ppp = ctx.pixels_per_point();
            let screen_h = ctx.screen_rect().height() * ppp;
            let vw = (rect.width() * ppp).max(2.0) as i32;
            let vh = (rect.height() * ppp).max(2.0) as i32;
            let x0 = (rect.left() * ppp) as i32;
            let x1 = (rect.right() * ppp) as i32;
            let y_top = (screen_h - rect.top() * ppp) as i32;
            let y_bottom = (screen_h - rect.bottom() * ppp) as i32;
            let dst = [x0, y_top, x1, y_bottom];

            let ptr = match app.mpv.as_mut() {
                Some(p) => PlayerPtr(p as *mut _),
                None => return,
            };
            let ectx = ctx.clone();
            let cb = eframe::egui_glow::CallbackFn::new(move |_info, painter| {
                let gl = painter.gl();
                let p = unsafe { &mut *ptr.get() };
                if p.ensure_render(&ectx) {
                    p.paint(gl, vw, vh, dst);
                }
            });
            ui.painter().add(egui::PaintCallback {
                rect,
                callback: std::sync::Arc::new(cb),
            });
            ctx.request_repaint();
        });
}

#[cfg(feature = "mpv")]
fn mpv_transport(
    app: &mut SmartFileManagerApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    can_prev: bool,
    slides: &[(PathBuf, bool)],
    cur: Option<usize>,
    can_next: bool,
    path: &PathBuf,
) -> Option<PathBuf> {
    // 1. Get properties in a short-lived borrow block
    let (dur, pos, paused, vol, is_muted) = {
        let Some(p) = app.mpv.as_ref() else { return None };
        (p.duration(), p.position(), p.paused(), p.volume() as f32, p.muted())
    };
    
    let mut goto: Option<PathBuf> = None;
    let mut seek_to: Option<f64> = None;
    let mut toggle_pause = false;
    let mut set_vol: Option<f32> = None;
    let mut toggle_mute = false;

    egui::Frame::none()
        .inner_margin(egui::Margin::same(0.0)) // 0 margin for edge-to-edge timeline!
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // Thin seek bar above the single control row.
                if dur > 0.0 {
                    let mut v = pos as f32;
                    let slider_w = ui.available_width();
                    
                    ui.scope(|ui| {
                        ui.spacing_mut().slider_width = slider_w;
                        let s = ui.add_sized(
                            [slider_w, 6.0],
                            egui::Slider::new(&mut v, 0.0..=dur as f32)
                                .show_value(false),
                        );
                        if s.drag_stopped() || s.lost_focus() {
                            seek_to = Some(v as f64);
                        }
                    });
                }
                
                // One row: transport + queue + volume (no stacked footer).
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(12.0, 6.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let prev_btn = egui::Button::new(RichText::new("⏮").size(15.0).strong())
                                .fill(Color32::from_rgb(28, 30, 40))
                                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(50, 52, 68)))
                                .min_size(egui::vec2(32.0, 26.0));
                            if ui.add_enabled(can_prev, prev_btn).clicked() {
                                goto = Some(slides[cur.unwrap() - 1].0.clone());
                            }

                            let icon = if paused { "▶" } else { "⏸" };
                            if ui
                                .add(egui::Button::new(RichText::new(icon).size(16.0)).min_size(egui::vec2(36.0, 26.0)))
                                .on_hover_text("Play / Pause (Space)")
                                .clicked()
                            {
                                toggle_pause = true;
                            }
                            if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                                toggle_pause = true;
                            }

                            let next_btn = egui::Button::new(RichText::new("⏭").size(15.0).strong())
                                .fill(Color32::from_rgb(28, 30, 40))
                                .stroke(egui::Stroke::new(1.0, Color32::from_rgb(50, 52, 68)))
                                .min_size(egui::vec2(32.0, 26.0));
                            if ui.add_enabled(can_next, next_btn).clicked() {
                                goto = Some(slides[cur.unwrap() + 1].0.clone());
                            }
                            
                            ui.add_space(6.0);
                            
                            if dur > 0.0 {
                                ui.label(
                                    RichText::new(format!("{}  /  {}", fmt_clock(pos), fmt_clock(dur)))
                                        .color(Color32::from_rgb(180, 185, 200))
                                        .size(13.0)
                                );
                            } else {
                                ui.label(muted("live"));
                            }

                            ui.add_space(10.0);
                            preview_queue_controls(app, ui, path, false);
                            
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let vol_w = 60.0;
                                let vol_display = if is_muted { 0 } else { vol as i32 };
                                ui.label(RichText::new(format!("{}%", vol_display)).size(12.0).color(Color32::from_rgb(170, 175, 190)));
                                
                                let mut slider_vol = if is_muted { 0.0 } else { vol };
                                if ui
                                    .add_sized(
                                        [vol_w, 20.0],
                                        egui::Slider::new(&mut slider_vol, 0.0..=130.0)
                                            .show_value(false),
                                    )
                                    .changed()
                                {
                                    set_vol = Some(slider_vol);
                                }
                                
                                let vol_icon = if is_muted || vol == 0.0 {
                                    "🔇"
                                } else if vol < 40.0 {
                                    "🔈"
                                } else if vol < 80.0 {
                                    "🔉"
                                } else {
                                    "🔊"
                                };
                                
                                let speaker_btn = egui::Button::new(RichText::new(vol_icon).size(13.0))
                                    .frame(false);
                                if ui.add(speaker_btn).clicked() {
                                    toggle_mute = true;
                                }
                            });
                        });
                    });
            });
        });

    // 2. Apply side-effects in a mut-borrow block
    if let Some(p) = app.mpv.as_ref() {
        if let Some(secs) = seek_to {
            p.seek_absolute(secs);
        }
        if toggle_pause {
            p.toggle_pause();
        }
        if let Some(v) = set_vol {
            p.set_volume(v as f64);
            if v > 0.0 {
                p.set_muted(false);
            } else {
                p.set_muted(true);
            }
        }
        if toggle_mute {
            let next_mute = !is_muted;
            p.set_muted(next_mute);
            if next_mute {
                p.set_volume(0.0);
            } else if vol == 0.0 {
                p.set_volume(50.0); // default to 50% on unmute
            }
        }
    }
    
    ctx.request_repaint(); // keep position/time live
    goto
}

fn delete_zip_prompt(app: &mut SmartFileManagerApp, ctx: &egui::Context) {
    let Some(zip) = app.explorer_pending_delete_zip.clone() else {
        return;
    };
    let name = zip.file_name().unwrap_or_default().to_string_lossy().to_string();
    egui::Window::new("Archive extracted")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label(format!("“{name}” was extracted."));
            ui.add_space(4.0);
            ui.label(muted("Move the original archive to the Trash?"));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if primary_button(ui, "Move to Trash").clicked() {
                    match trash_single_path(&zip) {
                        Ok(()) => app.explorer_status = Some(format!("Moved “{name}” to Trash.")),
                        Err(e) => app.explorer_status = Some(format!("Trash failed: {e}")),
                    }
                    app.explorer_pending_delete_zip = None;
                    app.explorer_reload();
                }
                if secondary_button(ui, "Keep archive").clicked() {
                    app.explorer_pending_delete_zip = None;
                }
            });
        });
}

// ---------------------------------------------------------------------------
// Action application + helpers
// ---------------------------------------------------------------------------

fn apply_action(app: &mut SmartFileManagerApp, act: Option<Act>) {
    let Some(act) = act else { return };
    match act {
        Act::Nav(p) => app.explorer_navigate(p),
        Act::Preview(p) => app.explorer_open_preview(p),
        Act::OpenDefault(p) => open_in_default_app(&p),
        Act::Reveal(p) => reveal_in_folder(&p),
        Act::Extract(p) => app.explorer_extract_zip(&p),
        Act::NewFolder => app.explorer_new_folder(),
        Act::Refresh => app.explorer_reload(),
        Act::Sort(k) => app.explorer_set_sort(k),
        Act::Trash(p) => {
            let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            match trash_single_path(&p) {
                Ok(()) => {
                    app.explorer_status = Some(format!("Moved “{name}” to Trash."));
                    app.explorer_selected = None;
                    app.explorer_reload();
                }
                Err(e) => app.explorer_status = Some(format!("Trash failed: {e}")),
            }
        }
        Act::NewQueue => app.explorer_new_queue(),
        Act::OpenQueue(i) => {
            app.explorer_active_queue = Some(i);
            app.explorer_queue_view_open = true;
        }
        Act::RemoveFromQueue(i, p) => app.explorer_remove_from_queue(i, p),
        Act::SubmitAllQueues => app.submit_all_queues(),
        Act::DeleteQueue(i) => app.explorer_delete_queue(i),
        Act::ChangeQueueTarget(i) => app.explorer_change_queue_target(i),
    }
}

fn banner(ui: &mut egui::Ui, bg: Color32, fg: Color32, text: &str) {
    egui::Frame::none()
        .fill(bg)
        .rounding(egui::Rounding::same(7.0))
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            ui.colored_label(fg, text);
        });
}

fn is_zip(e: &ExplorerEntry) -> bool {
    e.path
        .extension()
        .and_then(|x| x.to_str())
        .map(|s| s.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

fn category(e: &ExplorerEntry) -> Cat {
    if e.is_dir {
        return Cat::Folder;
    }
    match e
        .path
        .extension()
        .and_then(|x| x.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tiff" | "heic") => Cat::Image,
        Some("mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "wmv") => Cat::Video,
        Some("mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a") => Cat::Audio,
        Some("pdf" | "doc" | "docx" | "txt" | "md" | "rtf" | "odt" | "pages" | "key" | "csv") => {
            Cat::Document
        }
        Some("zip" | "rar" | "7z" | "tar" | "gz" | "bz2") => Cat::Archive,
        _ => Cat::Other,
    }
}

fn matches_type_filter(e: &ExplorerEntry, filter: ExplorerTypeFilter) -> bool {
    match filter {
        ExplorerTypeFilter::All => true,
        ExplorerTypeFilter::Folders => e.is_dir,
        ExplorerTypeFilter::Image => category(e) == Cat::Image,
        ExplorerTypeFilter::Video => category(e) == Cat::Video,
        ExplorerTypeFilter::Archive => category(e) == Cat::Archive,
    }
}



fn entry_kind(e: &ExplorerEntry) -> String {
    if e.is_dir {
        return "Folder".to_string();
    }
    match e.path.extension().and_then(|x| x.to_str()) {
        Some(ext) if !ext.is_empty() => format!("{} file", ext.to_uppercase()),
        _ => "File".to_string(),
    }
}

fn fmt_time(t: Option<SystemTime>) -> String {
    match t {
        Some(t) => {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%Y-%m-%d %H:%M").to_string()
        }
        None => "—".to_string(),
    }
}

/// Renders modern, high-fidelity macOS-inspired sky blue vector icons
fn draw_sky_blue_icon(ui: &mut egui::Ui, size: egui::Vec2, cat: Cat) {
    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    
    // Vibrant macOS-like Sky Blue palette
    let folder_color = Color32::from_rgb(90, 170, 245);       // Sky blue
    let folder_tab_color = Color32::from_rgb(130, 200, 255);  // Lighter sky blue
    let zip_color = Color32::from_rgb(80, 150, 220);          // Deep sky blue
    let file_color = Color32::from_rgb(230, 240, 255);        // Ice white/light blue
    
    match cat {
        Cat::Folder => {
            let r = rect;
            // Back tab
            let tab_w = r.width() * 0.45;
            let tab_h = r.height() * 0.25;
            let tab_rect = egui::Rect::from_min_size(r.min, egui::vec2(tab_w, tab_h));
            painter.rect_filled(tab_rect, egui::Rounding::same(r.width() * 0.1), folder_tab_color);
            
            // Main body
            let body_y = r.min.y + r.height() * 0.15;
            let body_rect = egui::Rect::from_min_max(
                egui::pos2(r.min.x, body_y),
                r.max,
            );
            painter.rect_filled(body_rect, egui::Rounding::same(r.width() * 0.12), folder_color);
        }
        Cat::Archive => {
            // Draw a gorgeous sky-blue zip file with a zipper
            let r = rect;
            
            // Draw file background sheet
            let body_rect = egui::Rect::from_min_max(r.min, r.max);
            painter.rect_filled(body_rect, egui::Rounding::same(r.width() * 0.1), file_color);
            painter.rect_stroke(body_rect, egui::Rounding::same(r.width() * 0.1), egui::Stroke::new(r.width() * 0.06, zip_color));
            
            // Top-right dog-ear fold lines
            let fold = r.width() * 0.28;
            painter.line_segment(
                [egui::pos2(r.max.x - fold, r.min.y), egui::pos2(r.max.x - fold, r.min.y + fold)],
                egui::Stroke::new(r.width() * 0.06, zip_color),
            );
            painter.line_segment(
                [egui::pos2(r.max.x - fold, r.min.y + fold), egui::pos2(r.max.x, r.min.y + fold)],
                egui::Stroke::new(r.width() * 0.06, zip_color),
            );
            
            // Draw a vertical zipper band in the middle
            let zip_w = r.width() * 0.20;
            let zip_x = r.center().x - zip_w * 0.5;
            let zip_rect = egui::Rect::from_min_max(
                egui::pos2(zip_x, r.min.y + r.height() * 0.2),
                egui::pos2(zip_x + zip_w, r.max.y - r.height() * 0.1),
            );
            painter.rect_filled(zip_rect, egui::Rounding::same(r.width() * 0.04), Color32::from_rgb(60, 80, 100));
            
            // Zipper teeth (horizontal lines)
            let steps = 6;
            for i in 0..steps {
                let y = zip_rect.min.y + (zip_rect.height() / (steps - 1) as f32) * i as f32;
                painter.line_segment(
                    [egui::pos2(zip_rect.min.x, y), egui::pos2(zip_rect.max.x, y)],
                    egui::Stroke::new(r.width() * 0.04, zip_color),
                );
            }
        }
        _ => {
            // Draw a beautiful macOS-style document sheet with a dog-ear fold!
            let r = rect;
            
            // Draw the sheet body
            let body_rect = egui::Rect::from_min_max(r.min, r.max);
            painter.rect_filled(body_rect, egui::Rounding::same(r.width() * 0.1), file_color);
            painter.rect_stroke(body_rect, egui::Rounding::same(r.width() * 0.1), egui::Stroke::new(r.width() * 0.06, folder_color));
            
            // Top-right dog-ear fold lines
            let fold = r.width() * 0.28;
            painter.line_segment(
                [egui::pos2(r.max.x - fold, r.min.y), egui::pos2(r.max.x - fold, r.min.y + fold)],
                egui::Stroke::new(r.width() * 0.06, folder_color),
            );
            painter.line_segment(
                [egui::pos2(r.max.x - fold, r.min.y + fold), egui::pos2(r.max.x, r.min.y + fold)],
                egui::Stroke::new(r.width() * 0.06, folder_color),
            );
            
            // Minimal text lines inside
            let line_y0 = r.min.y + r.height() * 0.45;
            let line_y1 = r.min.y + r.height() * 0.65;
            let line_left = r.min.x + r.width() * 0.2;
            let line_right = r.max.x - r.width() * 0.2;
            painter.line_segment(
                [egui::pos2(line_left, line_y0), egui::pos2(line_right, line_y0)],
                egui::Stroke::new(r.width() * 0.06, folder_color),
            );
            painter.line_segment(
                [egui::pos2(line_left, line_y1), egui::pos2(line_left + (line_right - line_left) * 0.8, line_y1)],
                egui::Stroke::new(r.width() * 0.06, folder_color),
            );
        }
    }
}

fn draw_preview_or_icon(
    app: &mut SmartFileManagerApp,
    ui: &mut egui::Ui,
    size: egui::Vec2,
    e: &ExplorerEntry,
) {
    let cat = category(e);
    if cat == Cat::Image || cat == Cat::Video {
        if let Some(tex) = app.thumbnail_cache.get(&e.path) {
            let rounding = if size.x > 50.0 {
                egui::Rounding {
                    nw: 8.0,
                    ne: 8.0,
                    sw: 0.0,
                    se: 0.0,
                }
            } else {
                egui::Rounding::same(size.x * 0.15)
            };
            let image_widget = egui::Image::from_texture(SizedTexture::from_handle(tex))
                .fit_to_exact_size(size)
                .rounding(rounding);
            
            if size.x > 50.0 {
                ui.centered_and_justified(|ui| {
                    ui.add(image_widget);
                });
            } else {
                ui.add(image_widget);
            }
            return;
        }

        if !app.thumbnail_loading.contains(&e.path) {
            app.thumbnail_loading.insert(e.path.clone());
            let path = e.path.clone();
            let tx = app.thumbnail_tx.clone();
            std::thread::spawn(move || {
                let res = if cat == Cat::Image {
                    load_image_thumb(&path)
                } else {
                    load_video_thumb(&path)
                };
                if let Ok(img) = res {
                    let _ = tx.send((path, img));
                }
            });
        }
    }

    // For standard icons or while loading, center a square icon inside the large grid viewport
    if size.x > 50.0 {
        let icon_sz = 40.0;
        ui.centered_and_justified(|ui| {
            draw_sky_blue_icon(ui, egui::vec2(icon_sz, icon_sz), cat);
        });
    } else {
        draw_sky_blue_icon(ui, size, cat);
    }
}

pub fn draw_thumb_for_path(
    app: &mut SmartFileManagerApp,
    ui: &mut egui::Ui,
    size: egui::Vec2,
    path: &std::path::Path,
) {
    let cat = if crate::ui::preview::is_image_path(path) {
        Cat::Image
    } else if crate::ui::preview::is_video_path(path) {
        Cat::Video
    } else {
        Cat::Other
    };

    if cat == Cat::Image || cat == Cat::Video {
        if let Some(tex) = app.thumbnail_cache.get(path) {
            let rounding = if size.x > 50.0 {
                egui::Rounding {
                    nw: 8.0,
                    ne: 8.0,
                    sw: 8.0,
                    se: 8.0,
                }
            } else {
                egui::Rounding::same(size.x * 0.15)
            };
            let tw = tex.size()[0] as f32;
            let th = tex.size()[1] as f32;
            let scale = (size.x / tw).min(size.y / th);
            let fitted_size = egui::vec2(tw * scale, th * scale);

            let image_widget = egui::Image::from_texture(SizedTexture::from_handle(tex))
                .fit_to_exact_size(fitted_size)
                .rounding(rounding);
            
            if size.x > 50.0 {
                ui.vertical_centered(|ui| {
                    ui.add(image_widget);
                });
            } else {
                ui.add(image_widget);
            }
            return;
        }

        if !app.thumbnail_loading.contains(path) {
            app.thumbnail_loading.insert(path.to_path_buf());
            let p = path.to_path_buf();
            let tx = app.thumbnail_tx.clone();
            std::thread::spawn(move || {
                let res = if cat == Cat::Image {
                    load_image_thumb(&p)
                } else {
                    load_video_thumb(&p)
                };
                if let Ok(img) = res {
                    let _ = tx.send((p, img));
                }
            });
        }
    }

    if size.x > 50.0 {
        let icon_sz = 40.0;
        ui.centered_and_justified(|ui| {
            draw_sky_blue_icon(ui, egui::vec2(icon_sz, icon_sz), cat);
        });
    } else {
        draw_sky_blue_icon(ui, size, cat);
    }
}


fn load_image_thumb(
    path: &std::path::Path,
) -> Result<eframe::egui::ColorImage, Box<dyn std::error::Error + Send + Sync>> {
    let buf = std::fs::read(path)?;
    let img = image::load_from_memory(&buf)?;
    let img = img.thumbnail(256, 256);
    let rgba = img.to_rgba8();
    let w = rgba.width() as usize;
    let h = rgba.height() as usize;
    Ok(eframe::egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw()))
}

fn load_video_thumb(
    path: &std::path::Path,
) -> Result<eframe::egui::ColorImage, Box<dyn std::error::Error + Send + Sync>> {
    let ffmpeg = resolve_bin("ffmpeg").ok_or("ffmpeg not found")?;
    let path_str = path.to_string_lossy().to_string();
    let output = std::process::Command::new(&ffmpeg)
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-ss")
        .arg("0.5")
        .arg("-i")
        .arg(&path_str)
        .args(["-vf", "scale=256:-2", "-vframes", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()?;

    if !output.status.success() || output.stdout.is_empty() {
        return Err("ffmpeg failed".into());
    }

    let w = 256;
    let h = output.stdout.len() / (w * 3);
    if h == 0 {
        return Err("invalid dimensions".into());
    }

    Ok(eframe::egui::ColorImage::from_rgb([w, h], &output.stdout[..w * h * 3]))
}

fn resolve_bin(name: &str) -> Option<String> {
    let on_path = std::process::Command::new(name)
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if on_path {
        return Some(name.to_string());
    }
    for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin", "/usr/bin"] {
        let p = format!("{dir}/{name}");
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    None
}
