//! Toast-style notifications for quick user feedback.

use std::time::{Duration, Instant};
use eframe::egui;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    pub created_at: Instant,
    pub duration: Duration,
}

impl Notification {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: NotificationLevel::Info,
            created_at: Instant::now(),
            duration: Duration::from_secs(3),
        }
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: NotificationLevel::Success,
            created_at: Instant::now(),
            duration: Duration::from_secs(3),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: NotificationLevel::Warning,
            created_at: Instant::now(),
            duration: Duration::from_secs(5),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: NotificationLevel::Error,
            created_at: Instant::now(),
            duration: Duration::from_secs(7),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.duration
    }

    pub fn progress(&self) -> f32 {
        let elapsed = self.created_at.elapsed().as_secs_f32();
        let duration = self.duration.as_secs_f32();
        (elapsed / duration).clamp(0.0, 1.0)
    }
}

pub struct NotificationManager {
    notifications: Vec<Notification>,
}

impl NotificationManager {
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
        }
    }

    pub fn add(&mut self, notification: Notification) {
        self.notifications.push(notification);
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.add(Notification::info(message));
    }

    pub fn success(&mut self, message: impl Into<String>) {
        self.add(Notification::success(message));
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        self.add(Notification::warning(message));
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.add(Notification::error(message));
    }

    pub fn update(&mut self) {
        self.notifications.retain(|n| !n.is_expired());
    }

    pub fn render(&mut self, ctx: &egui::Context) {
        self.update();

        // Show notifications in top-right corner
        let screen_rect = ctx.screen_rect();
        let mut y_pos = screen_rect.top() + 10.0;
        let x_pos = screen_rect.right() - 10.0;

        for notification in &self.notifications {
            let icon = match notification.level {
                NotificationLevel::Info => "ℹ",
                NotificationLevel::Success => "✓",
                NotificationLevel::Warning => "⚠",
                NotificationLevel::Error => "✕",
            };

            let color = match notification.level {
                NotificationLevel::Info => egui::Color32::BLUE,
                NotificationLevel::Success => egui::Color32::GREEN,
                NotificationLevel::Warning => egui::Color32::YELLOW,
                NotificationLevel::Error => egui::Color32::RED,
            };

            egui::Area::new(egui::Id::new(&notification.message))
                .fixed_pos(egui::pos2(x_pos - 300.0, y_pos))
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(egui::Color32::from_gray(30))
                        .stroke(egui::Stroke::new(1.0, color))
                        .rounding(egui::Rounding::same(4.0))
                        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(icon).size(14.0).color(color));
                                ui.label(&notification.message);
                            });
                        });
                });

            y_pos += 60.0;
        }
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}
