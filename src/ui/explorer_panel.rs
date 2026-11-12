//! Windows-Explorer-style file browser: places sidebar, breadcrumb, sortable +
//! groupable file table, left-click preview modal, and right-click actions
//! (open, reveal, extract archives, move to Trash, new folder).
use crate::app::{
    ExplorerEntry, ExplorerSortKey, ExplorerTypeFilter, ExplorerViewMode, PendingAction,
    SortwiseApp,
};
use crate::ui::preview::{format_bytes, open_in_default_app, reveal_in_folder};
use crate::ui::style::{danger_button, muted, primary_button, secondary_button};
use eframe::egui::{
    self, load::SizedTexture, scroll_area::ScrollBarVisibility, Color32, RichText, ScrollArea,
    TextureOptions,
};
use sortwise::resolver::trash_single_path;
use std::path::PathBuf;
use std::time::SystemTime;

/// Collected from clicks / menus and applied after rendering to avoid borrow clashes.
use crate::ui::icons::Icons;

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
    /// Ctrl/Cmd-click (toggle) or Shift-click (range) on an entry.
    SelectClick { idx: usize, range: bool },
    ClearSelection,
    StageSelected,
    TrashSelected,
    Stage(PathBuf),
    // Queue (staged move) actions.
    NewQueue,
    OpenQueue(usize),
    RemoveFromQueue(usize, PathBuf),
    SubmitAllQueues,
    DeleteQueue(usize),
    ChangeQueueTarget(usize),
    // Movable Queue actions
    MoveSingleMovable(usize),
    MoveAllMovable,
    RemoveMovable(usize),
    ClearMovable,
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

pub fn explorer_sidebar(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    // Narrow sidebar (icons only): names move to tooltips.
    let collapsed = ui.available_width() < 120.0;
    if !collapsed {
        ui.label(muted("PLACES"));
        ui.add_space(6.0);
    }

    let mut nav: Option<PathBuf> = None;
    for (icon, label, path) in places() {
        let selected = app.explorer_dir == path;
        if place_row(ui, icon, &label, selected, collapsed).on_hover_text(&label).clicked() {
            nav = Some(path);
        }
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(6.0);
    let open_clicked = if collapsed {
        ui.add(
            egui::Button::new(RichText::new(Icons::FOLDER_OPEN).size(15.0).strong())
                .min_size(egui::vec2(36.0, 28.0)),
        )
        .on_hover_text("Open folder…")
        .clicked()
    } else {
        secondary_button(ui, "Open folder…").clicked()
    };
    if open_clicked {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            nav = Some(dir);
        }
    }

    if let Some(p) = nav {
        app.explorer_navigate(p);
    }
}

fn place_row(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool, collapsed: bool) -> egui::Response {
    let bg = if selected {
        crate::ui::style::pal().accent_soft
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
                if !collapsed {
                    ui.add_space(4.0);
                    ui.label(RichText::new(label).color(crate::ui::style::pal().text));
                    ui.allocate_space(egui::vec2(ui.available_width(), 0.0));
                }
            });
        })
        .response
        .interact(egui::Sense::click())
}

fn places() -> Vec<(&'static str, String, PathBuf)> {
    static CACHE: std::sync::Mutex<Option<(std::time::Instant, Vec<(&'static str, String, PathBuf)>)>> =
        std::sync::Mutex::new(None);
    if let Ok(guard) = CACHE.lock() {
        if let Some((time, ref list)) = *guard {
            if time.elapsed() < std::time::Duration::from_secs(4) {
                return list.clone();
            }
        }
    }

    let mut out: Vec<(&'static str, String, PathBuf)> = Vec::new();
    let mut add = |icon, label: &str, p: Option<PathBuf>| {
        if let Some(p) = p {
            if p.exists() {
                out.push((icon, label.to_string(), p));
            }
        }
    };
    add(Icons::HOME, "Home", dirs::home_dir());
    add(Icons::DESKTOP, "Desktop", dirs::desktop_dir());
    add(Icons::DOCUMENT, "Documents", dirs::document_dir());
    add(Icons::DOWNLOAD, "Downloads", dirs::download_dir());
    add(Icons::PICTURE, "Pictures", dirs::picture_dir());
    add(Icons::VIDEO, "Movies", dirs::video_dir());
    add(Icons::AUDIO, "Music", dirs::audio_dir());

    #[cfg(target_os = "macos")]
    {
        if let Ok(rd) = std::fs::read_dir("/Volumes") {
            for e in rd.flatten() {
                out.push(("\u{E29E}", e.file_name().to_string_lossy().to_string(), e.path()));
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        out.push(("\u{E29E}", "Root".to_string(), PathBuf::from("/")));
    }

    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((std::time::Instant::now(), out.clone()));
    }
    out
}

// ---------------------------------------------------------------------------
// Main panel
// ---------------------------------------------------------------------------

pub fn explorer_main_panel(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    // Guarantee the listing matches the current directory regardless of when
    // navigation happened this frame.
    app.explorer_ensure_loaded();

    let mut act: Option<Act> = None;

    toolbar(app, ui, &mut act);
    ui.add_space(6.0);

    if let Some(err) = app.explorer_error.clone() {
        banner(ui, crate::ui::style::pal().danger_soft, crate::ui::style::pal().danger_on_soft, &format!("\u{E4E0}  {err}"));
        ui.add_space(4.0);
    }
    if let Some(msg) = app.explorer_status.clone() {
        banner(ui, crate::ui::style::pal().surface_raised, crate::ui::style::pal().success_on_soft, &msg);
        ui.add_space(4.0);
    }
    extract_banner_ui(app, ui);

    // Footer-first: reserve the bottom strips, then give the table the remaining height.
    let selection_h = if selected_entries(app).is_empty() { 0.0 } else { 48.0 };
    // 40 px: column header row plus frame padding, so the table never pushes the strips off-screen.
    let footer_h = 30.0 + selection_h + 40.0;
    let table_h = (ui.available_height() - footer_h).max(160.0);
    file_table(app, ui, &mut act, table_h);

    selection_bar(app, ui, &mut act);

    apply_action(app, act);
}

/// Select All + queue picker for collapsed recursive-search results.
fn search_selection_bar(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    egui::Frame::none()
        .fill(crate::ui::style::pal().surface_raised)
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
                        if secondary_button(ui, "+ New Queue").clicked() {
                            app.explorer_new_queue();
                        }
                    } else {
                        let mut pick = app
                            .explorer_preview_queue_pick
                            .filter(|&i| i < queue_names.len());
                        searchable_queue_combo(
                            ui,
                            "search_queue_pick",
                            220.0,
                            &mut pick,
                            &queue_names,
                            "Choose queue…",
                        );
                        app.explorer_preview_queue_pick = pick;

                        let add_enabled = pick.is_some() && !app.explorer_search_checked.is_empty();
                        if ui
                            .add_enabled(
                                add_enabled,
                                egui::Button::new(
                                    RichText::new("\u{E3D4}  Add to Queue")
                                        .strong()
                                        .color(crate::ui::style::pal().on_accent),
                                )
                                .fill(crate::ui::style::pal().accent),
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

/// Height of the toolbar row.
const TOOLBAR_H: f32 = 40.0;
/// Width of the view toggle, sort and filter menu and new-folder button, with their gaps.
const TOOLBAR_FIXED_RIGHT_W: f32 = 190.0;

fn icon_nav(ui: &mut egui::Ui, enabled: bool, icon: &str, tip: &str) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(icon).size(15.0).strong()).min_size(egui::vec2(28.0, 26.0)),
    )
    .on_hover_text(tip)
    .clicked()
}

/// One row: navigation, breadcrumb (fills the middle), then search, view, sort and filter, new folder.
fn toolbar(app: &mut SortwiseApp, ui: &mut egui::Ui, act: &mut Option<Act>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        // Icon buttons: the app-wide 16 px padding would make each one ~60 px wide.
        ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
        let can_back = !app.explorer_back.is_empty();
        let can_fwd = !app.explorer_fwd.is_empty();
        let can_up = app.explorer_dir.parent().is_some();
        if icon_nav(ui, can_back, Icons::NAV_BACK, "Back") {
            app.explorer_go_back();
        }
        if icon_nav(ui, can_fwd, Icons::NAV_FWD, "Forward") {
            app.explorer_go_forward();
        }
        if icon_nav(ui, can_up, Icons::NAV_UP, "Up one level") {
            app.explorer_go_up();
        }
        if icon_nav(ui, true, Icons::REFRESH, "Refresh folder") {
            app.explorer_reload();
        }

        // Search shrinks with the window; the breadcrumb keeps the rest.
        let search_w = (ui.available_width() * 0.22).clamp(110.0, 200.0);
        let crumb_w = (ui.available_width() - TOOLBAR_FIXED_RIGHT_W - search_w).max(120.0);
        ui.allocate_ui_with_layout(
            egui::vec2(crumb_w, TOOLBAR_H),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| breadcrumb_bar(app, ui, act),
        );

        // Right-to-left: the first control added sits at the far right.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new(Icons::NEW_FOLDER).size(15.0).strong().color(crate::ui::style::pal().text))
                        .fill(crate::ui::style::pal().surface_raised)
                        .min_size(egui::vec2(30.0, 26.0)),
                )
                .on_hover_text("New folder")
                .clicked()
            {
                *act = Some(Act::NewFolder);
            }
            sort_filter_menu(app, ui);
            view_toggle(app, ui);
            ui.add(
                egui::TextEdit::singleline(&mut app.explorer_filter)
                    .hint_text("Search (recursive)…")
                    .desired_width(search_w),
            );
        });
    });

    // Recursive-search status: shown on its own line only while a search is running or has results.
    if app.explorer_searching || app.explorer_search_pending || app.explorer_is_recursive_search {
        ui.horizontal(|ui| {
            if app.explorer_searching {
                ui.spinner();
                ui.label(muted("Searching…"));
            } else if app.explorer_search_pending {
                ui.label(
                    RichText::new(format!("{} Waiting…", Icons::SEARCH))
                        .color(crate::ui::style::pal().warning)
                        .size(11.5),
                );
            } else {
                ui.label(
                    RichText::new(format!("{} Recursive results", Icons::SEARCH))
                        .color(crate::ui::style::pal().accent_on_soft)
                        .size(11.5),
                );
            }
        });
    }

    if app.explorer_is_recursive_search && !app.explorer_searching {
        ui.add_space(4.0);
        search_selection_bar(app, ui);
    }
}

