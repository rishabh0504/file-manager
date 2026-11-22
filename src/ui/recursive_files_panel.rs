//! Header "Files" tab: flat recursive list of every file under a chosen folder, shown as
//! preview cards. Two parent tabs (Filter, Sort) control the list. Clicking a card opens the
//! shared preview modal, whose arrow keys step through this same filtered, sorted list.
//! Staged (Movable Queue) files show their destination. Files moved out of the folder are
//! removed from the list. Nothing is moved on disk from this view by itself.

use crate::app::{ExplorerFilesSort, ExplorerTypeFilter, FilesFilter, FilesPanelMode, FilesStage, StagedFilter, SortwiseApp};
use crate::ui::explorer_panel::draw_thumb_for_path;
use crate::ui::preview::format_bytes;
use crate::ui::style::{muted, secondary_button};
use eframe::egui::{self, Color32, CursorIcon, Frame, Margin, Rect, RichText, Rounding, ScrollArea, Stroke};
use sortwise::recursive_files::RecursiveFile;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const CARD_W: f32 = 240.0;
const CARD_H: f32 = 256.0;
const CARD_GAP: f32 = 12.0;
const CARD_PAD: f32 = 8.0;
const THUMB_H: f32 = 136.0;

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "heic"];
const VIDEO_EXTS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "mpg", "mpeg"];
const ARCHIVE_EXTS: &[&str] = &["zip", "rar", "7z", "tar", "gz", "bz2", "xz"];

fn kind_matches(path: &Path, filter: ExplorerTypeFilter) -> bool {
    let exts = match filter {
        ExplorerTypeFilter::All | ExplorerTypeFilter::Folders => return true,
        ExplorerTypeFilter::Image => IMAGE_EXTS,
        ExplorerTypeFilter::Video => VIDEO_EXTS,
        ExplorerTypeFilter::Archive => ARCHIVE_EXTS,
    };
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn file_matches(file: &RecursiveFile, f: &FilesFilter, query: &str, staged: &HashSet<PathBuf>, now: SystemTime) -> bool {
    if !query.is_empty() && !file.rel.to_lowercase().contains(query) {
        return false;
    }
    if !kind_matches(&file.path, f.kind) {
        return false;
    }
    if f.min_size.is_some_and(|m| file.size < m) || f.max_size.is_some_and(|m| file.size > m) {
        return false;
    }
    if let Some(days) = f.modified_within_days {
        if let Some(cutoff) = now.checked_sub(Duration::from_secs(u64::from(days) * 86_400)) {
            match file.modified {
                Some(t) if t >= cutoff => {}
                _ => return false,
            }
        }
    }
    let is_staged = staged.contains(&file.path);
    match f.staged {
        StagedFilter::All => true,
        StagedFilter::StagedOnly => is_staged,
        StagedFilter::NotStaged => !is_staged,
    }
}

/// Rebuild `explorer_files_view` (filtered + sorted indices) only when an input changed.
fn ensure_view(app: &mut SortwiseApp) {
    let key = (
        app.files_filter.clone(),
        app.explorer_files_sort,
        app.explorer_files_sort_asc,
        app.explorer_files_gen,
        app.explorer_files_staged_fingerprint(),
    );
    if app.explorer_files_view_key.as_ref() == Some(&key) {
        return;
    }
    let (f, sort, asc, _, _) = &key;
    let query = f.name.trim().to_lowercase();
    let staged: HashSet<PathBuf> = app.explorer_files_staged_set();
    let now = SystemTime::now();
    let files = &app.explorer_files;
    let mut idx: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| file_matches(file, f, &query, &staged, now))
        .map(|(i, _)| i)
        .collect();
    idx.sort_by(|&a, &b| {
        let (fa, fb) = (&files[a], &files[b]);
        let ord = match sort {
            ExplorerFilesSort::Size => fa.size.cmp(&fb.size),
            ExplorerFilesSort::Name => fa.rel.to_lowercase().cmp(&fb.rel.to_lowercase()),
            ExplorerFilesSort::Modified => fa.modified.cmp(&fb.modified),
            ExplorerFilesSort::Type => {
                let ea = fa.path.extension().map(|e| e.to_string_lossy().to_lowercase());
                let eb = fb.path.extension().map(|e| e.to_string_lossy().to_lowercase());
                ea.cmp(&eb)
            }
        }
        .then_with(|| fa.rel.cmp(&fb.rel));
        if *asc { ord } else { ord.reverse() }
    });
    app.explorer_files_view = idx;
    app.explorer_files_view_key = Some(key);
}

