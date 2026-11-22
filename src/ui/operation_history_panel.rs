//! Operation history display panel.

use crate::types::OperationRecord;
use eframe::egui;

pub fn operation_history_panel(records: &[OperationRecord], ui: &mut egui::Ui) {
    if records.is_empty() {
        ui.label(egui::RichText::new("No operations yet").weak());
        return;
    }

    ui.label(egui::RichText::new(format!("History ({} items)", records.len())).strong());
    ui.add_space(4.0);

    let table = egui_extras::StripBuilder::new(ui)
        .size(egui_extras::Size::exact(30.0))
        .vertical(|mut strip| {
            // Header
            strip.cell(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Time").strong());
                    ui.separator();
                    ui.label(egui::RichText::new("Operation").strong());
                    ui.separator();
                    ui.label(egui::RichText::new("Details").strong());
                });
            });

            ui.separator();

            // Recent operations (last 20)
            for record in records.iter().rev().take(20) {
                strip.cell(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(record.format_time());
                        ui.separator();
                        ui.label(record.op_type.to_string());
                        ui.separator();
                        ui.label(record.summary());
                    });
                });
            }
        });
}

pub fn compact_history_summary(records: &[OperationRecord], ui: &mut egui::Ui) {
    if records.is_empty() {
        return;
    }

    let success_count = records.iter().filter(|r| r.success).count();
    let fail_count = records.len() - success_count;

    ui.horizontal(|ui| {
        ui.label(format!("✅ {} | ❌ {}", success_count, fail_count));
        ui.label(format!("({} total)", records.len()));
    });
}