/// Sort field and direction, file-type filter and grouping, in one menu.
fn sort_filter_menu(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    ui.menu_button(RichText::new(Icons::SORT_FILTER).size(15.0).strong(), |ui| {
        ui.label(muted("Sort by"));
        let mut key = app.explorer_sort_key;
        let mut asc = app.explorer_sort_asc;
        let mut sort_changed = false;
        for (value, label) in [
            (ExplorerSortKey::Name, "Name"),
            (ExplorerSortKey::Size, "Size"),
            (ExplorerSortKey::Kind, "Kind"),
            (ExplorerSortKey::Modified, "Modified"),
        ] {
            if ui.radio_value(&mut key, value, label).changed() {
                sort_changed = true;
            }
        }
        if ui.checkbox(&mut asc, "Ascending").changed() {
            sort_changed = true;
        }
        if sort_changed {
            app.explorer_sort_key = key;
            app.explorer_sort_asc = asc;
            app.explorer_sort_entries();
        }

        ui.separator();
        ui.label(muted("Show"));
        let mut type_filter = app.explorer_type_filter;
        for (value, label) in [
            (ExplorerTypeFilter::All, "All"),
            (ExplorerTypeFilter::Folders, "Folders"),
            (ExplorerTypeFilter::Image, "Images"),
            (ExplorerTypeFilter::Video, "Videos"),
            (ExplorerTypeFilter::Archive, "Zips"),
        ] {
            ui.radio_value(&mut type_filter, value, label);
        }
        app.explorer_type_filter = type_filter;

        ui.separator();
        ui.checkbox(&mut app.explorer_group_by_kind, "Group by type");
    })
    .response
    .on_hover_text("Sort, filter and grouping");
}

fn view_toggle(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    let seg = |ui: &mut egui::Ui, icon: &str, active: bool, tip: &str| -> bool {
        let (fg, bg) = if active {
            (crate::ui::style::pal().on_accent, crate::ui::style::pal().accent)
        } else {
            (crate::ui::style::pal().text, crate::ui::style::pal().surface_raised)
        };
        ui.add(
            egui::Button::new(RichText::new(icon).size(15.0).strong().color(fg))
                .fill(bg)
                .min_size(egui::vec2(28.0, 24.0)),
        )
        .on_hover_text(tip)
        .clicked()
    };
    ui.spacing_mut().item_spacing.x = 4.0;
    if seg(ui, Icons::VIEW_LIST, app.explorer_view_mode == ExplorerViewMode::List, "List view") {
        app.explorer_view_mode = ExplorerViewMode::List;
    }
    if seg(ui, Icons::VIEW_GRID, app.explorer_view_mode == ExplorerViewMode::Grid, "Grid view") {
        app.explorer_view_mode = ExplorerViewMode::Grid;
    }
    if seg(ui, Icons::VIEW_FOLDERS, app.explorer_view_mode == ExplorerViewMode::Folders, "Folders view")
        && app.explorer_view_mode != ExplorerViewMode::Folders {
            app.explorer_view_mode = ExplorerViewMode::Folders;
            app.explorer_reload();
        }
    if seg(ui, Icons::DOCUMENT, app.explorer_view_mode == ExplorerViewMode::Files, "All files under this folder (recursive, largest first)") {
        app.explorer_view_mode = ExplorerViewMode::Files;
    }
}

fn breadcrumb_bar(app: &SortwiseApp, ui: &mut egui::Ui, act: &mut Option<Act>) {
    egui::Frame::none()
        .fill(crate::ui::style::pal().surface_raised)
        .rounding(egui::Rounding::same(7.0))
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            // Thin horizontal scrollbar so long paths don't dominate the bar height.
            ui.spacing_mut().scroll.bar_width = 4.0;
            ui.spacing_mut().scroll.handle_min_length = 24.0;
            ui.set_min_height(26.0);
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
                                        .color(crate::ui::style::pal().border_strong),
                                );
                            }
                            let name = c
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "/".to_string());
                            let last = i + 1 == crumbs.len();
                            let color = if last {
                                crate::ui::style::pal().text
                            } else {
                                crate::ui::style::pal().accent_on_soft
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

/// Entry indices that pass the name and type filters, in display order.
fn visible_indices(app: &SortwiseApp) -> Vec<usize> {
    let filter = app.explorer_filter.trim().to_lowercase();
    let type_filter = app.explorer_type_filter;
    // When in recursive-search mode, explorer_entries already holds globally-filtered
    // results — skip the second in-frame name filter to prevent double-filtering.
    app.explorer_entries
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
        .collect()
}

fn file_table(app: &mut SortwiseApp, ui: &mut egui::Ui, act: &mut Option<Act>, table_h: f32) {
    if app.explorer_view_mode == ExplorerViewMode::Folders {
        crate::ui::render_folder_grid(app, ui, table_h);
        return;
    }
    if app.explorer_view_mode == ExplorerViewMode::Files {
        crate::ui::recursive_files_panel::recursive_files_view(app, ui, table_h);
        return;
    }
    let visible: Vec<usize> = visible_indices(app);

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
        .fill(crate::ui::style::pal().surface)
        .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(padding)
        .show(ui, |ui| {
            // Column header (List view only — Grid sorts via the toolbar control).
            if is_list {
                egui::Frame::none()
                    .fill(crate::ui::style::pal().surface_raised)
                    .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let tw = table_widths(ui.available_width());
                            if header(ui, &format!("Name{}", arrow(ExplorerSortKey::Name)), tw[0]) {
                                *act = Some(Act::Sort(ExplorerSortKey::Name));
                            }
                            if header(ui, &format!("Size{}", arrow(ExplorerSortKey::Size)), tw[1]) {
                                *act = Some(Act::Sort(ExplorerSortKey::Size));
                            }
                            if tw[2] > 0.0
                                && header(ui, &format!("Kind{}", arrow(ExplorerSortKey::Kind)), tw[2])
                            {
                                *act = Some(Act::Sort(ExplorerSortKey::Kind));
                            }
                            if tw[3] > 0.0
                                && header(
                                    ui,
                                    &format!("Modified{}", arrow(ExplorerSortKey::Modified)),
                                    tw[3],
                                )
                            {
                                *act = Some(Act::Sort(ExplorerSortKey::Modified));
                            }
                        });
                    });
            }

            if visible.is_empty() {
                ScrollArea::vertical()
                    .max_height(table_h)
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                    .id_salt("explorer_files")
                    .show(ui, |ui| {
                        empty_state(app, ui);
                    });
            } else if is_list {
                const LIST_ROW_H: f32 = 30.0;
                if app.explorer_group_by_kind {
                    enum DisplayRow {
                        Header(&'static str, usize),
                        Item(usize),
                    }
                    let mut rows = Vec::new();
                    for (cat, label) in CAT_ORDER {
                        let group: Vec<usize> = visible
                            .iter()
                            .copied()
                            .filter(|&i| category(&app.explorer_entries[i]) == cat)
                            .collect();
                        if group.is_empty() {
                            continue;
                        }
                        rows.push(DisplayRow::Header(label, group.len()));
                        for idx in group {
                            rows.push(DisplayRow::Item(idx));
                        }
                    }
                    ScrollArea::vertical()
                        .max_height(table_h)
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                        .id_salt("explorer_files")
                        .show_rows(ui, LIST_ROW_H, rows.len(), |ui, row_range| {
                            for r in row_range {
                                match &rows[r] {
                                    DisplayRow::Header(label, count) => group_header(ui, label, *count),
                                    &DisplayRow::Item(idx) => row_ui(app, ui, idx, act),
                                }
                            }
                        });
                } else {
                    ScrollArea::vertical()
                        .max_height(table_h)
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                        .id_salt("explorer_files")
                        .show_rows(ui, LIST_ROW_H, visible.len(), |ui, row_range| {
                            for r in row_range {
                                row_ui(app, ui, visible[r], act);
                            }
                        });
                }
            } else {
                // Grid View
                // Columns come from the width the grid really has. Tiles then stretch to fill the row,
                // so the right edge lines up at every window width.
                const GAP: f32 = 12.0;
                let avail = ui.available_width() - 24.0;
                let cols = (((avail + GAP) / (TILE_W + GAP)).floor() as usize).clamp(1, 8);
                let tile_w = ((avail - GAP * (cols as f32 - 1.0)) / cols as f32).max(96.0);
                let spacing_x = GAP;
                const GRID_ROW_H: f32 = TILE_H + 12.0;

                if !app.explorer_group_by_kind {
                    let total_grid_rows = visible.len().div_ceil(cols);
                    ScrollArea::vertical()
                        .max_height(table_h)
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                        .id_salt("explorer_files")
                        .show_rows(ui, GRID_ROW_H, total_grid_rows, |ui, row_range| {
                            for r in row_range {
                                let start = r * cols;
                                let end = (start + cols).min(visible.len());
                                let chunk = &visible[start..end];
                                // Fixed-height row: show_rows culls by GRID_ROW_H, so rows must not
                                // render taller/shorter than it or the scroll range jitters near the bottom.
                                ui.allocate_ui(egui::vec2(ui.available_width(), GRID_ROW_H), |ui| {
                                    ui.horizontal_top(|ui| {
                                        ui.spacing_mut().item_spacing.x = spacing_x;
                                        for &idx in chunk {
                                            tile_ui(app, ui, idx, act, tile_w);
                                        }
                                    });
                                });
                            }
                        });
                } else {
                    // Grouped Grid View with viewport culling
                    ScrollArea::vertical()
                        .max_height(table_h)
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                        .id_salt("explorer_files")
                        .show(ui, |ui| {
                            let clip = ui.clip_rect();
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
                                ui.add_space(8.0);
                                for chunk in group.chunks(cols) {
                                    let cursor_y = ui.cursor().top();
                                    if cursor_y + GRID_ROW_H < clip.top() || cursor_y > clip.bottom() {
                                        ui.allocate_space(egui::vec2(ui.available_width(), GRID_ROW_H));
                                    } else {
                                        // Same fixed height as the culled placeholder above, so scrolling never shifts layout.
                                        ui.allocate_ui(egui::vec2(ui.available_width(), GRID_ROW_H), |ui| {
                                            ui.horizontal_top(|ui| {
                                                ui.spacing_mut().item_spacing.x = spacing_x;
                                                for &idx in chunk {
                                                    tile_ui(app, ui, idx, act, tile_w);
                                                }
                                            });
                                        });
                                    }
                                }
                            }
                        });
                }
            }
        });
}

const TILE_W: f32 = 128.0;
const TILE_H: f32 = 150.0;