/// Small rounded label used for header counts. Returns the response so callers can make it clickable.
fn chip(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) -> egui::Response {
    Frame::none()
        .fill(bg)
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(text).size(12.0).color(fg)).sense(egui::Sense::click()))
        })
        .inner
}

/// One line describing every active filter, e.g. "Videos · Over 100 MB · Staged only".
fn filter_summary(f: &FilesFilter) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some((label, _)) = TYPE_OPTIONS.iter().find(|(_, v)| *v == f.kind) {
        parts.push((*label).to_string());
    }
    if let Some((label, _)) = SIZE_OPTIONS.iter().find(|(_, v)| *v == (f.min_size, f.max_size)) {
        if *label != "Any size" {
            parts.push((*label).to_string());
        }
    }
    if let Some((label, _)) = MODIFIED_OPTIONS.iter().find(|(_, v)| *v == f.modified_within_days) {
        if f.modified_within_days.is_some() {
            parts.push((*label).to_string());
        }
    }
    if let Some((label, _)) = STAGED_OPTIONS.iter().find(|(_, v)| *v == f.staged) {
        if f.staged != StagedFilter::All {
            parts.push((*label).to_string());
        }
    }
    let name = f.name.trim();
    if !name.is_empty() {
        parts.push(format!("“{name}”"));
    }
    parts.join(" · ")
}

fn elide(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let kept: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// A dropdown over a fixed set of choices. `current` is set to the chosen value.
fn choice<T: Copy + PartialEq>(ui: &mut egui::Ui, id: &str, width: f32, current: &mut T, options: &[(&str, T)]) {
    let label = options
        .iter()
        .find(|(_, v)| v == current)
        .map(|(l, _)| *l)
        .unwrap_or("Custom");
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .width(width)
        .show_ui(ui, |ui| {
            for (l, v) in options {
                ui.selectable_value(&mut *current, *v, *l);
            }
        });
}

/// What the user picked in a queue picker.
pub(crate) enum PickerAction {
    None,
    Queue(usize),
    Unstage,
}

/// Width of the queue picker popup. Much wider than the button so long queue names read in full.
const QUEUE_POPUP_W: f32 = 420.0;
const QUEUE_ROW_H: f32 = 32.0;

/// Queue dropdown for one item in the Explorer grid. Same staging as the Files cards: a pick puts
/// the path in that folder queue, and "Remove from staging" clears it from every list. Returns the
/// button rect so the caller can keep tile clicks from opening the file.
pub(crate) fn item_queue_picker(app: &mut SortwiseApp, ui: &mut egui::Ui, path: &Path, width: f32) -> Rect {
    let names: Vec<String> = app.explorer_queues.iter().map(|q| q.name.clone()).collect();
    let stage = app.explorer_files_stage_of(path);
    let label = match &stage {
        FilesStage::Queue(q) => elide(q, 12),
        FilesStage::Item(Some(q)) => format!("Items · {}", elide(q, 8)),
        FilesStage::Item(None) => "In Items".to_string(),
        FilesStage::None => "Queue…".to_string(),
    };
    let current = match &stage {
        FilesStage::Queue(q) => Some(q.as_str()),
        _ => None,
    };
    let top_left = ui.cursor().min;
    let action = queue_picker(
        ui,
        egui::Id::new(("explorer_tile_queue", path)),
        width,
        &names,
        &label,
        current,
        stage != FilesStage::None,
    );
    match action {
        PickerAction::Queue(i) => {
            app.explorer_files_stage(&[path.to_path_buf()], Some(names[i].clone()));
        }
        PickerAction::Unstage => {
            app.explorer_files_unstage(path);
            app.explorer_status = Some("Removed from staging.".to_string());
        }
        PickerAction::None => {}
    }
    Rect::from_min_size(top_left, egui::vec2(width, 30.0))
}

/// Searchable queue dropdown for the Files tab: a large popup with a search box and big rows.
/// Lists queue names only; the destination folder is not repeated.
fn queue_picker(
    ui: &mut egui::Ui,
    id: egui::Id,
    button_w: f32,
    names: &[String],
    button_label: &str,
    highlight: Option<&str>,
    can_unstage: bool,
) -> PickerAction {
    let popup_id = id.with("popup");
    let search_id = id.with("search");
    let pal = crate::ui::style::pal();
    let is_open = ui.memory(|m| m.is_popup_open(popup_id));

    let button = ui.add_sized(
        [button_w, 30.0],
        egui::Button::new(
            RichText::new(format!("{button_label}  {}", if is_open { "▲" } else { "▼" })).size(14.0),
        )
        .fill(pal.surface_raised)
        .stroke(Stroke::new(1.0, pal.border)),
    );
    let mut just_opened = false;
    if button.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
        just_opened = !is_open;
    }

    let mut search: String = ui.data_mut(|d| d.get_temp(search_id).unwrap_or_default());
    let mut action = PickerAction::None;
    egui::popup::popup_above_or_below_widget(
        ui,
        popup_id,
        &button,
        egui::AboveOrBelow::Below,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            let popup_w = QUEUE_POPUP_W.max(button_w);
            ui.set_min_width(popup_w);
            ui.set_max_width(popup_w);
            let search_resp = ui.add(
                egui::TextEdit::singleline(&mut search)
                    .hint_text("Search queues…")
                    .desired_width(popup_w - 16.0)
                    .font(egui::TextStyle::Body),
            );
            if just_opened {
                search_resp.request_focus();
            }
            ui.separator();

            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                let query = search.trim().to_lowercase();
                if can_unstage
                    && ui
                        .add_sized(
                            [popup_w - 16.0, QUEUE_ROW_H],
                            egui::SelectableLabel::new(false, RichText::new("Remove from staging").size(15.0).color(pal.danger_on_soft)),
                        )
                        .clicked()
                {
                    action = PickerAction::Unstage;
                    ui.memory_mut(|m| m.close_popup());
                }
                let mut shown = 0;
                for (i, name) in names.iter().enumerate() {
                    if !query.is_empty() && !name.to_lowercase().contains(&query) {
                        continue;
                    }
                    shown += 1;
                    let selected = highlight == Some(name.as_str());
                    if ui
                        .add_sized(
                            [popup_w - 16.0, QUEUE_ROW_H],
                            egui::SelectableLabel::new(selected, RichText::new(name).size(15.0)),
                        )
                        .clicked()
                    {
                        action = PickerAction::Queue(i);
                        ui.memory_mut(|m| m.close_popup());
                    }
                }
                if shown == 0 {
                    ui.add_space(6.0);
                    ui.label(muted(if names.is_empty() {
                        "No queues yet. Create one from the Queue view.".to_string()
                    } else {
                        "No queue matches the search.".to_string()
                    }));
                }
            });
        },
    );
    ui.data_mut(|d| d.insert_temp(search_id, search));
    action
}

