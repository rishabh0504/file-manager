use crate::app::{ScanWorkKind, SmartFileManagerApp};
use crate::ui::style::{muted, panel_card, section_title};
use eframe::egui::{self, scroll_area::ScrollBarVisibility, ScrollArea};
use smart_file_manager::types::{FolderStatus, ProgressPhase};

pub fn progress_panel(app: &mut SmartFileManagerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ctx.request_repaint_after(std::time::Duration::from_millis(150));

    let prog = app.progress.lock().unwrap().clone();

    let body_h = ui.available_height().max(120.0);
    ScrollArea::vertical()
        .max_height(body_h)
        .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, false])
        .id_salt("progress_panel_main")
        .show(ui, |ui| {
            let title = match app.scan_work {
                ScanWorkKind::FullScan => "Scan in progress",
                ScanWorkKind::RefreshFindings => "Refreshing duplicate findings",
            };
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(title)
                        .strong()
                        .size(20.0)
                        .color(egui::Color32::from_rgb(236, 239, 252)),
                );
            });
            ui.add_space(12.0);

            panel_card(ui, |ui| {
        match &prog.phase {
            ProgressPhase::Enumerating {
                files_found,
                current_path,
            } => {
                ui.label(egui::RichText::new("Enumerating").strong());
                ui.add_space(4.0);
                ui.label(format!("Files discovered: {files_found}"));
                if !current_path.as_os_str().is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(current_path.display().to_string())
                            .font(egui::FontId::monospace(11.0))
                            .color(egui::Color32::from_rgb(160, 175, 220)),
                    );
                }
            }
            ProgressPhase::Processing {
                stage_label,
                stage,
                stages_total,
            } => {
                ui.label(egui::RichText::new("Hashing pipeline").strong());
                ui.add_space(4.0);
                ui.label(format!(
                    "Stage {}/{} — {}",
                    stage, stages_total, stage_label
                ));
                let pct = if prog.bytes_total > 0 {
                    prog.bytes_processed as f32 / prog.bytes_total as f32
                } else {
                    0.0
                };
                ui.add_space(8.0);
                ui.add(
                    egui::ProgressBar::new(pct.clamp(0.0, 1.0))
                        .show_percentage()
                        .animate(true),
                );
                ui.add_space(6.0);
                ui.label(muted(format!(
                    "{} / {} bytes · {} / {} files hashed · {} groups",
                    prog.bytes_processed,
                    prog.bytes_total,
                    prog.files_hashed,
                    prog.files_to_hash_total,
                    prog.groups_found
                )));
                if !prog.current_path.as_os_str().is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(prog.current_path.display().to_string())
                            .font(egui::FontId::monospace(11.0)),
                    );
                }
            }
            ProgressPhase::Finishing => {
                ui.spinner();
                ui.label("Finishing…");
            }
        }
    });

            if !prog.folders.is_empty() {
                ui.add_space(10.0);
                panel_card(ui, |ui| {
                    ui.label(section_title("Roots"));
                    for f in &prog.folders {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(f.folder_path.display().to_string())
                                    .font(egui::FontId::monospace(11.0)),
                            );
                            match &f.status {
                                FolderStatus::Error(e) => {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(255, 140, 140),
                                        format!("Error: {e}"),
                                    );
                                }
                                FolderStatus::Queued => {
                                    ui.label(muted("Queued"));
                                }
                                FolderStatus::Scanning => {
                                    ui.label(muted("Scanning…"));
                                }
                                FolderStatus::Hashing => {
                                    ui.label(muted("Hashing…"));
                                }
                                FolderStatus::Done { groups_found } => {
                                    ui.label(muted(format!("Done ({groups_found} groups)")));
                                }
                                FolderStatus::Cancelled => {
                                    ui.label(muted("Cancelled"));
                                }
                            }
                        });
                    }
                });
            }

            ui.add_space(16.0);
            let cancel_label = match app.scan_work {
                ScanWorkKind::FullScan => "Cancel scan",
                ScanWorkKind::RefreshFindings => "Cancel refresh",
            };
            if ui
                .button(
                    egui::RichText::new(cancel_label).color(egui::Color32::from_rgb(255, 160, 160)),
                )
                .clicked()
            {
                app.token.cancel();
            }
        });
}
