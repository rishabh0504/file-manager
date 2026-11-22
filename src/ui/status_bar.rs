//! Bottom status bar (UI plan §6.1, Phase 6): location or scan state on the left, counts in the
//! middle, reclaimable size and the error log on the right.

use crate::app::{AppMode, AppView, SortwiseApp};
use crate::ui::preview::format_bytes;
use crate::ui::style::{muted, pal};
use eframe::egui::{self, RichText};

pub fn status_bar(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    let left = left_text(app);
    let middle = middle_text(app);
    let reclaimable = results_reclaimable(app);
    let error_count = app.last_result.as_ref().map_or(0, |r| r.error_log.len());

    ui.horizontal_centered(|ui| {
        ui.label(muted(left));
        if !middle.is_empty() {
            ui.add_space(16.0);
            ui.label(muted(middle));
        }

        // Right to left: theme toggle (rightmost), error log, then reclaimable size.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (icon, tip) = if crate::ui::style::is_dark() {
                (crate::ui::icons::Icons::THEME_TO_LIGHT, "Switch to light theme")
            } else {
                (crate::ui::icons::Icons::THEME_TO_DARK, "Switch to dark theme")
            };
            if ui.add(egui::Button::new(RichText::new(icon).size(13.0)).frame(false)).on_hover_text(tip).clicked() {
                crate::ui::style::toggle_dark(ui.ctx());
            }
            ui.add_space(12.0);
            if error_count > 0 {
                let label = RichText::new(format!("Errors ({error_count})")).size(12.0).color(pal().danger_on_soft);
                if ui.add(egui::Button::new(label).frame(false)).on_hover_text("Open the error log").clicked() {
                    app.show_error_log = true;
                }
                ui.add_space(12.0);
            }
            if let Some(bytes) = reclaimable {
                ui.label(muted(format!("{} reclaimable", format_bytes(bytes))));
            }
        });
    });
}

/// Where the user is, or what the scan is doing.
fn left_text(app: &SortwiseApp) -> String {
    match app.mode {
        AppMode::Explorer => app.explorer_dir.display().to_string(),
        AppMode::Dedupe | AppMode::CompareSourceDest => match app.view {
            AppView::Scanning => "Scanning…".into(),
            AppView::Results => "Duplicate results".into(),
            AppView::Scan => "Ready".into(),
        },
        AppMode::EmptyFolders => match app.view {
            AppView::Scanning => "Scanning for empty folders…".into(),
            _ => "Empty folders".into(),
        },
        AppMode::ArchiveFiles => match app.view {
            AppView::Scanning => "Scanning for archives…".into(),
            _ => "Archives".into(),
        },
        AppMode::AllFiles => match &app.files_view_dir {
            Some(d) => d.display().to_string(),
            None => "Files".into(),
        },
    }
}

/// Item and selection counts for the current screen. Empty when there is nothing to count.
fn middle_text(app: &SortwiseApp) -> String {
    match app.mode {
        AppMode::Explorer => {
            let total = app.explorer_entries.len();
            let folders = app.explorer_entries.iter().filter(|e| e.is_dir).count();
            let selected = app.explorer_entries.iter().filter(|e| app.explorer_multi.contains(&e.path)).count();
            let mut text = format!("{total} items · {folders} folders · {} files", total - folders);
            if selected > 0 {
                text.push_str(&format!(" · {selected} selected"));
            }
            text
        }
        AppMode::Dedupe | AppMode::CompareSourceDest => {
            let selected = app.results_selected_files.len();
            if selected > 0 { format!("{selected} selected") } else { String::new() }
        }
        AppMode::EmptyFolders => {
            let selected = app.empty_folders_selected.len();
            if selected > 0 { format!("{selected} selected") } else { String::new() }
        }
        AppMode::ArchiveFiles => {
            let selected = app.archive_files_selected.len();
            if selected > 0 { format!("{selected} selected") } else { String::new() }
        }
        AppMode::AllFiles => {
            let checked = app.explorer_files_checked.len();
            if checked > 0 { format!("{checked} checked") } else { String::new() }
        }
    }
}

/// Reclaimable bytes of the loaded duplicate results, when Duplicates or Compare is showing them.
fn results_reclaimable(app: &SortwiseApp) -> Option<u64> {
    if !matches!(app.mode, AppMode::Dedupe | AppMode::CompareSourceDest) {
        return None;
    }
    app.last_result.as_ref().map(|r| r.stats.total_wasted_bytes)
}