const TYPE_OPTIONS: &[(&str, ExplorerTypeFilter)] = &[
    ("Videos", ExplorerTypeFilter::Video),
    ("Zips / archives", ExplorerTypeFilter::Archive),
    ("Images", ExplorerTypeFilter::Image),
    ("All files", ExplorerTypeFilter::All),
];

/// (min, max) byte bounds for one size preset.
type SizeBounds = (Option<u64>, Option<u64>);

const SIZE_OPTIONS: &[(&str, SizeBounds)] = &[
    ("Any size", (None, None)),
    ("Over 10 MB", (Some(10 * MB), None)),
    ("Over 100 MB", (Some(100 * MB), None)),
    ("Over 1 GB", (Some(GB), None)),
    ("Under 10 MB", (None, Some(10 * MB))),
];

const MODIFIED_OPTIONS: &[(&str, Option<u32>)] = &[
    ("Any time", None),
    ("Last 24 hours", Some(1)),
    ("Last 7 days", Some(7)),
    ("Last 30 days", Some(30)),
    ("Last year", Some(365)),
];

const STAGED_OPTIONS: &[(&str, StagedFilter)] = &[
    ("All files", StagedFilter::All),
    ("Staged only", StagedFilter::StagedOnly),
    ("Not staged", StagedFilter::NotStaged),
];

const SORT_OPTIONS: &[(&str, ExplorerFilesSort)] = &[
    ("Size", ExplorerFilesSort::Size),
    ("Name", ExplorerFilesSort::Name),
    ("Modified", ExplorerFilesSort::Modified),
    ("Type", ExplorerFilesSort::Type),
];

const ORDER_OPTIONS: &[(&str, bool)] = &[("Descending", false), ("Ascending", true)];

