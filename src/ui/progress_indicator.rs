//! Real-time progress indicator with cancel button.

use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub struct ProgressIndicator {
    pub operation_name: String,
    pub current: u64,
    pub total: u64,
    pub started_at: Instant,
    pub cancel_requested: Arc<AtomicBool>,
}

impl ProgressIndicator {
    pub fn new(operation_name: String, total: u64, cancel_requested: Arc<AtomicBool>) -> Self {
        Self {
            operation_name,
            current: 0,
            total: total.max(1),
            started_at: Instant::now(),
            cancel_requested,
        }
    }

    pub fn render(&self, ui: &mut egui::Ui) -> bool {
        ui.vertical(|ui| {
            ui.heading(&self.operation_name);
            
            let progress = self.current as f32 / self.total as f32;
            ui.add(egui::ProgressBar::new(progress).text(format!(
                "{}/{}",
                self.current, self.total
            )));

            let elapsed = self.started_at.elapsed().as_secs_f32();
            let speed = if elapsed > 0.0 { self.current as f32 / elapsed } else { 0.0 };
            let eta_secs = if speed > 0.0 { (self.total as f32 - self.current as f32) / speed } else { 0.0 };

            ui.horizontal(|ui| {
                ui.label(format!("Speed: {:.1} items/sec", speed));
                ui.label(format!("ETA: {:.0}s", eta_secs));
            });

            ui.horizontal(|ui| {
                if ui.button("⏸ Pause").clicked() {
                    self.cancel_requested.store(true, Ordering::SeqCst);
                    return true;
                }
                if ui.button("❌ Cancel").clicked() {
                    self.cancel_requested.store(true, Ordering::SeqCst);
                    return true;
                }
                false
            }).inner
        }).inner
    }

    pub fn should_stop(&self) -> bool {
        self.cancel_requested.load(Ordering::SeqCst)
    }
}
