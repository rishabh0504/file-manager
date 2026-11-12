use crate::app::{ScanWorkKind, SortwiseApp};
use crate::ui::style::{muted, panel_card, section_title};
use eframe::egui::{self, scroll_area::ScrollBarVisibility, ScrollArea};
use sortwise::types::{FolderStatus, ProgressPhase};
use crate::ui::preview::format_bytes;
use std::time::Duration;

pub fn progress_panel(app: &mut SortwiseApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ctx.request_repaint_after(std::time::Duration::from_millis(150));

    let prog = app.progress.lock().unwrap().clone();

    let body_h = ui.available_height().max(120.0);
    ScrollArea::vertical()
        .max_height(body_h)
        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
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
                        .color(crate::ui::style::pal().text),
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
                            .color(crate::ui::style::pal().accent_on_soft),
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
                    "{} of {} · {} of {} files · {} groups so far",
                    format_bytes(prog.bytes_processed),
                    format_bytes(prog.bytes_total),
                    prog.files_hashed,
                    prog.files_to_hash_total,
                    prog.groups_found
                )));
                if let Some(started) = app.scan_started_at {
                    ui.label(muted(timing_line(started.elapsed(), prog.bytes_processed, prog.bytes_total)));
                }
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
                                        crate::ui::style::pal().danger_on_soft,
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
            if !app.live_groups.is_empty() {
                let n = app.live_groups.len();
                if crate::ui::style::secondary_button(ui, &format!("Browse {n} group(s) found so far")).clicked() {
                    app.show_live_results();
                }
                ui.add_space(8.0);
            }
            let cancel_label = match app.scan_work {
                ScanWorkKind::FullScan => "Stop — keep results so far",
                ScanWorkKind::RefreshFindings => "Cancel refresh",
            };
            if ui
                .button(
                    egui::RichText::new(cancel_label).color(crate::ui::style::pal().danger_on_soft),
                )
                .clicked()
            {
                app.token.cancel();
            }
        });
}

/// "Elapsed 1m 05s · 42 MB/s · about 3m left" (U12). Rate and ETA need some progress first.
pub fn timing_line(elapsed: Duration, processed: u64, total: u64) -> String {
    let secs = elapsed.as_secs_f64();
    let mut parts = vec![format!("Elapsed {}", human_duration(secs))];
    if secs >= 1.0 && processed > 0 {
        let rate = processed as f64 / secs;
        parts.push(format!("{}/s", format_bytes(rate as u64)));
        if total > processed {
            let left = (total - processed) as f64 / rate;
            parts.push(format!("about {} left", human_duration(left)));
        }
    }
    parts.join(" · ")
}

fn human_duration(secs: f64) -> String {
    let s = secs.round() as u64;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m {:02}s", s / 60, s % 60),
        _ => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formats_read_naturally() {
        assert_eq!(human_duration(5.0), "5s");
        assert_eq!(human_duration(65.0), "1m 05s");
        assert_eq!(human_duration(3700.0), "1h 01m");
    }

    #[test]
    fn timing_line_hides_eta_before_progress() {
        assert_eq!(timing_line(Duration::from_millis(200), 0, 100), "Elapsed 0s");
        let line = timing_line(Duration::from_secs(10), 10 * 1024 * 1024, 110 * 1024 * 1024);
        assert!(line.contains("1.0 MB/s") || line.contains("MB/s"), "{line}");
        assert!(line.contains("about 1m 40s left"), "{line}");
    }
}