/// Full Files tab page: folder bar, Filter / Sort parent tabs, then the card list.
pub fn recursive_files_page(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    let dir_label = app
        .files_view_dir
        .as_ref()
        .map(|d| d.display().to_string())
        .unwrap_or_else(|| "No folder".into());
    let has_folder = app.files_view_dir.is_some();
    if has_folder {
        // Run before drawing so the header counts match the list below.
        app.explorer_files_ensure();
        ensure_view(app);
    }
    let pal = crate::ui::style::pal();

    // Header card: title, folder, actions, then count chips.
    Frame::none()
        .fill(pal.surface)
        .stroke(Stroke::new(1.0, pal.border))
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Files").strong().size(18.0));
                // Leave room for the two buttons on the right; the full path is in the tooltip.
                let max_chars = ((ui.available_width() - 290.0) / 7.0).clamp(8.0, 64.0) as usize;
                ui.label(muted(elide(&dir_label, max_chars))).on_hover_text(dir_label.clone());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(has_folder, |ui| {
                        if secondary_button(ui, "⟳ Refresh")
                            .on_hover_text("Re-scan this folder for new, renamed or deleted files")
                            .clicked()
                        {
                            app.explorer_files_refresh();
                        }
                    });
                    if secondary_button(ui, "Choose folder…").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            app.files_view_dir = Some(dir);
                        }
                    }
                });
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if !has_folder {
                    ui.label(muted("Choose a folder to list its files."));
                    return;
                }
                let total: u64 = app.explorer_files_view.iter().map(|&i| app.explorer_files[i].size).sum();
                chip(ui, &format!("{} files", app.explorer_files_view.len()), pal.text, pal.surface_raised);
                chip(ui, &format_bytes(total), pal.text, pal.surface_raised);
                // Staged files under this folder: in a folder queue or in Items.
                let staged_set = app.explorer_files_staged_set();
                let staged = app.explorer_files.iter().filter(|f| staged_set.contains(&f.path)).count();
                let (fg, bg) = if staged > 0 {
                    (pal.success_on_soft, pal.success_soft)
                } else {
                    (pal.text_muted, pal.surface_raised)
                };
                // Click the staged chip to show only staged files.
                if chip(ui, &format!("● {staged} staged"), fg, bg)
                    .on_hover_text("Click to show only staged files")
                    .clicked()
                {
                    app.files_filter.staged = StagedFilter::StagedOnly;
                    app.files_panel_mode = FilesPanelMode::Filter;
                }
                if app.explorer_files_loading {
                    ui.spinner();
                    ui.label(muted("Scanning…"));
                }
                if app.explorer_files_truncated {
                    ui.label(
                        RichText::new("Listing capped; open a smaller folder for the rest.")
                            .color(pal.warning)
                            .size(11.5),
                    );
                }
            });
        });
    ui.add_space(8.0);

    // Parent tabs on the left; active-filter summary and reset on the right.
    // Search and Sort controls are only available once a folder is chosen.
    ui.add_enabled_ui(has_folder, |ui| {
        ui.horizontal(|ui| {
            for (mode, label) in [(FilesPanelMode::Filter, "Filter"), (FilesPanelMode::Sort, "Sort")] {
                if ui.selectable_label(app.files_panel_mode == mode, RichText::new(label).strong()).clicked() {
                    app.files_panel_mode = mode;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_button(ui, "Reset").on_hover_text("Clear all filters").clicked() {
                    app.files_filter = FilesFilter::default();
                }
                let max_chars = ((ui.available_width() - 90.0) / 7.0).clamp(0.0, 70.0) as usize;
                if max_chars >= 8 {
                    ui.label(muted(elide(&filter_summary(&app.files_filter), max_chars)));
                }
            });
        });
        ui.add_space(4.0);
        match app.files_panel_mode {
            FilesPanelMode::Filter => {
                // Combo boxes do not wrap on their own: split into two rows when narrow.
                let two_rows = ui.available_width() < 860.0;
                ui.horizontal(|ui| {
                    let name_w = if two_rows { (ui.available_width() - 170.0).clamp(120.0, 320.0) } else { 220.0 };
                    ui.add(
                        egui::TextEdit::singleline(&mut app.files_filter.name)
                            .hint_text(format!("{}  Name contains…", crate::ui::icons::Icons::SEARCH))
                            .margin(egui::Margin::symmetric(8.0, 6.0))
                            .desired_width(name_w),
                    );
                    choice(ui, "files_f_type", 140.0, &mut app.files_filter.kind, TYPE_OPTIONS);
                    if !two_rows {
                        files_filter_rest(app, ui);
                    }
                });
                if two_rows {
                    ui.horizontal(|ui| files_filter_rest(app, ui));
                }
            }
            FilesPanelMode::Sort => {
                ui.horizontal(|ui| {
                    ui.label(muted("Sort by"));
                    choice(ui, "files_s_key", 130.0, &mut app.explorer_files_sort, SORT_OPTIONS);
                    choice(ui, "files_s_order", 130.0, &mut app.explorer_files_sort_asc, ORDER_OPTIONS);
                });
            }
        }
    });
    ui.add_space(6.0);

    if !has_folder {
        ui.add_space(12.0);
        ui.label(muted("Choose a folder to list its files. Filter and sort unlock after that."));
        return;
    }
    let table_h = ui.available_height().max(160.0);
    recursive_files_view(app, ui, table_h);
}