fn tile_ui(app: &mut SortwiseApp, ui: &mut egui::Ui, idx: usize, act: &mut Option<Act>, tile_w: f32) {
    let e = app.explorer_entries[idx].clone();
    let selected = app.explorer_selected == Some(idx);
    let checked = app.explorer_is_recursive_search && app.explorer_search_checked.contains(&e.path);
    let in_selection = app.explorer_multi.contains(&e.path);
    let staged = app.movable_items.iter().any(|m| m.path == e.path);
    let fill = if selected || checked || in_selection {
        crate::ui::style::pal().accent_soft
    } else {
        crate::ui::style::pal().surface_raised
    };
    let mut checkbox_clicked = false;
    let resp = egui::Frame::none()
        .fill(fill)
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(0.0))
        .show(ui, |ui| {
            ui.set_width(tile_w);
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
                ui.allocate_ui(egui::vec2(tile_w, TILE_H * 0.7), |ui| {
                    draw_preview_or_icon(app, ui, egui::vec2(tile_w, TILE_H * 0.7), &e);
                });

                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .show(ui, |ui| {
                        // The label frame adds 8 px on each side, so its content is 16 px narrower than the tile.
                        ui.set_width(tile_w - 16.0);
                        ui.vertical_centered(|ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(elide(&e.name, 22))
                                        .color(crate::ui::style::pal().text)
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
        item_interactions(&resp, idx, &e, staged, act);
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

fn empty_state(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        let filtering = !app.explorer_filter.trim().is_empty();
        if filtering {
            ui.label(RichText::new("\u{E30C}").size(34.0));
            ui.add_space(6.0);
            ui.label(muted("Nothing matches the current filter."));
            return;
        }
        ui.label(RichText::new("\u{E3DE}").size(34.0));
        ui.add_space(8.0);
        ui.label(
            RichText::new("No items to show")
                .strong()
                .size(15.0)
                .color(crate::ui::style::pal().text),
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
                if let Err(e) = crate::ui::preview::open_privacy_settings() {
                    app.explorer_status = Some(format!("Could not open Privacy Settings: {e}"));
                }
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

/// Column widths for the explorer list, from the width the table actually has. Columns drop out as
/// the table narrows (Kind first, then Modified), so Name always keeps room and nothing is pushed
/// off the edge. Name takes whatever the visible columns leave. A hidden column has width 0.
/// Returns `[name, size, kind, modified]`.
fn table_widths(available: f32) -> [f32; 4] {
    const NAME_MIN: f32 = 160.0;
    const SIZE: f32 = 90.0;
    const KIND: f32 = 110.0;
    const MODIFIED: f32 = 140.0;
    const PAD: f32 = 40.0;
    if available >= NAME_MIN + SIZE + KIND + MODIFIED + PAD {
        [available - SIZE - KIND - MODIFIED - PAD, SIZE, KIND, MODIFIED]
    } else if available >= NAME_MIN + SIZE + KIND + PAD {
        [available - SIZE - KIND - PAD, SIZE, KIND, 0.0]
    } else {
        [(available - SIZE - PAD).max(NAME_MIN), SIZE, 0.0, 0.0]
    }
}

/// A fixed-width table column. `allocate_ui` alone reserves only the space its content uses,
/// which lets Size / Kind / Modified drift left; `set_min_width` pins the column width.
fn column<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(egui::vec2(width, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(width);
        add(ui)
    })
    .inner
}

fn header(ui: &mut egui::Ui, label: &str, width: f32) -> bool {
    let mut clicked = false;
    column(ui, width, |ui| {
        if ui
            .add(
                egui::Label::new(
                    RichText::new(label)
                        .strong()
                        .color(crate::ui::style::pal().text_muted),
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
        .fill(crate::ui::style::pal().surface_raised)
        .inner_margin(egui::Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.allocate_ui(egui::vec2(ui.available_width(), 30.0), |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{label}  ({count})"))
                            .strong()
                            .size(12.0)
                            .color(crate::ui::style::pal().accent_on_soft),
                    );
                });
            });
        });
}

fn row_ui(
    app: &mut SortwiseApp,
    ui: &mut egui::Ui,
    idx: usize,
    act: &mut Option<Act>,
) {
    let e = app.explorer_entries[idx].clone();
    let selected = app.explorer_selected == Some(idx);
    let checked = app.explorer_is_recursive_search && app.explorer_search_checked.contains(&e.path);
    let in_selection = app.explorer_multi.contains(&e.path);
    let staged = app.movable_items.iter().any(|m| m.path == e.path);
    let fill = if selected || checked || in_selection {
        crate::ui::style::pal().accent_soft
    } else {
        crate::ui::style::pal().surface
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
        .inner_margin(egui::Margin::symmetric(10.0, 3.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
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
                let tw = table_widths(ui.available_width());
                column(ui, tw[0], |ui| {
                    ui.horizontal(|ui| {
                        draw_preview_or_icon(app, ui, egui::vec2(16.0, 16.0), &e);
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&e.name).color(crate::ui::style::pal().text));
                            if let Some(ref rel) = rel_path {
                                ui.label(
                                    RichText::new(rel)
                                        .color(crate::ui::style::pal().accent)
                                        .size(10.5),
                                );
                            }
                        });
                    });
                });
                column(ui, tw[1], |ui| {
                    ui.label(muted(if e.is_dir { "—".to_string() } else { format_bytes(e.size) }));
                });
                if tw[2] > 0.0 {
                    column(ui, tw[2], |ui| {
                        ui.label(muted(entry_kind(&e)));
                    });
                }
                if tw[3] > 0.0 {
                    column(ui, tw[3], |ui| {
                        ui.label(muted(fmt_time(e.modified)));
                    });
                }
            });
        })
        .response
        .interact(egui::Sense::click());

    if !checkbox_clicked {
        item_interactions(&resp, idx, &e, staged, act);
    }
}

/// Shared click / double-click / right-click behaviour for list rows and grid tiles.
/// Plain click keeps the existing open or preview. Ctrl/Cmd-click toggles the entry in the
/// selection, and Shift-click selects a range from the last anchor.
fn item_interactions(resp: &egui::Response, idx: usize, e: &ExplorerEntry, staged: bool, act: &mut Option<Act>) {
    let modifiers = resp.ctx.input(|i| i.modifiers);
    if resp.clicked() && (modifiers.command || modifiers.shift) {
        *act = Some(Act::SelectClick { idx, range: modifiers.shift && !modifiers.command });
    } else if resp.clicked() {
        // Left click: folders open, files preview.
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
        if !e.is_dir {
            let label = if staged { "Unstage" } else { "Stage" };
            if ui.button(label).clicked() {
                *act = Some(Act::Stage(e.path.clone()));
                ui.close_menu();
            }
        }
        if ui.button("Copy path").clicked() {
            ui.output_mut(|o| o.copied_text = e.path.display().to_string());
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

/// Selection actions (only while items are selected) and the counts line.
fn selection_bar(app: &SortwiseApp, ui: &mut egui::Ui, act: &mut Option<Act>) {
    let selected = selected_entries(app);
    if !selected.is_empty() {
        egui::Frame::none()
            .fill(crate::ui::style::pal().accent_soft)
            .rounding(egui::Rounding::same(7.0))
            .inner_margin(egui::Margin::symmetric(12.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} selected", selected.len()))
                            .strong()
                            .color(crate::ui::style::pal().accent_on_soft),
                    );
                    ui.add_space(8.0);
                    if secondary_button(ui, "Stage")
                        .on_hover_text("Stage the selected files for moving")
                        .clicked()
                    {
                        *act = Some(Act::StageSelected);
                    }
                    if danger_button(ui, "Move to Trash").clicked() {
                        *act = Some(Act::TrashSelected);
                    }
                    if secondary_button(ui, "Clear").clicked() {
                        *act = Some(Act::ClearSelection);
                    }
                });
            });
        ui.add_space(4.0);
    }

}

// ---------------------------------------------------------------------------
// Modals
// ---------------------------------------------------------------------------

// Removed CPU rotate_color_image in favor of GPU-accelerated egui::Image::rotate rendering.

