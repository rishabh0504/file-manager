use crate::app::{SortwiseApp, AppView};
use crate::ui::style::{self, primary_button, secondary_button, muted};
use eframe::egui::{self, RichText};
use crate::ui::scan_panel::{launch_card, render_folder_box, scan_columns, scan_header, scan_scroll};
use crate::ui::result_list::{count_label, list_card, list_header, list_row};

pub fn archive_files_scan_panel(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    scan_scroll(ui, "archive_files_scan_panel_main", |ui, col_h| {
        scan_header(
            ui,
            crate::ui::icons::Icons::ARCHIVE,
            "Find Archive Files",
            "Finds .zip, .rar, .7z and similar archives, searched recursively inside each folder you add.",
        );
        let can_start = !app.roots.is_empty();
        scan_columns(
            app,
            ui,
            col_h,
            |app, ui| {
                render_folder_box(
                    ui,
                    "\u{E24A} Folders to Scan",
                    "Archive files are searched inside these",
                    &mut app.roots,
                    "+ Add Folder",
                    crate::ui::style::pal().accent,
                    crate::ui::style::pal().accent_soft,
                    "archive_roots_list",
                );
            },
            |app, ui| {
                let clicked = launch_card(
                    ui,
                    "\u{E270} Scan",
                    |ui| {
                        ui.label(muted("Finds archives only. Choose where to move them after the scan."));
                    },
                    "Find archive files",
                    can_start,
                    "Select at least 1 folder to scan.",
                    "Searches every folder you added, recursively.",
                );
                if clicked {
                    app.start_archive_files_scan();
                }
            },
        );
    });
}

pub fn archive_files_progress_panel(app: &mut SortwiseApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let count = *app.archive_files_progress_count.lock().unwrap();
    let current_path = app.archive_files_progress_path.lock().unwrap().clone();

    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.add(egui::Spinner::new().size(40.0));
        ui.add_space(16.0);
        ui.label(
            RichText::new("Scanning for Archive Files...")
                .strong()
                .size(16.0)
                .color(crate::ui::style::pal().text),
        );
        ui.add_space(8.0);
        ui.label(muted(format!("Found {} archives so far", count)));
        if !current_path.as_os_str().is_empty() {
            ui.add_space(12.0);
            let display_path = if current_path.to_string_lossy().len() > 60 {
                let s = current_path.to_string_lossy();
                format!("...{}", &s[s.len() - 57..])
            } else {
                current_path.to_string_lossy().into_owned()
            };
            ui.label(RichText::new(display_path).font(egui::FontId::monospace(11.0)).color(crate::ui::style::pal().text_muted));
        }

        ui.add_space(24.0);
        if ui.button(RichText::new("\u{E4F6} Cancel scan").strong().color(crate::ui::style::pal().danger_on_soft)).clicked() {
            app.token.cancel();
        }
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

pub fn archive_files_results_panel(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    let total = app.archive_files_results.len();
    let selected = app.archive_files_selected.len();
    if list_header(ui, "Archive Files Found", &format!("Found {} in the scanned folders.", count_label(total, "archive file", "archive files"))) {
        app.view = AppView::Scan;
    }

    // Read everything the card shows before drawing, so no closure needs the app.
    let moving = app.archive_files_moving;
    let (done, move_total, fname) = app
        .archive_files_move_progress
        .lock()
        .map(|g| g.clone())
        .unwrap_or((0, 0, String::new()));
    let target = app.archive_files_move_target.clone();
    let can_move = target.is_some() && selected > 0;
    let why_not = if target.is_none() {
        "Choose a destination folder first."
    } else {
        "Select at least one archive."
    };
    let move_label = format!("\u{E390} Move {}", count_label(selected, "archive", "archives"));

    let mut choose = false;
    let mut select_all: Option<bool> = None;
    let mut move_clicked = false;

    // One toolbar card: destination on top, selection and the single primary action below.
    egui::Frame::none()
        .fill(crate::ui::style::pal().surface)
        .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border))
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Move to").strong().color(crate::ui::style::pal().text));
                match &target {
                    Some(t) => {
                        ui.add(egui::Label::new(style::path_monospace(t)).truncate());
                    }
                    None => {
                        ui.label(muted("No destination chosen"));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    choose = secondary_button(ui, "Choose folder…").clicked();
                });
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let mut all = total > 0 && selected == total;
                if ui.checkbox(&mut all, "Select all").changed() {
                    select_all = Some(all);
                }
                ui.label(muted(format!("({selected} of {total} selected)")));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if moving {
                        ui.add(egui::ProgressBar::new(if move_total > 0 { done as f32 / move_total as f32 } else { 0.0 })
                            .desired_width(160.0)
                            .text(format!("Moving {}/{}", done + 1, move_total.max(1))));
                        if !fname.is_empty() {
                            let shown = if fname.len() > 30 { format!("…{}", &fname[fname.len().saturating_sub(27)..]) } else { fname.clone() };
                            ui.label(muted(shown));
                        }
                    } else {
                        move_clicked = ui
                            .add_enabled_ui(can_move, |ui| primary_button(ui, &move_label))
                            .inner
                            .on_disabled_hover_text(why_not)
                            .clicked();
                    }
                });
            });
        });
    ui.add_space(10.0);

    if choose {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            app.archive_files_move_target = Some(folder);
        }
    }
    if let Some(all) = select_all {
        app.archive_files_selected = if all {
            app.archive_files_results.iter().cloned().collect()
        } else {
            std::collections::HashSet::new()
        };
    }
    if move_clicked {
        app.execute_move_archive_files();
    }

    let listed = app.archive_files_results.clone();
    list_card(ui, |ui| {
        if listed.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(muted("No archive files found in the scanned directories."));
            });
            return;
        }
        for p in listed.iter() {
            let mut checked = app.archive_files_selected.contains(p);
            let was = checked;
            list_row(ui, p, &mut checked, |_| false);
            if checked != was {
                if checked {
                    app.archive_files_selected.insert(p.clone());
                } else {
                    app.archive_files_selected.remove(p);
                }
            }
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
        }
    });
}
