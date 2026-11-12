use crate::app::{SortwiseApp, AppView, PendingAction};
use crate::ui::style::{panel_card, secondary_button, danger_button, muted};
use eframe::egui::{self, RichText};
use crate::ui::scan_panel::{launch_card, render_folder_box, scan_columns, scan_header, scan_scroll};
use crate::ui::result_list::{count_label, list_card, list_header, list_row, selection_toolbar};
use sortwise::action_plan;

pub fn empty_folders_scan_panel(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    scan_scroll(ui, "empty_folders_scan_panel_main", |ui, col_h| {
        scan_header(
            ui,
            crate::ui::icons::Icons::FOLDER,
            "Find Empty Folders",
            "Finds folders that hold nothing, searched recursively inside each folder you add.",
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
                    "Empty folders are searched inside these",
                    &mut app.roots,
                    "+ Add Folder",
                    crate::ui::style::pal().accent,
                    crate::ui::style::pal().accent_soft,
                    "empty_roots_list",
                );
            },
            |app, ui| {
                let clicked = launch_card(
                    ui,
                    "\u{E270} Scan",
                    |ui| {
                        ui.label(muted("Looks for folders with nothing in them. Nothing is removed until you confirm."));
                    },
                    "Find empty folders",
                    can_start,
                    "Select at least 1 folder to scan.",
                    "Searches every folder you added, recursively.",
                );
                if clicked {
                    app.start_empty_folders_scan();
                }
            },
        );
    });
}

pub fn empty_folders_progress_panel(app: &mut SortwiseApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let count = *app.empty_folders_progress_count.lock().unwrap();
    let current_path = app.empty_folders_progress_path.lock().unwrap().clone();

    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.add(egui::Spinner::new().size(40.0));
        ui.add_space(16.0);
        ui.label(
            RichText::new("Scanning for Empty Folders...")
                .strong()
                .size(16.0)
                .color(crate::ui::style::pal().text),
        );
        ui.add_space(8.0);
        ui.label(muted(format!("Found {} empty folders so far", count)));
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
        if danger_button(ui, "\u{E4F6} Cancel scan").clicked() {
            app.token.cancel();
        }
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}

pub fn empty_folders_results_panel(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    let total = app.empty_folders_results.len();
    let selected = app.empty_folders_selected.len();
    if list_header(
        ui,
        "Empty Folders Found",
        &format!("Found {} eligible for Trash.", count_label(total, "empty folder", "empty folders")),
    ) {
        app.view = AppView::Scan;
    }

    if app.apply_message.is_some() || app.apply_errors.is_some() {
        panel_card(ui, |ui| {
            if let Some(ref m) = app.apply_message {
                ui.label(m);
            }
            if let Some(ref e) = app.apply_errors {
                ui.colored_label(crate::ui::style::pal().danger, e);
            }
        });
        ui.add_space(10.0);
    }

    let toolbar = selection_toolbar(ui, total, selected, |ui| {
        // Goes through the shared Trash confirm, like every other trash action.
        let trash = egui::Button::new(
            RichText::new(format!("\u{E4A6} Trash {}", count_label(selected, "empty folder", "empty folders")))
                .strong()
                .color(crate::ui::style::pal().on_accent),
        )
        .fill(crate::ui::style::pal().danger)
        .rounding(6.0);
        ui.add_enabled(selected > 0, trash)
            .on_hover_text("Move the checked folders to Trash. You confirm first.")
            .clicked()
    });
    if let Some(all) = toolbar.select_all {
        app.empty_folders_selected = if all {
            app.empty_folders_results.iter().cloned().collect()
        } else {
            std::collections::HashSet::new()
        };
    }
    if toolbar.primary {
        app.pending_action = Some(PendingAction::Trash {
            title: format!("Trash {}", count_label(selected, "empty folder", "empty folders")),
            action: "empty_folders",
            plan: action_plan::for_empty_folders(&app.empty_folders_results, &app.empty_folders_selected),
        });
    }

    let listed = app.empty_folders_results.clone();
    list_card(ui, |ui| {
        if listed.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(muted("No empty folders found in the scanned directories."));
            });
            return;
        }
        for (i, p) in listed.iter().enumerate() {
            let mut checked = app.empty_folders_selected.contains(p);
            let was = checked;
            let open = list_row(ui, p, &mut checked, |ui| secondary_button(ui, "\u{E256} Open").clicked());
            if checked != was {
                if checked {
                    app.empty_folders_selected.insert(p.clone());
                } else {
                    app.empty_folders_selected.remove(p);
                }
            }
            if open {
                crate::ui::preview::open_in_default_app(p);
            }
            if i + 1 < listed.len() {
                ui.separator();
            }
        }
    });
}