/// Full media player overlay: large centered image/video with ◀ / ▶ navigation
/// across the folder's files (current filter + sort order). Arrow keys also work.
pub fn player_window(app: &mut SortwiseApp, ctx: &egui::Context) {
    if !app.explorer_preview_open {
        return;
    }
    let Some(path) = app.explorer_preview_path.clone() else {
        return;
    };

    // 1. Draw a dark translucent modal backdrop overlay behind the modal window.
    // Order::Middle paints it over background panels but behind Order::Foreground / Windows,
    // so it dims the background without intercepting or swallowing clicks meant for the modal.
    egui::Area::new(egui::Id::new("player_modal_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            let screen_rect = ui.ctx().screen_rect();
            ui.painter().rect_filled(screen_rect, 0.0, Color32::from_black_alpha(120));
        });

    let filter = app.explorer_filter.trim().to_lowercase();
    let type_filter = app.explorer_type_filter;
    // Include folders so prev/next can land on them and “➕ Queue” can stage a folder.
    let slides: Vec<(PathBuf, bool)> = if app.folder_modal_open && !app.folder_modal_entries.is_empty() {
        let modal_filter = app.folder_modal_filter.trim().to_lowercase();
        app.folder_modal_entries
            .iter()
            .filter(|e| modal_filter.is_empty() || e.name.to_lowercase().contains(&modal_filter))
            .map(|e| (e.path.clone(), e.is_dir))
            .collect()
    } else {
        app.explorer_entries
            .iter()
            .filter(|e| {
                matches_type_filter(e, type_filter)
                    && (filter.is_empty() || e.name.to_lowercase().contains(&filter))
            })
            .map(|e| (e.path.clone(), e.is_dir))
            .collect()
    };
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

    let closed = crate::ui::modal::show_bare(
        ctx,
        "media_player",
        "Media player",
        crate::ui::modal::ModalSize::Screen,
        |ui| {
            // Header: Clean layout —
            // Left: Cropped filename + queue dropdown + stage button
            // Right: N/total · Trash · Cancel/Close
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Truncated filename (max 260px) with tooltip displaying full name and path
                        let disp_name = if name.chars().count() > 28 {
                            let prefix: String = name.chars().take(25).collect();
                            format!("{prefix}…")
                        } else {
                            name.clone()
                        };
                        ui.label(
                            RichText::new(&disp_name)
                                .strong()
                                .size(15.0)
                                .color(crate::ui::style::pal().text),
                        )
                        .on_hover_text(format!("{}\n{}", name, path.display()));

                        ui.add_space(8.0);
                        preview_queue_controls(app, ui, &path, path_is_dir);

                        ui.add_space(6.0);
                        let is_staged = app.is_in_movable(&path);
                        let movable_btn = if is_staged {
                            egui::Button::new(
                                RichText::new(format!("{} Staged", Icons::STAGE))
                                    .size(12.0)
                                    .strong()
                                    .color(crate::ui::style::pal().success_on_soft),
                            )
                            .fill(crate::ui::style::pal().success_soft)
                            .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().success))
                            .min_size(egui::vec2(0.0, 24.0))
                        } else {
                            egui::Button::new(
                                RichText::new(Icons::STAGE)
                                    .size(12.0)
                                    .color(crate::ui::style::pal().text),
                            )
                            .fill(crate::ui::style::pal().surface_raised)
                            .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border_strong))
                            .min_size(egui::vec2(28.0, 24.0))
                        };
                        if ui
                            .add(movable_btn)
                            .on_hover_text("Add to staging (S)")
                            .clicked()
                        {
                            app.toggle_movable(path.clone());
                        }

                        // Shortcut 'S' to toggle movable queue when not typing in text field
                        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::S)) {
                            app.toggle_movable(path.clone());
                        }

                        // Right-to-left: Cancel, Trash, Counter
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(Icons::CLOSE)
                                            .size(14.0)
                                            .strong()
                                            .color(crate::ui::style::pal().danger_on_soft),
                                    )
                                    .fill(crate::ui::style::pal().danger_soft)
                                    .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().danger_soft))
                                    .min_size(egui::vec2(28.0, 24.0)),
                                )
                                .on_hover_text("Close preview (ESC)")
                                .clicked()
                            {
                                close = true;
                            }
                            ui.add_space(6.0);
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(Icons::TRASH)
                                            .size(14.0)
                                            .color(crate::ui::style::pal().danger_on_soft),
                                    )
                                    .fill(crate::ui::style::pal().danger_soft)
                                    .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().danger_soft))
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
                                app.clear_preview_cache();
                                match trash_single_path(&path) {
                                    Ok(()) => {
                                        app.explorer_status =
                                            Some(format!("Moved “{trash_name}” to Trash."));
                                        // Targeted in-place removal: do not reload/re-scan entire directory
                                        app.explorer_entries.retain(|e| e.path != path);
                                        if app.folder_modal_open {
                                            app.folder_modal_entries.retain(|e| e.path != path);
                                        }
                                        app.thumbnail_cache.remove(&path);
                                        app.thumbnail_loading.remove(&path);

                                        if let Some(next_p) = next_preview {
                                            app.explorer_selected =
                                                app.explorer_entries.iter().position(|e| e.path == next_p);
                                            app.explorer_open_preview(next_p.clone());
                                            #[cfg(feature = "mpv")]
                                            {
                                                if crate::ui::preview::is_video_path(&next_p) {
                                                    ensure_mpv(app, ctx, &next_p);
                                                } else {
                                                    super::results_panel::update_preview(ctx, app, &next_p, 0);
                                                }
                                            }
                                            #[cfg(not(feature = "mpv"))]
                                            {
                                                super::results_panel::update_preview(ctx, app, &next_p, 0);
                                            }
                                        } else {
                                            app.close_explorer_preview();
                                        }
                                        ctx.request_repaint();
                                        return;
                                    }
                                    Err(e) => {
                                        app.explorer_status = Some(format!("Trash failed: {e}"));
                                    }
                                }
                            }
                            if let Some(i) = cur {
                                ui.add_space(6.0);
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
                        .inner_margin(egui::Margin::symmetric(14.0, 6.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Bottom Left: Breadcrumb + Utility buttons
                                preview_bottom_utilities(app, ui, &path, path_is_dir, mpv_active);

                                // Bottom Right: Transport buttons & time
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if dur > 0.0 {
                                        ui.label(
                                            RichText::new(format!(
                                                "{}  /  {}",
                                                fmt_clock(pos),
                                                fmt_clock(dur)
                                            ))
                                            .color(crate::ui::style::pal().text_muted)
                                            .size(13.0),
                                        );
                                    } else {
                                        ui.label(muted("live"));
                                    }
                                    ui.add_space(8.0);

                                    let next_btn = egui::Button::new(RichText::new("\u{E5A6}").size(15.0).strong())
                                        .fill(crate::ui::style::pal().surface_raised)
                                        .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
                                        .min_size(egui::vec2(32.0, 26.0));
                                    if ui.add_enabled(can_next, next_btn).clicked() {
                                        goto = Some(slides[cur.unwrap() + 1].0.clone());
                                    }

                                    let icon = if is_paused { "\u{E3D0}" } else { "\u{E39E}" };
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

                                    let prev_btn = egui::Button::new(RichText::new("\u{E5A4}").size(15.0).strong())
                                        .fill(crate::ui::style::pal().surface_raised)
                                        .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
                                        .min_size(egui::vec2(32.0, 26.0));
                                    if ui.add_enabled(can_prev, prev_btn).clicked() {
                                        goto = Some(slides[cur.unwrap() - 1].0.clone());
                                    }
                                });
                            });
                        });
                });

                if toggle {
                    if let Some(dec) = app.inline_video.as_ref() {
                        dec.toggle_pause();
                    }
                }
            }

            // Images / folders / other: single row — breadcrumbs + utilities (left) and prev/next (right).
            if !has_video_transport {
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Bottom Left: Breadcrumb + Utility buttons
                            preview_bottom_utilities(app, ui, &path, path_is_dir, mpv_active);

                            // Bottom Right: Prev/Next
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let next_btn = egui::Button::new(RichText::new("\u{E5A6}").size(14.0).strong())
                                    .fill(crate::ui::style::pal().surface_raised)
                                    .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
                                    .min_size(egui::vec2(32.0, 26.0));
                                if ui
                                    .add_enabled(can_next, next_btn)
                                    .on_hover_text("Next (Right Arrow)")
                                    .clicked()
                                {
                                    goto = Some(slides[cur.unwrap() + 1].0.clone());
                                }

                                let prev_btn = egui::Button::new(RichText::new("\u{E5A4}").size(14.0).strong())
                                    .fill(crate::ui::style::pal().surface_raised)
                                    .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
                                    .min_size(egui::vec2(32.0, 26.0));
                                if ui
                                    .add_enabled(can_prev, prev_btn)
                                    .on_hover_text("Previous (Left Arrow)")
                                    .clicked()
                                {
                                    goto = Some(slides[cur.unwrap() - 1].0.clone());
                                }
                            });
                        });
                    });
            }
        },
    );
    if closed {
        close = true;
    }

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

// ---------------------------------------------------------------------------
// Staging side panel (UI-Redesign-Plan §6.8)
// ---------------------------------------------------------------------------

