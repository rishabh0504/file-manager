//! Explorer "Files" view: flat recursive list of every file under the current folder.
//! Default order: size, largest first. Each file can be checked and sent to a queue
//! (Movable Queue, with a destination). Nothing is moved on disk from this view.

use crate::app::{ExplorerFilesSort, ExplorerTypeFilter, SortwiseApp};
use crate::ui::explorer_panel::fmt_time;
use crate::ui::preview::format_bytes;
use crate::ui::style::{muted, secondary_button};
use eframe::egui::{self, RichText, ScrollArea};
use std::path::{Path, PathBuf};

const ROW_H: f32 = 26.0;
/// Width taken by checkbox, size, modified, queue tag and queue picker, plus spacing.
const FIXED_COLS_W: f32 = 440.0;

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

/// Rebuild `explorer_files_view` (filtered + sorted indices) only when an input changed.
fn ensure_view(app: &mut SortwiseApp) {
    let key = (
        app.explorer_filter.trim().to_lowercase(),
        app.explorer_type_filter,
        app.explorer_files_sort,
        app.explorer_files_sort_asc,
        app.explorer_files_gen,
    );
    if app.explorer_files_view_key.as_ref() == Some(&key) {
        return;
    }
    let (query, kind, sort, asc, _) = &key;
    let files = &app.explorer_files;
    let mut idx: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, f)| (query.is_empty() || f.rel.to_lowercase().contains(query.as_str())) && kind_matches(&f.path, *kind))
        .map(|(i, _)| i)
        .collect();
    idx.sort_by(|&a, &b| {
        let (fa, fb) = (&files[a], &files[b]);
        let ord = match sort {
            ExplorerFilesSort::Size => fa.size.cmp(&fb.size),
            ExplorerFilesSort::Name => fa.rel.to_lowercase().cmp(&fb.rel.to_lowercase()),
            ExplorerFilesSort::Modified => fa.modified.cmp(&fb.modified),
        }
        .then_with(|| fa.rel.cmp(&fb.rel));
        if *asc { ord } else { ord.reverse() }
    });
    app.explorer_files_view = idx;
    app.explorer_files_view_key = Some(key);
}

fn elide(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let kept: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{kept}…")
}

pub fn recursive_files_view(app: &mut SortwiseApp, ui: &mut egui::Ui, table_h: f32) {
    app.explorer_files_ensure();
    ensure_view(app);

    let total: u64 = app.explorer_files_view.iter().map(|&i| app.explorer_files[i].size).sum();
    let warn = crate::ui::style::pal().warning;

    // Header: counts, status, sort.
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{} files · {}",
                app.explorer_files_view.len(),
                format_bytes(total)
            ))
            .size(12.0),
        );
        if app.explorer_files_loading {
            ui.spinner();
            ui.label(muted("Scanning…"));
        }
        if app.explorer_files_truncated {
            ui.label(RichText::new("Listing capped; open a smaller folder for the rest.").color(warn).size(11.5));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let arrow = if app.explorer_files_sort_asc { "▲" } else { "▼" };
            if ui.button(arrow).on_hover_text("Toggle sort direction").clicked() {
                app.explorer_files_sort_asc = !app.explorer_files_sort_asc;
            }
            let label = match app.explorer_files_sort {
                ExplorerFilesSort::Size => "Size",
                ExplorerFilesSort::Name => "Name",
                ExplorerFilesSort::Modified => "Modified",
            };
            egui::ComboBox::from_id_salt("files_view_sort")
                .selected_text(label)
                .width(100.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.explorer_files_sort, ExplorerFilesSort::Size, "Size");
                    ui.selectable_value(&mut app.explorer_files_sort, ExplorerFilesSort::Name, "Name");
                    ui.selectable_value(&mut app.explorer_files_sort, ExplorerFilesSort::Modified, "Modified");
                });
            ui.label(muted("Sort"));
        });
    });

    // Action bar for checked files.
    let names: Vec<String> = app.explorer_queues.iter().map(|q| q.name.clone()).collect();
    let mut chosen: Option<Option<String>> = None;
    let checked_n = app.explorer_files_checked.len();
    ui.horizontal(|ui| {
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
            egui::ComboBox::from_id_salt("files_bulk_queue")
                .selected_text("Queue checked to…")
                .show_ui(ui, |ui| {
                    for n in &names {
                        if ui.selectable_label(false, n).clicked() {
                            chosen = Some(Some(n.clone()));
                        }
                    }
                });
            if ui
                .button("Add to Movable Queue")
                .on_hover_text("Stage without a destination; pick one later in the queue")
                .clicked()
            {
                chosen = Some(None);
            }
        });
    });
    ui.add_space(4.0);

    if app.explorer_files_view.is_empty() && !app.explorer_files_loading {
        ui.add_space(12.0);
        ui.label(muted("No files match. Dot-files, system files and text/code files are not listed."));
        return;
    }

    // Per-row queue picks are collected during drawing and applied afterwards.
    let mut row_pick: Option<(PathBuf, String)> = None;
    {
        let view = &app.explorer_files_view;
        let files = &app.explorer_files;
        let checked = &mut app.explorer_files_checked;
        let movable = &app.movable_items;
        let avail = ui.available_width();
        let text_w = (avail - FIXED_COLS_W).max(240.0);
        let name_w = text_w * 0.45;
        let rel_w = text_w * 0.55;

        ScrollArea::vertical()
            .max_height(table_h)
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, view.len(), |ui, range| {
                for row in range {
                    let idx = view[row];
                    let f = &files[idx];
                    let path = &f.path;
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let staged = movable.iter().find(|m| &m.path == path);

                    ui.horizontal(|ui| {
                        let mut on = checked.contains(path);
                        if ui.checkbox(&mut on, "").changed() {
                            if on {
                                checked.insert(path.clone());
                            } else {
                                checked.remove(path);
                            }
                        }
                        ui.add_sized(
                            [name_w, ROW_H],
                            egui::Label::new(RichText::new(elide(&name, (name_w / 7.5) as usize)).strong()),
                        )
                        .on_hover_text(&f.rel);
                        ui.add_sized(
                            [rel_w, ROW_H],
                            egui::Label::new(muted(elide(&f.rel, (rel_w / 6.5) as usize))),
                        );
                        ui.add_sized([90.0, ROW_H], egui::Label::new(RichText::new(format_bytes(f.size)).monospace()));
                        ui.add_sized([130.0, ROW_H], egui::Label::new(muted(fmt_time(f.modified))));

                        let current = staged.and_then(|m| m.target_queue.clone());
                        let shown = match (&staged, &current) {
                            (None, _) => "Queue…".to_string(),
                            (Some(_), Some(q)) => q.clone(),
                            (Some(_), None) => "Queued (no dest.)".to_string(),
                        };
                        egui::ComboBox::from_id_salt(("files_row_queue", idx))
                            .selected_text(shown)
                            .width(150.0)
                            .show_ui(ui, |ui| {
                                if names.is_empty() {
                                    ui.label(muted("No queues yet. Create one from the Queue view."));
                                }
                                for n in &names {
                                    if ui.selectable_label(current.as_deref() == Some(n.as_str()), n).clicked() {
                                        row_pick = Some((path.clone(), n.clone()));
                                    }
                                }
                            });
                    });
                }
            });
    }

    if let Some((path, queue)) = row_pick {
        app.explorer_files_stage(&[path], Some(queue));
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
