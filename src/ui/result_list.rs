//! Shared layout for the list results screens (Empty folders, Archives): a header with Back,
//! a selection toolbar with one primary action, and a scrolling list card. Duplicates and
//! Compare keep their own group layout.

use crate::ui::style::{self, muted, secondary_button};
use eframe::egui::{self, scroll_area::ScrollBarVisibility, RichText, ScrollArea};
use std::path::Path;

/// What the user did in the toolbar this frame.
#[derive(Debug, Default)]
pub struct ToolbarAction {
    /// `Some(true)` / `Some(false)` when the select-all box was toggled.
    pub select_all: Option<bool>,
    /// True when the primary button was clicked.
    pub primary: bool,
}

/// Title, one muted summary line, and Back on the right. Returns true when Back was clicked.
pub fn list_header(ui: &mut egui::Ui, title: &str, summary: &str) -> bool {
    let mut back = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).strong().size(22.0).color(style::pal().text));
            ui.add_space(2.0);
            ui.label(muted(summary));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            back = secondary_button(ui, "Back").clicked();
        });
    });
    ui.add_space(14.0);
    back
}

/// Toolbar card above a list: select-all, the count, and the primary action on the right.
/// `right` draws the primary action and returns true when it was clicked. It must not
/// borrow the app (precompute what it shows); the caller applies the result afterwards.
pub fn selection_toolbar(
    ui: &mut egui::Ui,
    total: usize,
    selected: usize,
    right: impl FnOnce(&mut egui::Ui) -> bool,
) -> ToolbarAction {
    let mut action = ToolbarAction::default();
    egui::Frame::none()
        .fill(style::pal().surface)
        .stroke(egui::Stroke::new(1.0, style::pal().border))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut all = total > 0 && selected == total;
                if ui.checkbox(&mut all, "Select all").changed() {
                    action.select_all = Some(all);
                }
                ui.label(muted(format!("({selected} of {total} selected)")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    action.primary = right(ui);
                });
            });
        });
    ui.add_space(10.0);
    action
}

/// Scrolling list card that fills the rest of the window.
pub fn list_card(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(style::pal().surface)
        .stroke(egui::Stroke::new(1.0, style::pal().border_strong))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(ui.available_height())
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, body);
        });
}

/// One list row: checkbox, file or folder name with its parent path under it, and an optional
/// trailing action. Same name-first style as the Duplicates rows. Returns true when the trailing
/// action was clicked.
pub fn list_row(
    ui: &mut egui::Ui,
    path: &Path,
    checked: &mut bool,
    trailing: impl FnOnce(&mut egui::Ui) -> bool,
) -> bool {
    let mut trailing_clicked = false;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let parent = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.checkbox(checked, "");
        ui.vertical(|ui| {
            ui.add(egui::Label::new(RichText::new(name).strong().size(13.0).color(style::pal().text)).truncate());
            ui.add(egui::Label::new(RichText::new(parent).size(11.5).color(style::pal().text_muted)).truncate());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            trailing_clicked = trailing(ui);
        });
    });
    trailing_clicked
}

/// "1 folder" / "3 folders". Keeps button and summary text grammatical.
pub fn count_label(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}