/// Right-hand staging panel with two tabs: Items (loose files) and Folder queues.
/// Opened from the top-bar Staging button; pushes the content instead of covering it.
/// Replaces the former Queue and Movable Queue windows.
pub fn staging_panel(app: &mut SortwiseApp, ctx: &egui::Context) {
    if !(app.movable_queue_open || app.explorer_queue_view_open) {
        return;
    }
    let p = crate::ui::style::pal();
    let mut act: Option<Act> = None;
    let mut close = false;
    let mut show_items = app.movable_queue_open;
    let mut inline_toggle: Option<PathBuf> = None;

    let total_items = app.movable_items.len();
    let total_queued: usize = app.explorer_queues.iter().map(|q| q.items.len()).sum();
    let queue_names: Vec<String> = app.explorer_queues.iter().map(|q| q.name.clone()).collect();
    let queue_labels: Vec<String> = app
        .explorer_queues
        .iter()
        .map(|q| format!("{} ({})", q.name, q.items.len()))
        .collect();

    // Fixed 340 px. Rows size themselves from the available width, and a resizable panel would
    // grow to its maximum on the first frame and stay there.
    egui::SidePanel::right("staging_panel")
        .resizable(false)
        .exact_width(staging_width(ctx))
        .frame(
            egui::Frame::none()
                .fill(p.surface)
                .stroke(egui::Stroke::new(1.0, p.border))
                .inner_margin(egui::Margin::same(12.0)),
        )
        .show(ctx, |ui| {
            // Header: title, total count, close.
            ui.horizontal(|ui| {
                ui.label(RichText::new("Staged").strong().size(15.0).color(p.text));
                ui.label(muted(format!("{} total", total_items + total_queued)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::ui::style::icon_button(ui, Icons::CLOSE, false)
                        .on_hover_text("Close (Esc)")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
            ui.add_space(6.0);

            // Tabs: Folder queues first (the default), then Items.
            ui.horizontal(|ui| {
                if staging_tab(ui, &format!("Folder queues ({total_queued})"), !show_items) {
                    show_items = false;
                }
                if staging_tab(ui, &format!("Items ({total_items})"), show_items) {
                    show_items = true;
                }
            });
            ui.separator();

            // Footer first, as a bottom panel: it takes the height its rows need, and the body
            // scrolls in whatever is left. A fixed reserved height cropped the buttons.
            egui::TopBottomPanel::bottom("staging_footer")
                .resizable(false)
                .frame(egui::Frame::none().inner_margin(egui::Margin { top: 8.0, right: 12.0, ..Default::default() }))
                .show_inside(ui, |ui| {
                    if show_items {
                        items_footer(app, ui, &queue_names, &mut act);
                    } else {
                        queues_footer(app, ui, &mut act);
                    }
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::none().inner_margin(egui::Margin { right: 12.0, ..Default::default() }))
                .show_inside(ui, |ui| {
                    let body_h = ui.available_height().max(120.0);
                    if show_items {
                        items_body(app, ui, ctx, body_h, &queue_names, &mut act, &mut inline_toggle);
                    } else {
                        queues_body(app, ui, body_h, &queue_labels, &mut act);
                    }
                });
        });

    if close {
        app.movable_queue_open = false;
        app.explorer_queue_view_open = false;
    } else if show_items != app.movable_queue_open {
        app.movable_queue_open = show_items;
        app.explorer_queue_view_open = !show_items;
        if !show_items && app.explorer_active_queue.is_none() && !app.explorer_queues.is_empty() {
            app.explorer_active_queue = Some(0);
        }
    }

    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.movable_queue_open = false;
        app.explorer_queue_view_open = false;
    }

    apply_action(app, act);
    if let Some(path) = inline_toggle {
        toggle_inline_playback(app, ctx, &path);
    }
}

/// Staging panel width. Beside the content it is 340 px. On a narrow window (under 900 px) it takes
/// the whole window instead, so the toolbar and list are never squeezed into a sliver. Close it to
/// get the content back.
fn staging_width(ctx: &egui::Context) -> f32 {
    // The collapsed sidebar (56 px) stays on the left, so the panel gets the rest of the window.
    const SIDEBAR_ICONS: f32 = 56.0;
    let window = ctx.screen_rect().width();
    if window < 900.0 {
        window - SIDEBAR_ICONS
    } else {
        340.0
    }
}

/// One tab of the staging panel: text with an underline when active.
fn staging_tab(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    let p = crate::ui::style::pal();
    let text = RichText::new(label)
        .strong()
        .size(13.0)
        .color(if active { p.accent_on_soft } else { p.text_muted });
    let resp = ui.add(egui::Button::new(text).fill(egui::Color32::TRANSPARENT).stroke(egui::Stroke::NONE));
    if active {
        let r = resp.rect;
        ui.painter().rect_filled(
            egui::Rect::from_min_max(egui::pos2(r.left(), r.bottom() - 2.0), r.right_bottom()),
            0.0,
            p.accent,
        );
    }
    resp.clicked() && !active
}

/// Play or stop a video in its staging row. Same behaviour as before the panel redesign.
fn toggle_inline_playback(app: &mut SortwiseApp, ctx: &egui::Context, path: &std::path::Path) {
    if app.movable_playing_path.as_deref() == Some(path) {
        app.movable_playing_path = None;
        app.inline_video = None;
        #[cfg(feature = "mpv")]
        if let Some(p) = app.mpv.as_ref() {
            p.stop();
        }
    } else {
        app.movable_playing_path = Some(path.to_path_buf());
        #[cfg(feature = "mpv")]
        {
            ensure_mpv(app, ctx, path);
        }
        #[cfg(not(feature = "mpv"))]
        {
            let _ = ctx;
            app.inline_video = crate::ui::video_inline::InlineVideoDecoder::try_spawn(path).ok();
        }
    }
}

/// Items tab: loose files waiting for a queue or a move.
fn items_body(
    app: &mut SortwiseApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    body_h: f32,
    queue_names: &[String],
    act: &mut Option<Act>,
    inline_toggle: &mut Option<PathBuf>,
) {
    let p = crate::ui::style::pal();
    let mut assign: Option<(usize, String)> = None;
    // Fixed-size body: children cannot grow the panel beyond its width.
    let body_w = ui.available_width();
    ui.allocate_ui(egui::vec2(body_w, body_h), |ui| egui::ScrollArea::vertical()
        .max_height(body_h)
        .auto_shrink([false, false])
        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
        .id_salt("staging_items")
        .show(ui, |ui| {
            if app.movable_items.is_empty() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(muted("Nothing staged in Items."));
                    ui.add_space(4.0);
                    ui.label(muted("Stage a file from its right-click menu in Explorer."));
                });
                return;
            }
            for i in 0..app.movable_items.len() {
                // Never wider than the panel: an overlong row would push its menu button off-screen.
                ui.set_max_width(ui.available_width());
                let path = app.movable_items[i].path.clone();
                let target = app.movable_items[i].target_queue.clone();
                let fname = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let parent = path.parent().map(|d| d.display().to_string()).unwrap_or_default();
                let is_video = crate::ui::preview::is_video_path(&path);
                let is_video_or_img = is_video || crate::ui::preview::is_image_path(&path);
                let playing = app.movable_playing_path.as_ref() == Some(&path);
                let size_str = app
                    .movable_size(&path)
                    .map(format_bytes)
                    .unwrap_or_else(|| "—".to_string());
                let target_label = match &target {
                    Some(q) => format!("→ {q}"),
                    None => "No queue chosen".to_string(),
                };

                // Fixed-size row: its reported width is the panel width, so content cannot grow the panel.
                // The list's own frame keeps a 12 px gutter on the right; a full-width row runs into it.
                let row_w = (ui.available_width() - 12.0).max(200.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(row_w, 46.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                    draw_thumb_for_path(app, ui, egui::vec2(40.0, 40.0), &path);
                    ui.vertical(|ui| {
                        // Leave room for the menu button and the two item gaps (10 px each side of it).
                        // Any overflow makes the panel grow a little each frame, up to its maximum.
                        ui.set_width((ui.available_width() - 56.0).max(80.0));
                        ui.add(egui::Label::new(RichText::new(&fname).strong().color(p.text)).truncate())
                            .on_hover_text(path.display().to_string());
                        ui.add(egui::Label::new(muted(&parent)).truncate());
                        ui.label(RichText::new(format!("{size_str} · {target_label}")).size(11.5).color(p.text_muted));
                    });
                    // Row menu: a plain button that opens a popup. egui's menu_button reports a much
                    // wider size than its icon, which grew the whole panel.
                    let menu_id = egui::Id::new(("staging_row_menu", i));
                    let menu_btn = ui.add(
                        egui::Button::new(RichText::new(Icons::MORE).size(15.0)).min_size(egui::vec2(32.0, 28.0)),
                    );
                    if menu_btn.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(menu_id));
                    }
                    egui::popup::popup_below_widget(
                        ui,
                        menu_id,
                        &menu_btn,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            ui.set_min_width(170.0);
                            if is_video_or_img && ui.button("Preview").clicked() {
                                *act = Some(Act::Preview(path.clone()));
                                ui.memory_mut(|m| m.close_popup());
                            }
                            if is_video {
                                let label = if playing { "Stop playing here" } else { "Play here" };
                                if ui.button(label).clicked() {
                                    *inline_toggle = Some(path.clone());
                                    ui.memory_mut(|m| m.close_popup());
                                }
                            }
                            ui.menu_button("Move to queue", |ui| {
                                if queue_names.is_empty() {
                                    ui.label(muted("No queues yet"));
                                }
                                for qn in queue_names {
                                    if ui.button(qn).clicked() {
                                        assign = Some((i, qn.clone()));
                                        ui.memory_mut(|m| m.close_popup());
                                    }
                                }
                            });
                            if ui
                                .add_enabled(target.is_some(), egui::Button::new("Move now"))
                                .on_hover_text("Move this item to its queue's folder now")
                                .clicked()
                            {
                                *act = Some(Act::MoveSingleMovable(i));
                                ui.memory_mut(|m| m.close_popup());
                            }
                            ui.separator();
                            if ui.button("Remove").clicked() {
                                *act = Some(Act::RemoveMovable(i));
                                ui.memory_mut(|m| m.close_popup());
                            }
                        },
                    );
                });

                // Inline video viewport for the row that is playing.
                if playing {
                    let stage_w = ui.available_width();
                    let stage_h = 200.0;
                    egui::Frame::none()
                        .fill(p.canvas)
                        .rounding(egui::Rounding::same(8.0))
                        .show(ui, |ui| {
                            ui.set_min_size(egui::vec2(stage_w, stage_h));
                            ui.set_max_size(egui::vec2(stage_w, stage_h));
                            #[cfg(feature = "mpv")]
                            mpv_media(app, ui, ctx);
                            #[cfg(not(feature = "mpv"))]
                            player_media(app, ui, ctx, egui::vec2(stage_w, stage_h));
                        });
                }
                ui.add_space(4.0);
                ui.separator();
            }
        }));
    if let Some((i, q)) = assign {
        if let Some(item) = app.movable_items.get_mut(i) {
            item.target_queue = Some(q.clone());
        }
        app.movable_last_chosen_queue = Some(q);
    }
}

/// Items footer: assign every item to one queue, move the ready ones, or clear the list.
fn items_footer(app: &mut SortwiseApp, ui: &mut egui::Ui, queue_names: &[String], act: &mut Option<Act>) {
    let ready = app.movable_items.iter().filter(|item| item.target_queue.is_some()).count();
    // Step 1: send the staged items to one of the queues you made.
    ui.horizontal(|ui| {
        if !queue_names.is_empty() && !app.movable_items.is_empty() {
            let mut batch_pick = None;
            if searchable_queue_combo(ui, "movable_batch_assign", 150.0, &mut batch_pick, queue_names, "Send all to ▾") {
                if let Some(q_idx) = batch_pick {
                    let q_name = queue_names[q_idx].clone();
                    app.movable_last_chosen_queue = Some(q_name.clone());
                    for item in &mut app.movable_items {
                        item.target_queue = Some(q_name.clone());
                    }
                }
            }
            ui.label(muted(format!("{ready} of {} sent", app.movable_items.len())));
        } else if !app.movable_items.is_empty() {
            ui.label(muted("Create a queue first, then send items to it."));
        }
    });
    // Step 2: one click moves every item that has a queue.
    ui.horizontal(|ui| {
        let move_btn = egui::Button::new(
            RichText::new(format!("{} Move {ready} ready", Icons::PACKAGE))
                .strong()
                .color(crate::ui::style::pal().on_accent),
        )
        .fill(crate::ui::style::pal().accent);
        if ui
            .add_enabled(ready > 0, move_btn)
            .on_hover_text("Move each sent item to its queue's folder")
            .clicked()
        {
            *act = Some(Act::MoveAllMovable);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    !app.movable_items.is_empty(),
                    egui::Button::new(RichText::new(format!("{} Clear", Icons::CLOSE)).color(crate::ui::style::pal().danger_on_soft))
                        .frame(false),
                )
                .on_hover_text("Clear all staged items without moving")
                .clicked()
            {
                *act = Some(Act::ClearMovable);
            }
        });
    });
}

/// Folder queues tab: pick a queue, see its destination and files.
fn queues_body(app: &mut SortwiseApp, ui: &mut egui::Ui, body_h: f32, queue_labels: &[String], act: &mut Option<Act>) {
    let p = crate::ui::style::pal();
    if app.explorer_queues.is_empty() {
        ui.add_space(12.0);
        ui.label(muted("No queues yet. Create one to start staging files."));
        ui.add_space(10.0);
        if primary_button(ui, "+ New Queue").clicked() {
            *act = Some(Act::NewQueue);
        }
        return;
    }
    let idx = app.explorer_active_queue.unwrap_or(0).min(app.explorer_queues.len() - 1);
    let submitting = app.explorer_queue_submitting;

    ui.horizontal(|ui| {
        let mut pick = Some(idx);
        if searchable_queue_combo(ui, "queue_modal_select_pick", 200.0, &mut pick, queue_labels, "Choose queue…") {
            if let Some(i) = pick {
                *act = Some(Act::OpenQueue(i));
            }
        }
        if secondary_button(ui, "+ New").clicked() {
            *act = Some(Act::NewQueue);
        }
    });
    if app.explorer_queues.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            for i in 0..app.explorer_queues.len() {
                let active = app.explorer_active_queue == Some(i);
                let (fg, bg) = if active {
                    (p.on_accent, p.accent)
                } else {
                    (p.text, p.surface_raised)
                };
                if ui
                    .add(egui::Button::new(RichText::new(&queue_labels[i]).color(fg)).fill(bg))
                    .clicked()
                {
                    *act = Some(Act::OpenQueue(i));
                }
            }
        });
    }

    // Destination folder, truncated with the full path on hover.
    let target = app.explorer_queues[idx].target.clone();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(muted("Folder:"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !submitting && secondary_button(ui, "Change…").clicked() {
                *act = Some(Act::ChangeQueueTarget(idx));
            }
            let w = (ui.available_width()).max(60.0);
            ui.add_sized(
                [w, 18.0],
                egui::Label::new(RichText::new(target.display().to_string()).color(p.accent_on_soft).size(12.0)).truncate(),
            )
            .on_hover_text(target.display().to_string());
        });
    });
    ui.add_space(4.0);

    // Files staged in this queue.
    let items: Vec<PathBuf> = app.explorer_queues[idx].items.clone();
    let list_h = (body_h - 92.0).max(80.0);
    egui::Frame::none()
        .fill(p.canvas)
        .stroke(egui::Stroke::new(1.0, p.border))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(list_h)
                .auto_shrink([false, false])
                .id_salt("queue_items")
                .show(ui, |ui| {
                    if items.is_empty() {
                        ui.add_space(8.0);
                        ui.label(muted("No items staged in this queue."));
                        ui.add_space(8.0);
                    }
                    for path in &items {
                        let is_dir = path.is_dir();
                        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let label = if is_dir { format!("{} {name}/", Icons::FOLDER) } else { name };
                        ui.horizontal(|ui| {
                            if ui
                                .add(egui::Button::new(RichText::new(Icons::CLOSE).color(p.danger_on_soft)).frame(false))
                                .on_hover_text("Remove from queue")
                                .clicked()
                            {
                                *act = Some(Act::RemoveFromQueue(idx, path.clone()));
                            }
                            ui.label(RichText::new(label).color(if is_dir { p.accent_on_soft } else { p.text }))
                                .on_hover_text(path.display().to_string());
                        });
                    }
                });
        });

    if submitting {
        let (done, total, _fname) = app
            .explorer_queue_submit_progress
            .lock()
            .map(|g| g.clone())
            .unwrap_or((0, 0, String::new()));
        ui.label(muted(format!("Moving… {done}/{total}")));
    }
}