pub fn recursive_files_view(app: &mut SortwiseApp, ui: &mut egui::Ui, table_h: f32) {
    app.explorer_files_ensure();
    ensure_view(app);

    // Action bar for checked files.
    let names: Vec<String> = app.explorer_queues.iter().map(|q| q.name.clone()).collect();
    let mut chosen: Option<Option<String>> = None;
    let checked_n = app.explorer_files_checked.len();
    ui.horizontal_wrapped(|ui| {
        if secondary_button(ui, "Select visible").clicked() {
            for &i in &app.explorer_files_view {
                app.explorer_files_checked.insert(app.explorer_files[i].path.clone());
            }
        }
        if secondary_button(ui, "Clear").clicked() {
            app.explorer_files_checked.clear();
        }
        ui.label(muted(format!("{checked_n} checked")));
        ui.add_enabled_ui(checked_n > 0, |ui| {
            if let PickerAction::Queue(i) = queue_picker(
                ui,
                egui::Id::new("files_bulk_queue"),
                200.0,
                &names,
                "Queue checked to…",
                None,
                false,
            ) {
                chosen = Some(Some(names[i].clone()));
            }
            if ui
                .button("Add to Items")
                .on_hover_text("Stage in the Items tab without a queue; pick a queue there later")
                .clicked()
            {
                chosen = Some(None);
            }
        });
    });
    ui.add_space(4.0);

    if app.explorer_files_view.is_empty() && !app.explorer_files_loading {
        ui.add_space(12.0);
        ui.label(muted("No files match these filters. Dot-files, system files and text/code files are never listed."));
        return;
    }

    let n = app.explorer_files_view.len();
    let avail = ui.available_width();
    let cols = ((avail + CARD_GAP) / (CARD_W + CARD_GAP)).floor().max(1.0) as usize;
    let rows = n.div_ceil(cols);

    // Card actions are collected while drawing and applied afterwards.
    let mut row_pick: Option<(PathBuf, String)> = None;
    let mut unstage: Option<PathBuf> = None;
    let mut open: Option<PathBuf> = None;
    ScrollArea::vertical()
        .max_height(table_h)
        .auto_shrink([false, false])
        .show_rows(ui, CARD_H + CARD_GAP, rows, |ui, range| {
            ui.spacing_mut().item_spacing = egui::vec2(CARD_GAP, CARD_GAP);
            for r in range {
                ui.horizontal(|ui| {
                    for c in 0..cols {
                        let i = r * cols + c;
                        if i >= n {
                            break;
                        }
                        let file = app.explorer_files[app.explorer_files_view[i]].clone();
                        file_card(app, ui, &file, &names, &mut row_pick, &mut unstage, &mut open);
                    }
                });
            }
        });

    if let Some(path) = unstage {
        app.explorer_files_unstage(&path);
        app.explorer_status = Some("Removed from staging.".to_string());
    }
    if let Some((path, queue)) = row_pick {
        app.explorer_files_stage(&[path], Some(queue));
    }
    if let Some(path) = open {
        app.files_open_preview(path);
    }
    if let Some(dest) = chosen {
        let paths: Vec<PathBuf> = app
            .explorer_files
            .iter()
            .filter(|f| app.explorer_files_checked.contains(&f.path))
            .map(|f| f.path.clone())
            .collect();
        if !paths.is_empty() {
            app.explorer_files_stage(&paths, dest);
            app.explorer_files_checked.clear();
        }
    }
}

