//! App shell: navigation sidebar (modes + places) and the top toolbar.
use crate::app::{AppMode, AppView, SortwiseApp};
use crate::ui::icons::Icons;
use crate::ui::style::{self, pal};
use eframe::egui::{self, RichText, Rounding, Sense, Stroke, Vec2};

/// Nav sidebar width when expanded / as an icon rail.
pub const NAV_W: f32 = 216.0;
pub const RAIL_W: f32 = 56.0;

pub fn page_title(mode: AppMode, view: AppView) -> &'static str {
    match (mode, view) {
        (AppMode::Explorer, _) => "Explorer",
        (AppMode::AllFiles, _) => "All files",
        (AppMode::Dedupe, AppView::Results) => "Duplicate results",
        (AppMode::Dedupe, _) => "Duplicates",
        (AppMode::CompareSourceDest, AppView::Results) => "Compare results",
        (AppMode::CompareSourceDest, _) => "Compare A → B",
        (AppMode::EmptyFolders, _) => "Empty folders",
        (AppMode::ArchiveFiles, _) => "Archives",
    }
}

fn nav_item(ui: &mut egui::Ui, icon: &str, label: &str, active: bool, rail: bool, tooltip: &str) -> bool {
    let p = pal();
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::click());
    let resp = resp.on_hover_text(tooltip);
    let fill = if active {
        p.accent_soft
    } else if resp.hovered() {
        p.hover
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, Rounding::same(style::R_SM), fill);
    let fg = if active { p.accent_on_soft } else { p.text };
    let icon_fg = if active { p.accent } else { p.text_muted };
    if rail {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(17.0),
            icon_fg,
        );
    } else {
        ui.painter().text(
            egui::pos2(rect.left() + 12.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(16.0),
            icon_fg,
        );
        ui.painter().text(
            egui::pos2(rect.left() + 38.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.0),
            fg,
        );
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn group_label(ui: &mut egui::Ui, text: &str, rail: bool) {
    if rail {
        ui.add_space(6.0);
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
        ui.painter().hline(r.x_range().shrink(10.0), r.center().y, Stroke::new(1.0, pal().border));
        ui.add_space(6.0);
    } else {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(style::eyebrow(text));
        });
        ui.add_space(2.0);
    }
}

/// Left navigation: brand, the six modes grouped, then Places in Explorer.
/// Each mode keeps exactly the side effects the old header tabs had.
pub fn nav(app: &mut SortwiseApp, ui: &mut egui::Ui, rail: bool) {
    ui.spacing_mut().item_spacing.y = 2.0;

    // Brand
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if !rail {
            ui.add_space(10.0);
        } else {
            ui.add_space((ui.available_width() - 24.0).max(0.0) / 2.0);
        }
        if app.brand_logo.is_none() {
            app.brand_logo = crate::ui::brand::load_logo(ui.ctx());
        }
        match app.brand_logo.as_ref() {
            Some(logo) => {
                ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(24.0, 24.0)));
            }
            None => {
                ui.label(RichText::new(Icons::FOLDER_OPEN).size(20.0));
            }
        }
        if !rail {
            ui.add_space(4.0);
            ui.label(RichText::new("Sortwise").strong().size(15.0).color(pal().text));
        }
    });
    ui.add_space(6.0);

    let mode = app.mode;

    group_label(ui, "Browse", rail);
    if nav_item(ui, Icons::FOLDER_OPEN, "Explorer", mode == AppMode::Explorer, rail, "Browse files") {
        app.mode = AppMode::Explorer;
    }
    if nav_item(ui, Icons::DOCUMENT, "All files", mode == AppMode::AllFiles, rail, "All files under a folder, recursive, largest first") {
        app.mode = AppMode::AllFiles;
        app.view = AppView::Scan;
        if app.files_view_dir.is_none() {
            app.files_view_dir = Some(app.explorer_dir.clone());
        }
    }

    group_label(ui, "Clean up", rail);
    // Switching to Duplicates never starts a scan. The user picks folders first.
    if nav_item(ui, Icons::SEARCH, "Duplicates", mode == AppMode::Dedupe, rail, "Find duplicate files") {
        app.mode = AppMode::Dedupe;
    }
    if nav_item(ui, Icons::COMPARE, "Compare A → B", mode == AppMode::CompareSourceDest, rail, "Find copies in B that also exist in A") {
        app.mode = AppMode::CompareSourceDest;
        app.view = AppView::Scan;
    }
    if nav_item(ui, Icons::FOLDER, "Empty folders", mode == AppMode::EmptyFolders, rail, "Find empty folders") {
        app.mode = AppMode::EmptyFolders;
        app.view = AppView::Scan;
    }
    if nav_item(ui, Icons::PACKAGE, "Archives", mode == AppMode::ArchiveFiles, rail, "Find archive files (zip, rar, 7z…)") {
        app.mode = AppMode::ArchiveFiles;
        app.view = AppView::Scan;
    }

    if app.mode == AppMode::Explorer {
        group_label(ui, "", true);
        egui::ScrollArea::vertical()
            .id_salt("nav_places")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                crate::ui::explorer_sidebar(app, ui);
            });
    }
}

/// Top toolbar: page title on the left, Staging on the right (Explorer and All files only).
pub fn toolbar(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    let mode = app.mode;
    ui.horizontal_centered(|ui| {
        ui.label(RichText::new(page_title(mode, app.view)).strong().size(16.0).color(pal().text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if matches!(mode, AppMode::Explorer | AppMode::AllFiles) {
                staging_button(app, ui);
            }
        });
    });
}

/// One entry point for everything set aside: loose items and folder queues.
fn staging_button(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    let p = pal();
    let movable_count = app.movable_items.len();
    let staged_folder: usize = app.explorer_queues.iter().map(|q| q.items.len()).sum();
    let staged_total = movable_count + staged_folder;
    let open = app.movable_queue_open || app.explorer_queue_view_open;
    let narrow = style::is_narrow(ui.ctx());
    let label = match (narrow, staged_total > 0) {
        (true, true) => format!("{}  {staged_total}", Icons::INBOX),
        (true, false) => Icons::INBOX.to_string(),
        (false, true) => format!("{}  Staging  {staged_total}", Icons::INBOX),
        (false, false) => format!("{}  Staging", Icons::INBOX),
    };
    let (fill, text_color, stroke) = if open {
        (p.accent, p.on_accent, Stroke::NONE)
    } else if staged_total > 0 {
        (p.accent_soft, p.accent_on_soft, Stroke::new(1.0, p.accent))
    } else {
        (p.surface, p.text, Stroke::new(1.0, p.border_strong))
    };
    let btn = egui::Button::new(RichText::new(label).strong().size(13.0).color(text_color))
        .fill(fill)
        .stroke(stroke)
        .rounding(Rounding::same(style::R_SM))
        .min_size(egui::vec2(0.0, style::H_CONTROL));
    if ui
        .add(btn)
        .on_hover_text("Files set aside before moving: loose items and folder queues")
        .clicked()
    {
        if open {
            app.movable_queue_open = false;
            app.explorer_queue_view_open = false;
        } else {
            // Opens on Folder queues, the first tab.
            app.explorer_queue_view_open = true;
        }
    }
}