/// Folder queues footer: move everything, or delete the selected queue (its files are not touched).
fn queues_footer(app: &mut SortwiseApp, ui: &mut egui::Ui, act: &mut Option<Act>) {
    if app.explorer_queues.is_empty() {
        return;
    }
    let idx = app.explorer_active_queue.unwrap_or(0).min(app.explorer_queues.len() - 1);
    let qname = app.explorer_queues[idx].name.clone();
    let submitting = app.explorer_queue_submitting;
    let total_all: usize = app.explorer_queues.iter().map(|q| q.items.len()).sum();
    ui.horizontal(|ui| {
        let submit = egui::Button::new(
            RichText::new(format!("{} Move all ({total_all})", Icons::PACKAGE))
                .strong()
                .color(crate::ui::style::pal().on_accent),
        )
        .fill(crate::ui::style::pal().accent);
        if ui
            .add_enabled(!submitting && total_all > 0, submit)
            .on_hover_text("Move all staged items to their queues' folders")
            .clicked()
        {
            *act = Some(Act::SubmitAllQueues);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !submitting
                && ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("{} Delete queue", Icons::TRASH))
                                .color(crate::ui::style::pal().danger_on_soft),
                        )
                        .fill(crate::ui::style::pal().danger_soft)
                        .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().danger_soft)),
                    )
                    .on_hover_text(format!("Delete “{qname}” (files are not touched)"))
                    .clicked()
            {
                *act = Some(Act::DeleteQueue(idx));
            }
        });
    });
}

fn compact_path_breadcrumb(p: &std::path::Path) -> String {
    let parent = match p.parent() {
        Some(par) => par,
        None => return "/".to_string(),
    };
    let comps: Vec<String> = parent
        .components()
        .filter_map(|c| match c {
            std::path::Component::RootDir => None,
            std::path::Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();
    if comps.is_empty() {
        "/".to_string()
    } else if comps.len() > 3 {
        format!("… / {} / {} / {}", comps[comps.len() - 3], comps[comps.len() - 2], comps[comps.len() - 1])
    } else {
        comps.join(" / ")
    }
}

fn preview_bottom_utilities(
    app: &mut SortwiseApp,
    ui: &mut egui::Ui,
    path: &PathBuf,
    path_is_dir: bool,
    mpv_active: bool,
) {
    let crumb = compact_path_breadcrumb(path);
    ui.label(
        RichText::new(format!("\u{E24A} {crumb}"))
            .size(11.5)
            .color(crate::ui::style::pal().text_muted),
    )
    .on_hover_text(path.display().to_string());

    ui.add_space(2.0);
    if ui
        .add(
            egui::Button::new(RichText::new("\u{E256}").size(15.0))
                .fill(crate::ui::style::pal().surface_raised)
                .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border_strong))
                .min_size(egui::vec2(28.0, 24.0)),
        )
        .on_hover_text("Reveal in Finder / Explorer")
        .clicked()
    {
        reveal_in_folder(path);
    }

    ui.add_space(2.0);
    let open_tip = if app.inline_video.is_some() || mpv_active {
        "Open in system player (with audio)"
    } else {
        "Open in default app"
    };
    if ui
        .add(
            egui::Button::new(RichText::new("↗").size(15.0).strong())
                .fill(crate::ui::style::pal().surface_raised)
                .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border_strong))
                .min_size(egui::vec2(28.0, 24.0)),
        )
        .on_hover_text(open_tip)
        .clicked()
    {
        open_in_default_app(path);
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
        .map(|n| format!("Queue folder “{}” (moves on Submit)", n.to_string_lossy()))
        .unwrap_or_else(|| "Queue folder (moves on Submit)".into());
    if ui
        .add(
            egui::Button::new(RichText::new("\u{E24A}+").size(15.0))
                .fill(crate::ui::style::pal().surface_raised)
                .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border_strong))
                .min_size(egui::vec2(28.0, 24.0)),
        )
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
                    app.explorer_preview_queue_pick = Some(app.explorer_queues.len() - 1);
                }
                if let Some(i) = app
                    .explorer_preview_queue_pick
                    .filter(|&i| i < app.explorer_queues.len())
                {
                    app.explorer_add_to_queue(i, folder);
                } else {
                    app.explorer_status = Some("Choose a queue first".to_string());
                }
            }
        }
    }

    ui.add_space(2.0);
    if ui
        .add(
            egui::Button::new(RichText::new("\u{E036}").size(15.0))
                .fill(crate::ui::style::pal().surface_raised)
                .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border_strong))
                .min_size(egui::vec2(28.0, 24.0)),
        )
        .on_hover_text("Rotate 90°")
        .clicked()
    {
        app.explorer_preview_rotation = (app.explorer_preview_rotation + 90) % 360;
        app.preview_rotation = app.explorer_preview_rotation;
        #[cfg(feature = "mpv")]
        if mpv_active {
            if let Some(p) = app.mpv.as_ref() {
                p.set_rotation(app.explorer_preview_rotation as i64);
            }
        }
    }
}

/// Searchable combo box for selecting a queue using PopupCloseBehavior::CloseOnClickOutside
/// so interacting with the search TextEdit does not dismiss the menu.
pub(crate) fn searchable_queue_combo(
    ui: &mut egui::Ui,
    id_str: &str,
    width: f32,
    pick: &mut Option<usize>,
    queue_names: &[String],
    placeholder: &str,
) -> bool {
    let id = egui::Id::new(id_str);
    let popup_id = id.with("popup");
    let mut search: String = ui.data_mut(|d| d.get_temp(id.with("search")).unwrap_or_default());
    let selected_text = pick
        .and_then(|i| queue_names.get(i).cloned())
        .unwrap_or_else(|| placeholder.to_string());

    let is_open = ui.memory(|m| m.is_popup_open(popup_id));
    let mut just_opened = false;
    let button_response = ui.add_sized(
        [width, 24.0],
        egui::Button::new(
            RichText::new(format!(
                "{}  {}",
                selected_text,
                if is_open { "▲" } else { "▼" }
            ))
            .size(13.0),
        )
        .fill(crate::ui::style::pal().surface_raised)
        .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border)),
    );

    if button_response.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
        if !is_open {
            just_opened = true;
        }
    }

    let mut changed = false;
    egui::popup::popup_above_or_below_widget(
        ui,
        popup_id,
        &button_response,
        egui::AboveOrBelow::Below,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width.max(300.0));
            let field_w = (width - 24.0).max(180.0);
            let has_search = !search.is_empty();
            let edit_w = if has_search { field_w - 50.0 } else { field_w };
            ui.horizontal(|ui| {
                let search_resp = ui.add(
                    egui::TextEdit::singleline(&mut search)
                        .hint_text("\u{E30C} Search queue...")
                        .desired_width(edit_w),
                );
                if just_opened {
                    search_resp.request_focus();
                }
                if has_search && ui.small_button("\u{E4F6}").clicked() {
                    search.clear();
                }
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .min_scrolled_height(140.0)
                .max_height(320.0)
                .show(ui, |ui| {
                    let query = search.trim().to_lowercase();
                    let mut match_count = 0;
                    for (i, name) in queue_names.iter().enumerate() {
                        if query.is_empty() || name.to_lowercase().contains(&query) {
                            match_count += 1;
                            if ui.selectable_value(pick, Some(i), name).clicked() {
                                changed = true;
                                ui.memory_mut(|m| m.close_popup());
                            }
                        }
                    }
                    if match_count == 0 {
                        ui.label(
                            RichText::new("No matching queues")
                                .italics()
                                .color(crate::ui::style::pal().border_strong),
                        );
                    }
                });
        },
    );

    if !ui.memory(|m| m.is_popup_open(popup_id)) {
        search.clear();
    }

    ui.data_mut(|d| d.insert_temp(id.with("search"), search));
    changed
}

/// Queue combo / New Queue + Add — meant to sit on the same row as transport/nav.
fn preview_queue_controls(
    app: &mut SortwiseApp,
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
        if secondary_button(ui, "+ New Queue").clicked() {
            app.explorer_new_queue();
        }
        return;
    }

    // Auto-detect if this file/folder already belongs to a queue.
    if let Some(existing_idx) = app.explorer_find_queue_for_path(path) {
        app.explorer_preview_queue_pick = Some(existing_idx);
    }

    let mut pick = app
        .explorer_preview_queue_pick
        .filter(|&i| i < queue_names.len())
        .or(Some(0));

    // Queue dropdown (searchbox) first, then action button (Queue / Dequeue).
    searchable_queue_combo(
        ui,
        "preview_queue_pick",
        220.0,
        &mut pick,
        &queue_names,
        "Choose a queue…",
    );
    app.explorer_preview_queue_pick = pick;

    let tip = if is_dir {
        "Stage this folder (moves on Submit)"
    } else {
        "Stage this file (moves on Submit)"
    };
    if let Some(i) = pick {
        let is_in = app.explorer_is_in_queue(i, path);
        if is_in {
            let dequeue_btn = egui::Button::new(
                RichText::new("\u{E32A}")
                    .size(14.0)
                    .strong()
                    .color(crate::ui::style::pal().danger_on_soft),
            )
            .fill(crate::ui::style::pal().danger_soft)
            .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().danger))
            .min_size(egui::vec2(28.0, 24.0));

            ui.add_space(4.0);
            if ui
                .add(dequeue_btn)
                .on_hover_text(if is_dir { "Remove this folder from queue" } else { "Remove this file from queue" })
                .clicked()
            {
                app.explorer_toggle_queue(i, path.clone());
            }
            ui.add_space(2.0);
            ui.label(RichText::new("\u{E184}").color(crate::ui::style::pal().success).size(13.0))
                .on_hover_text("Staged in selected queue");
        } else {
            ui.add_space(4.0);
            if ui
                .add(
                    egui::Button::new(
                        RichText::new("\u{E3D4}")
                            .size(14.0)
                            .strong()
                            .color(crate::ui::style::pal().on_accent),
                    )
                    .fill(crate::ui::style::pal().accent)
                    .min_size(egui::vec2(28.0, 24.0)),
                )
                .on_hover_text(tip)
                .clicked()
            {
                app.explorer_toggle_queue(i, path.clone());
            }
        }
    }
}