/// One preview card: large thumbnail, name, size, staged state, check box and queue picker.
/// Clicking the card body (outside the check box and picker) opens the preview modal.
fn file_card(
    app: &mut SortwiseApp,
    ui: &mut egui::Ui,
    f: &RecursiveFile,
    names: &[String],
    row_pick: &mut Option<(PathBuf, String)>,
    unstage: &mut Option<PathBuf>,
    open: &mut Option<PathBuf>,
) {
    let path = f.path.clone();
    let stage = app.explorer_files_stage_of(&path);
    let is_staged = stage != FilesStage::None;
    let is_checked = app.explorer_files_checked.contains(&path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_uppercase())
        .unwrap_or_else(|| "FILE".into());
    let inner_w = CARD_W - CARD_PAD * 2.0;

    let mut child_rects: Vec<Rect> = Vec::new();
    let pal = crate::ui::style::pal();
    let stroke = if is_staged { pal.success } else { pal.border };
    let frame = Frame::none()
        .fill(if is_checked { pal.accent_soft } else { pal.surface_raised })
        .stroke(Stroke::new(1.0, stroke))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(CARD_PAD))
        .show(ui, |ui| {
            ui.set_width(inner_w);
            ui.set_height(CARD_H - CARD_PAD * 2.0);
            ui.vertical(|ui| {
                draw_thumb_for_path(app, ui, egui::vec2(inner_w, THUMB_H), &path);
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(elide(&name, 30)).strong()))
                    .on_hover_text(path.display().to_string());
                match &stage {
                    FilesStage::Queue(q) => {
                        ui.label(RichText::new(format!("● In queue {}", elide(q, 22))).color(pal.success_on_soft).size(11.5))
                            .on_hover_text("Listed in Staged → Folder queues; moves on Submit");
                    }
                    FilesStage::Item(Some(q)) => {
                        ui.label(RichText::new(format!("● Items → {}", elide(q, 22))).color(pal.success_on_soft).size(11.5))
                            .on_hover_text("Listed in Staged → Items");
                    }
                    FilesStage::Item(None) => {
                        ui.label(RichText::new("● In Items (no queue)").color(pal.success_on_soft).size(11.5))
                            .on_hover_text("Listed in Staged → Items");
                    }
                    FilesStage::None => {
                        ui.label(muted(" "));
                    }
                }
                ui.horizontal(|ui| {
                    let mut on = is_checked;
                    let cb = ui.checkbox(&mut on, "");
                    child_rects.push(cb.rect);
                    if cb.changed() {
                        if on {
                            app.explorer_files_checked.insert(path.clone());
                        } else {
                            app.explorer_files_checked.remove(&path);
                        }
                    }
                    ui.label(muted(format!("{} · {}", format_bytes(f.size), ext)));
                });
                let current = match &stage {
                    FilesStage::Queue(q) => Some(q.clone()),
                    _ => None,
                };
                let button_label = match &stage {
                    FilesStage::Queue(q) => q.clone(),
                    _ => "Add to queue…".to_string(),
                };
                let picker_rect = ui.cursor();
                match queue_picker(
                    ui,
                    egui::Id::new(("files_card_queue", &path)),
                    inner_w,
                    names,
                    &button_label,
                    current.as_deref(),
                    is_staged,
                ) {
                    PickerAction::Queue(i) => *row_pick = Some((path.clone(), names[i].clone())),
                    PickerAction::Unstage => *unstage = Some(path.clone()),
                    PickerAction::None => {}
                }
                child_rects.push(egui::Rect::from_min_size(picker_rect.min, egui::vec2(inner_w, 30.0)));
            });
        });

    // Click on the card body. A card must not react while a modal (preview, progress, dialog)
    // covers it, so the pointer's top layer has to be this card's layer. Child widgets are
    // excluded by rect, because the card's own hit area would otherwise sit above them.
    let rect = frame.response.rect;
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let own_layer = ui.layer_id();
    let over_card = !app.explorer_preview_open
        && pointer.is_some_and(|p| {
            rect.contains(p)
                && !child_rects.iter().any(|r| r.contains(p))
                && ui.ctx().layer_id_at(p) == Some(own_layer)
        });
    if over_card {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        if ui.input(|i| i.pointer.primary_released()) {
            *open = Some(path.clone());
        }
    }
}

/// Size, modified and staged filters (second filter row on narrow windows).
fn files_filter_rest(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    let mut size = (app.files_filter.min_size, app.files_filter.max_size);
    choice(ui, "files_f_size", 130.0, &mut size, SIZE_OPTIONS);
    app.files_filter.min_size = size.0;
    app.files_filter.max_size = size.1;
    choice(ui, "files_f_modified", 130.0, &mut app.files_filter.modified_within_days, MODIFIED_OPTIONS);
    choice(ui, "files_f_staged", 120.0, &mut app.files_filter.staged, STAGED_OPTIONS);
}
