//! Recent operations panel for explorer view.

use crate::types::OperationRecord;
use eframe::egui;

pub fn recent_operations_panel(records: &[OperationRecord], ui: &mut egui::Ui) {
    if records.is_empty() {
        return;
    }

    ui.collapsing("📋 Recent Operations", |ui| {
        ui.add_space(4.0);

        // Show last 10 operations
        for record in records.iter().rev().take(10) {
            let status_icon = if record.success { "✅" } else { "❌" };
            
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(status_icon).small());
                ui.label(egui::RichText::new(record.format_time()).weak().small());
                ui.label(record.op_type.to_string());
                ui.separator();
                ui.label(egui::RichText::new(record.summary()).small());
            });
        }

        ui.add_space(2.0);
        
        if records.len() > 10 {
            ui.label(
                egui::RichText::new(format!("... and {} more", records.len() - 10))
                    .weak()
                    .small()
            );
        }
    });
}