fn player_folder_slide(ui: &mut egui::Ui, name: &str, area: egui::Vec2) {
    let stage = egui::vec2(
        area.x.min(ui.available_width()).max(120.0),
        area.y.min(ui.available_height()).max(120.0),
    );
    egui::Frame::none()
        .fill(crate::ui::style::pal().canvas)
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.set_min_size(stage);
            ui.set_max_size(stage);
            let inner = ui.available_size();
            ui.allocate_ui_with_layout(inner, egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.add_space((inner.y / 2.0 - 60.0).max(10.0));
                ui.label(
                    RichText::new("\u{E24A}")
                        .size(56.0)
                        .color(crate::ui::style::pal().accent_on_soft),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!("{name}/"))
                        .strong()
                        .size(18.0)
                        .color(crate::ui::style::pal().text),
                );
                ui.add_space(6.0);
                ui.label(muted("Folder — use \u{E3D4} Queue to stage the whole folder"));
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
    app: &mut SortwiseApp,
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
        .fill(crate::ui::style::pal().canvas)
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
                        RichText::new("\u{E30C}")
                            .size(34.0)
                            .color(crate::ui::style::pal().text_muted),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            app.preview_subtitle.clone().unwrap_or_else(|| {
                                "No inline preview — use “Open in default app”.".into()
                            }),
                        )
                        .color(crate::ui::style::pal().text_muted),
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
fn ensure_mpv(app: &mut SortwiseApp, _ctx: &egui::Context, path: &std::path::Path) -> bool {
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
        // Report only real read failures; a successful read needs no status line (U13).
        if let Err(err) = std::fs::File::open(path) {
            app.explorer_status = Some(format!("Cannot open '{}': {}", path.file_name().unwrap_or_default().to_string_lossy(), err));
            return false;
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
fn mpv_media(app: &mut SortwiseApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    egui::Frame::none()
        .fill(crate::ui::style::pal().canvas)
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
    app: &mut SortwiseApp,
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
                
                // One row: utilities (left) + transport & volume (right).
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(14.0, 6.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Bottom Left: Breadcrumb + Utility buttons
                            preview_bottom_utilities(app, ui, path, false, true);

                            // Bottom Right: transport + volume
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let vol_w = 60.0;
                                let vol_display = if is_muted { 0 } else { vol as i32 };
                                ui.label(RichText::new(format!("{}%", vol_display)).size(12.0).color(crate::ui::style::pal().text_muted));
                                
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
                                    "\u{E45C}"
                                } else if vol < 40.0 {
                                    "\u{E44E}"
                                } else if vol < 80.0 {
                                    "\u{E44C}"
                                } else {
                                    "\u{E44A}"
                                };
                                
                                let speaker_btn = egui::Button::new(RichText::new(vol_icon).size(13.0))
                                    .frame(false);
                                if ui.add(speaker_btn).clicked() {
                                    toggle_mute = true;
                                }

                                ui.add_space(8.0);

                                if dur > 0.0 {
                                    ui.label(
                                        RichText::new(format!("{}  /  {}", fmt_clock(pos), fmt_clock(dur)))
                                            .color(crate::ui::style::pal().text_muted)
                                            .size(13.0)
                                    );
                                } else {
                                    ui.label(muted("live"));
                                }

                                ui.add_space(8.0);

                                let next_btn = egui::Button::new(RichText::new(Icons::NEXT).size(15.0).strong())
                                    .fill(crate::ui::style::pal().surface_raised)
                                    .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
                                    .min_size(egui::vec2(32.0, 26.0));
                                if ui.add_enabled(can_next, next_btn).clicked() {
                                    goto = Some(slides[cur.unwrap() + 1].0.clone());
                                }

                                let icon = if paused { Icons::PLAY } else { Icons::PAUSE };
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

                                let prev_btn = egui::Button::new(RichText::new(Icons::PREV).size(15.0).strong())
                                    .fill(crate::ui::style::pal().surface_raised)
                                    .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
                                    .min_size(egui::vec2(32.0, 26.0));
                                if ui.add_enabled(can_prev, prev_btn).clicked() {
                                    goto = Some(slides[cur.unwrap() - 1].0.clone());
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

/// Inline banner after an archive was extracted (UI plan §6.2). Replaces the old modal.
/// Its Trash action goes through the shared Trash confirm; it never trashes directly.
fn extract_banner_ui(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    let Some(banner) = app.extract_banner.clone() else {
        return;
    };
    // Dismissed on navigation: the banner belongs to the folder it was shown in.
    if app.explorer_dir != banner.dir {
        app.extract_banner = None;
        return;
    }
    let name = banner.zip.file_name().unwrap_or_default().to_string_lossy().to_string();
    let (mut open_clicked, mut trash_clicked, mut dismiss_clicked) = (false, false, false);
    egui::Frame::none()
        .fill(crate::ui::style::pal().success_soft)
        .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("Extracted “{name}” to {}.", banner.dest.display()))
                        .color(crate::ui::style::pal().success_on_soft),
                );
                if secondary_button(ui, "Open folder").clicked() {
                    open_clicked = true;
                }
                if secondary_button(ui, "Move archive to Trash…")
                    .on_hover_text("You confirm before anything moves.")
                    .clicked()
                {
                    trash_clicked = true;
                }
                if secondary_button(ui, "Dismiss").clicked() {
                    dismiss_clicked = true;
                }
            });
        });
    ui.add_space(4.0);

    if open_clicked {
        app.extract_banner = None;
        app.explorer_navigate(banner.dest.clone());
    } else if trash_clicked {
        let size = std::fs::metadata(&banner.zip).map(|m| m.len()).unwrap_or(0);
        app.pending_action = Some(PendingAction::Trash {
            title: format!("Move “{name}” to Trash"),
            action: "extracted_archive",
            plan: sortwise::action_plan::for_paths(&[(banner.zip.clone(), size)]),
        });
    } else if dismiss_clicked {
        app.extract_banner = None;
    }
}

// ---------------------------------------------------------------------------
// Action application + helpers
// ---------------------------------------------------------------------------

/// Selected entries (Ctrl/Cmd multi-selection) in display order: `(path, is_dir, size)`.
/// Only entries that are currently listed count, so nothing hidden is ever acted on.
fn selected_entries(app: &SortwiseApp) -> Vec<(PathBuf, bool, u64)> {
    app.explorer_entries
        .iter()
        .filter(|e| app.explorer_multi.contains(&e.path))
        .map(|e| (e.path.clone(), e.is_dir, e.size))
        .collect()
}

/// Ctrl/Cmd-click toggles one entry; Shift-click selects the range from the anchor.
fn select_click(app: &mut SortwiseApp, idx: usize, range: bool) {
    let Some(entry) = app.explorer_entries.get(idx) else {
        return;
    };
    let path = entry.path.clone();
    if range {
        let visible = visible_indices(app);
        let anchor = app.explorer_anchor.unwrap_or(idx);
        if let (Some(a), Some(b)) = (
            visible.iter().position(|&i| i == anchor),
            visible.iter().position(|&i| i == idx),
        ) {
            for &i in &visible[a.min(b)..=a.max(b)] {
                app.explorer_multi.insert(app.explorer_entries[i].path.clone());
            }
        }
    } else {
        if !app.explorer_multi.remove(&path) {
            app.explorer_multi.insert(path);
        }
        app.explorer_anchor = Some(idx);
    }
}

/// Ask for confirmation before trashing `items` (path, size). Runs through the shared confirm dialog.
pub(crate) fn queue_trash(app: &mut SortwiseApp, items: Vec<(PathBuf, u64)>) {
    let plan = sortwise::action_plan::for_paths(&items);
    if plan.is_empty() {
        return;
    }
    app.pending_action = Some(PendingAction::Trash {
        title: "Move to Trash".to_string(),
        action: "explorer_trash",
        plan,
    });
}

/// Runs after an Explorer trash is confirmed: report the result, clear the selection, reload.
pub(crate) fn explorer_after_trash(app: &mut SortwiseApp) {
    app.explorer_status = app.apply_message.take().or_else(|| app.apply_errors.take());
    app.apply_errors = None;
    app.explorer_multi.clear();
    app.explorer_anchor = None;
    app.explorer_selected = None;
    app.explorer_reload();
    if app.folder_modal_open {
        if let Some(path) = app.folder_modal_path.clone() {
            if !path.exists() {
                app.folder_modal_next();
            }
        }
    }
}

fn apply_action(app: &mut SortwiseApp, act: Option<Act>) {
    let Some(act) = act else { return };
    // A plain open or preview starts a fresh selection.
    if matches!(act, Act::Preview(_) | Act::OpenDefault(_) | Act::Nav(_)) {
        app.explorer_multi.clear();
        app.explorer_anchor = None;
    }
    match act {
        Act::SelectClick { idx, range } => select_click(app, idx, range),
        Act::ClearSelection => {
            app.explorer_multi.clear();
            app.explorer_anchor = None;
        }
        Act::StageSelected => {
            let files: Vec<PathBuf> = selected_entries(app).into_iter().filter(|(_, is_dir, _)| !is_dir).map(|(p, _, _)| p).collect();
            let skipped = selected_entries(app).len() - files.len();
            let added = app.stage_paths(&files);
            app.explorer_status = Some(if skipped > 0 {
                format!("Staged {added} file(s). {skipped} folder(s) skipped: stage files only.")
            } else {
                format!("Staged {added} file(s).")
            });
        }
        Act::TrashSelected => {
            let items: Vec<(PathBuf, u64)> = selected_entries(app).into_iter().map(|(p, _, size)| (p, size)).collect();
            queue_trash(app, items);
        }
        Act::Stage(p) => app.toggle_movable(p),
        Act::Nav(p) => app.explorer_navigate(p),
        Act::Preview(p) => app.explorer_open_preview(p),
        Act::OpenDefault(p) => open_in_default_app(&p),
        Act::Reveal(p) => reveal_in_folder(&p),
        Act::Extract(p) => app.explorer_extract_zip(&p),
        Act::NewFolder => app.explorer_new_folder(),
        Act::Refresh => app.explorer_reload(),
        Act::Sort(k) => app.explorer_set_sort(k),
        Act::Trash(p) => {
            let size = app.explorer_entries.iter().find(|e| e.path == p).map(|e| e.size).unwrap_or(0);
            queue_trash(app, vec![(p, size)]);
        }
        Act::NewQueue => app.explorer_new_queue(),
        Act::OpenQueue(i) => {
            app.explorer_active_queue = Some(i);
            // Only one staging tab can be open: opening a queue switches to Folder queues.
            app.movable_queue_open = false;
            app.explorer_queue_view_open = true;
        }
        Act::RemoveFromQueue(i, p) => app.explorer_remove_from_queue(i, p),
        Act::SubmitAllQueues => app.submit_all_queues(),
        Act::DeleteQueue(i) => app.explorer_delete_queue(i),
        Act::ChangeQueueTarget(i) => app.explorer_change_queue_target(i),
        Act::MoveSingleMovable(idx) => {
            app.move_single_movable_item(idx);
        }
        Act::MoveAllMovable => {
            app.move_all_movable_items();
        }
        Act::RemoveMovable(idx) => app.remove_from_movable(idx),
        Act::ClearMovable => app.clear_movable_items(),
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

pub(crate) fn fmt_time(t: Option<SystemTime>) -> String {
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
    let folder_color = crate::ui::style::pal().accent;       // Sky blue
    let folder_tab_color = crate::ui::style::pal().accent_on_soft;  // Lighter sky blue
    let zip_color = crate::ui::style::pal().accent;          // Deep sky blue
    let file_color = crate::ui::style::pal().text;        // Ice white/light blue
    
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
            painter.rect_filled(zip_rect, egui::Rounding::same(r.width() * 0.04), crate::ui::style::pal().accent_soft);
            
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

pub(crate) fn draw_preview_or_icon(
    app: &mut SortwiseApp,
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

        request_thumbnail(app, &e.path, cat == Cat::Video);
    }

    // For standard icons or while loading, center a square icon inside the large grid viewport
    if size.x > 50.0 {
        let icon_sz = 40.0;
        ui.allocate_ui_with_layout(size, egui::Layout::centered_and_justified(egui::Direction::TopDown), |ui| {
            draw_sky_blue_icon(ui, egui::vec2(icon_sz, icon_sz), cat);
        });
    } else {
        draw_sky_blue_icon(ui, size, cat);
    }
}

pub fn draw_thumb_for_path(
    app: &mut SortwiseApp,
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
                // A fixed-size slot: centring alone would claim the whole remaining row width.
                ui.allocate_ui_with_layout(size, egui::Layout::centered_and_justified(egui::Direction::TopDown), |ui| {
                    ui.add(image_widget);
                });
            } else {
                ui.add(image_widget);
            }
            return;
        }

        request_thumbnail(app, path, cat == Cat::Video);
    }

    if size.x > 50.0 {
        let icon_sz = 40.0;
        ui.allocate_ui_with_layout(size, egui::Layout::centered_and_justified(egui::Direction::TopDown), |ui| {
            draw_sky_blue_icon(ui, egui::vec2(icon_sz, icon_sz), cat);
        });
    } else {
        draw_sky_blue_icon(ui, size, cat);
    }
}

pub fn request_thumbnail(app: &mut SortwiseApp, path: &std::path::Path, is_video: bool) {
    if app.thumbnail_cache.contains_key(path)
        || app.thumbnail_loading.contains(path)
        || app.thumbnail_failed.contains(path)
    {
        return;
    }
    app.thumbnail_loading.insert(path.to_path_buf());
    let epoch = app.navigation_epoch.load(std::sync::atomic::Ordering::Relaxed);
    let _ = app.thumbnail_worker_tx.try_send(crate::app::ThumbnailRequest {
        path: path.to_path_buf(),
        is_video,
        epoch,
    });
}

pub(crate) fn load_image_thumb(
    path: &std::path::Path,
) -> Result<eframe::egui::ColorImage, Box<dyn std::error::Error + Send + Sync>> {
    // 1. Embedded EXIF thumbnail (P-4): small, fast, present in most camera JPEG and RAW files.
    if let Some(img) = exif_thumbnail(path) {
        return Ok(to_color_image(&img));
    }
    // 2. Full decode, streamed from disk, with an allocation cap so a huge image fails
    //    quickly as "no thumbnail" instead of exhausting memory.
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = image::ImageDecoder::orientation(&mut decoder)?;
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(to_color_image(&img))
}

/// Fit inside 256x256, downscaling only. `DynamicImage::thumbnail` would also enlarge small
/// images, which blurs them.
fn fit_thumb(img: &image::DynamicImage) -> image::DynamicImage {
    if img.width() <= 256 && img.height() <= 256 {
        img.clone()
    } else {
        img.thumbnail(256, 256)
    }
}

fn to_color_image(img: &image::DynamicImage) -> eframe::egui::ColorImage {
    let rgba = fit_thumb(img).to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    eframe::egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw())
}

/// The camera's embedded JPEG preview, rotated per the EXIF orientation tag.
/// Returns `None` when the file has no readable EXIF preview (the caller falls back to a full decode).
fn exif_thumbnail(path: &std::path::Path) -> Option<image::DynamicImage> {
    let file = std::fs::File::open(path).ok()?;
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::BufReader::new(file))
        .ok()?;
    let offset = exif
        .get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL)?
        .value
        .get_uint(0)? as usize;
    let len = exif
        .get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL)?
        .value
        .get_uint(0)? as usize;
    let bytes = exif.buf().get(offset..offset.checked_add(len)?)?;
    let mut img = image::load_from_memory(bytes).ok()?;
    if let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
        if let Some(v) = field.value.get_uint(0) {
            img.apply_orientation(image::metadata::Orientation::from_exif(v as u8).unwrap_or(image::metadata::Orientation::NoTransforms));
        }
    }
    Some(img)
}

fn resolve_ffmpeg() -> Option<&'static str> {
    static FFMPEG: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    FFMPEG.get_or_init(|| resolve_bin("ffmpeg")).as_deref()
}

pub(crate) fn load_video_thumb(
    path: &std::path::Path,
) -> Result<eframe::egui::ColorImage, Box<dyn std::error::Error + Send + Sync>> {
    let ffmpeg = resolve_ffmpeg().ok_or("ffmpeg not found")?;
    let path_str = path.to_string_lossy().to_string();
    let output = std::process::Command::new(ffmpeg)
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

#[cfg(test)]
mod thumb_tests {
    use super::load_image_thumb;

    #[test]
    fn large_image_becomes_bounded_thumbnail() {
        let dir = std::env::temp_dir().join(format!("sfm_thumb_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.jpg");
        image::RgbImage::from_fn(3000, 2000, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]))
            .save(&path)
            .unwrap();

        let thumb = load_image_thumb(&path).expect("decodes");
        let [w, h] = thumb.size;
        assert!(w <= 256 && h <= 256, "thumbnail is {w}x{h}");
        assert_eq!(w, 256, "longest side fits the box exactly");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn embedded_exif_preview_is_used_and_rotated() {
        // Fixture: 300x200 red JPEG whose EXIF holds a 60x40 blue preview with orientation 6
        // (rotate 90° clockwise). The preview must be used, not the red main image, and rotated.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/exif_thumb_rotated.jpg");
        let thumb = load_image_thumb(&path).expect("decodes");
        assert_eq!(thumb.size, [40, 60], "preview 60x40 rotated to portrait");
        let centre = thumb.pixels[thumb.pixels.len() / 2];
        assert!(centre.b() > centre.r(), "blue preview, not the red main image: {centre:?}");
    }
}

#[cfg(test)]
mod selection_tests {
    //! The selection decides what Explorer trash touches, so these pin the affected set.
    use super::*;

    fn entry(name: &str, is_dir: bool) -> ExplorerEntry {
        ExplorerEntry {
            name: name.to_string(),
            path: PathBuf::from(format!("/d/{name}")),
            is_dir,
            size: 10,
            modified: None,
        }
    }

    fn app_with(entries: Vec<ExplorerEntry>) -> SortwiseApp {
        let mut app = SortwiseApp::default();
        app.explorer_entries = entries;
        app
    }

    fn paths(items: &[(PathBuf, bool, u64)]) -> Vec<String> {
        items.iter().map(|(p, _, _)| p.file_name().unwrap().to_string_lossy().to_string()).collect()
    }

    #[test]
    fn ctrl_click_toggles_one_entry() {
        let mut app = app_with(vec![entry("a", false), entry("b", false), entry("c", false)]);
        select_click(&mut app, 0, false);
        select_click(&mut app, 2, false);
        assert_eq!(paths(&selected_entries(&app)), vec!["a", "c"]);
        select_click(&mut app, 0, false);
        assert_eq!(paths(&selected_entries(&app)), vec!["c"]);
    }

    #[test]
    fn shift_click_selects_the_range_from_the_anchor() {
        let mut app = app_with(vec![entry("a", false), entry("b", false), entry("c", true), entry("d", false)]);
        select_click(&mut app, 1, false); // anchor on b
        select_click(&mut app, 3, true); // shift to d
        assert_eq!(paths(&selected_entries(&app)), vec!["b", "c", "d"]);
    }

    #[test]
    fn selection_never_includes_entries_no_longer_listed() {
        let mut app = app_with(vec![entry("a", false), entry("b", false)]);
        select_click(&mut app, 0, false);
        app.explorer_entries.retain(|e| e.name != "a");
        assert!(selected_entries(&app).is_empty());
    }

    #[test]
    fn staging_a_selection_never_stages_a_file_twice() {
        let mut app = app_with(vec![entry("a", false), entry("b", false)]);
        let a = PathBuf::from("/d/a");
        assert_eq!(app.stage_paths(&[a.clone(), PathBuf::from("/d/b")]), 2);
        assert_eq!(app.stage_paths(&[a.clone()]), 0);
        assert_eq!(app.movable_items.len(), 2);
    }

    #[test]
    fn trash_plan_for_selection_is_exactly_the_selected_paths() {
        let mut app = app_with(vec![entry("a", false), entry("b", false), entry("c", false)]);
        select_click(&mut app, 0, false);
        select_click(&mut app, 2, false);
        let items: Vec<(PathBuf, u64)> = selected_entries(&app).into_iter().map(|(p, _, s)| (p, s)).collect();
        queue_trash(&mut app, items);
        match app.pending_action {
            Some(PendingAction::Trash { ref plan, action, .. }) => {
                assert_eq!(action, "explorer_trash");
                assert_eq!(plan.paths, vec![PathBuf::from("/d/a"), PathBuf::from("/d/c")]);
            }
            _ => panic!("expected a pending Trash action"),
        }
    }
}
