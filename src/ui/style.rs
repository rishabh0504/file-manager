//! Visual design system: dark theme, spacing, reusable frames and widgets.
use eframe::egui::{self, Color32, Frame, Margin, RichText, Rounding, Stroke, Vec2};
use smart_file_manager::types::ActionMode;

/// Main canvas behind panels (matches `panel_fill` for a single cohesive surface).
pub const CANVAS_FILL: Color32 = Color32::from_rgb(20, 20, 24);

/// Primary accent (used for active nav, primary buttons, step badges).
pub const ACCENT: Color32 = Color32::from_rgb(72, 132, 248);
/// Slightly darker surface for the navigation rail.
pub const NAV_FILL: Color32 = Color32::from_rgb(14, 14, 18);
/// Hairline border between structural panels.
pub const BORDER: Color32 = Color32::from_rgb(38, 38, 48);

// Semantic Color Tokens
pub const DANGER: Color32 = Color32::from_rgb(220, 70, 70);
pub const WARNING: Color32 = Color32::from_rgb(240, 160, 60);

pub const SURFACE_1: Color32 = Color32::from_rgb(26, 26, 34);  // Panel cards
pub const SURFACE_2: Color32 = Color32::from_rgb(34, 34, 44);  // Elevated/selected
pub const SURFACE_3: Color32 = Color32::from_rgb(44, 48, 64);  // Hover states

pub const TEXT_PRI: Color32 = Color32::from_rgb(224, 228, 244);
pub const TEXT_SEC: Color32 = Color32::from_rgb(150, 158, 190);
pub const TEXT_TER: Color32 = Color32::from_rgb(95, 100, 125);
#[allow(dead_code)]
pub const ACCENT_2: Color32 = Color32::from_rgb(110, 80, 240); // Violet gradient end

pub fn configure_app_style(ctx: &egui::Context) {
    // Default is `ThemePreference::System`; on a light OS theme, `set_visuals` would
    // only update the light palette — force dark so our styling always applies.
    ctx.set_theme(egui::ThemePreference::Dark);

    let mut v = egui::Visuals::dark();
    v.window_fill = Color32::from_rgb(18, 18, 22);
    v.panel_fill = CANVAS_FILL;
    v.extreme_bg_color = Color32::from_rgb(10, 10, 14);
    v.faint_bg_color = Color32::from_rgb(32, 32, 40);
    v.widgets.noninteractive.bg_fill = Color32::from_rgb(28, 28, 36);
    v.widgets.inactive.bg_fill = Color32::from_rgb(36, 36, 46);
    v.widgets.hovered.bg_fill = SURFACE_3;
    v.widgets.active.bg_fill = Color32::from_rgb(52, 58, 78);
    v.selection.bg_fill = Color32::from_rgb(45, 85, 145);
    v.hyperlink_color = Color32::from_rgb(140, 195, 255);
    v.warn_fg_color = WARNING;
    v.error_fg_color = DANGER;
    ctx.set_visuals_of(egui::Theme::Dark, v);

    // Setup native system emoji fallbacks
    let mut fonts = egui::FontDefinitions::default();
    
    #[cfg(target_os = "macos")]
    if let Ok(emoji_data) = std::fs::read("/System/Library/Fonts/Apple Color Emoji.ttc") {
        fonts.font_data.insert(
            "AppleColorEmoji".to_owned(),
            egui::FontData::from_owned(emoji_data),
        );
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            vec.push("AppleColorEmoji".to_owned());
        }
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            vec.push("AppleColorEmoji".to_owned());
        }
    }

    #[cfg(target_os = "windows")]
    if let Ok(emoji_data) = std::fs::read("C:\\Windows\\Fonts\\seguiemj.ttf") {
        fonts.font_data.insert(
            "SegoeUIEmoji".to_owned(),
            egui::FontData::from_owned(emoji_data),
        );
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            vec.push("SegoeUIEmoji".to_owned());
        }
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            vec.push("SegoeUIEmoji".to_owned());
        }
    }
    
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(10.0, 6.0);
        s.spacing.button_padding = Vec2::new(16.0, 8.0);
        s.spacing.window_margin = Margin::same(12.0);
        s.spacing.menu_margin = Margin::same(8.0);
    });
}

pub fn section_title(text: &str) -> RichText {
    RichText::new(text).strong().size(15.0).color(TEXT_PRI)
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).size(12.0).color(TEXT_SEC)
}

#[allow(dead_code)]
pub fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::none()
        .fill(SURFACE_1)
        .stroke(Stroke::new(1.0, Color32::from_rgb(50, 50, 62)))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(14.0))
        .show(ui, add_contents)
        .inner
}

pub fn panel_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    Frame::none()
        .fill(SURFACE_1)
        .stroke(Stroke::new(1.0, BORDER))
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::same(12.0))
        .show(ui, add_contents)
        .inner
}

pub fn stat_chip(ui: &mut egui::Ui, label: &str, value: &str) {
    Frame::none()
        .fill(Color32::from_rgb(32, 40, 60))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(11.0).color(TEXT_SEC));
                ui.label(RichText::new(value).strong().color(TEXT_PRI));
            });
        });
}

pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .strong()
                .color(Color32::from_rgb(248, 250, 255)),
        )
        .fill(ACCENT)
        .min_size(Vec2::new(0.0, 34.0)),
    )
}

pub fn secondary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(TEXT_PRI))
            .fill(Color32::from_rgb(40, 42, 52))
            .stroke(Stroke::new(1.0, Color32::from_rgb(58, 62, 78)))
            .min_size(Vec2::new(0.0, 28.0)),
    )
}

pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .strong()
                .color(Color32::from_rgb(255, 240, 240)),
        )
        .fill(DANGER)
        .min_size(Vec2::new(0.0, 28.0)),
    )
}

/// Small rounded status pill (e.g. KEEP / DUP badges in the copies list).
pub fn pill(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    Frame::none()
        .fill(bg)
        .rounding(Rounding::same(9.0))
        .inner_margin(Margin::symmetric(8.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).strong().color(fg));
        });
}

pub enum PillVariant {
    Keep,
    Dup,
    #[allow(dead_code)]
    Warning,
    #[allow(dead_code)]
    Info,
}

pub fn semantic_pill(ui: &mut egui::Ui, variant: PillVariant) {
    let (text, fg, bg) = match variant {
        PillVariant::Keep => ("KEEP", Color32::from_rgb(210, 255, 226), Color32::from_rgb(32, 85, 55)),
        PillVariant::Dup => ("DUP", Color32::from_rgb(255, 226, 200), Color32::from_rgb(95, 60, 32)),
        PillVariant::Warning => ("WARN", Color32::from_rgb(255, 240, 200), Color32::from_rgb(95, 80, 32)),
        PillVariant::Info => ("INFO", Color32::from_rgb(200, 230, 255), Color32::from_rgb(32, 60, 95)),
    };
    pill(ui, text, fg, bg);
}

pub fn path_monospace(path: &std::path::Path) -> RichText {
    RichText::new(path.display().to_string())
        .font(egui::FontId::monospace(11.5))
        .color(Color32::from_rgb(180, 200, 255))
}

pub fn gradient_badge(ui: &mut egui::Ui, label: &str, value_str: &str, fill_pct: f32) {
    Frame::none()
        .fill(SURFACE_2)
        .stroke(Stroke::new(1.0, Color32::from_rgb(50, 50, 60)))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(8.0, 6.0))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(10.5).color(TEXT_SEC));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(value_str).strong().color(TEXT_PRI));
                    });
                });
                ui.add_space(4.0);
                
                // Progress/fill bar indicator
                let w = ui.available_width().max(60.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 4.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, Rounding::same(2.0), Color32::from_rgb(45, 45, 55));
                
                let fill_w = (w * fill_pct.clamp(0.0, 1.0)) as f32;
                if fill_w > 0.0 {
                    let mut fill_rect = rect;
                    fill_rect.max.x = fill_rect.min.x + fill_w;
                    ui.painter().rect_filled(fill_rect, Rounding::same(2.0), ACCENT);
                }
            });
        });
}

pub fn folder_chip(ui: &mut egui::Ui, folder_name: &str) -> egui::Response {
    Frame::none()
        .fill(Color32::from_rgb(32, 34, 42))
        .stroke(Stroke::new(1.0, Color32::from_rgb(52, 56, 70)))
        .rounding(Rounding::same(5.0))
        .inner_margin(Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("📁").size(11.0));
                ui.label(RichText::new(folder_name).strong().size(11.5).color(TEXT_PRI));
            });
        })
        .response
}

pub fn overlap_heat_row(ui: &mut egui::Ui, folder_a: &str, folder_b: &str, overlap_pct: f32, shared_groups: usize) {
    ui.horizontal(|ui| {
        let label = format!("{} ↔ {}", folder_a, folder_b);
        ui.allocate_ui(Vec2::new(ui.available_width() - 110.0, 20.0), |ui| {
            ui.label(RichText::new(label).size(11.0).color(TEXT_SEC));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("({shared_groups})")).size(10.5).color(TEXT_TER));
            
            // Percentage pill colored by match degree
            let bg = if overlap_pct >= 80.0 {
                Color32::from_rgb(90, 30, 30) // Red-ish alerts
            } else if overlap_pct >= 40.0 {
                Color32::from_rgb(85, 60, 20) // Orange-ish warning
            } else {
                Color32::from_rgb(30, 60, 40) // Green-ish stable
            };
            let fg = if overlap_pct >= 80.0 {
                Color32::from_rgb(255, 180, 180)
            } else if overlap_pct >= 40.0 {
                Color32::from_rgb(255, 220, 160)
            } else {
                Color32::from_rgb(180, 255, 200)
            };
            pill(ui, &format!("{:.0}%", overlap_pct), fg, bg);
        });
    });
}

pub fn icon_button(ui: &mut egui::Ui, icon: &str, active: bool) -> egui::Response {
    let bg = if active { SURFACE_3 } else { Color32::TRANSPARENT };
    ui.add(
        egui::Button::new(RichText::new(icon).size(13.0).color(TEXT_PRI))
            .fill(bg)
            .min_size(Vec2::new(26.0, 26.0))
            .frame(true),
    )
}

pub fn mode_pill(ui: &mut egui::Ui, mode: ActionMode) {
    let (text, fg, bg) = match mode {
        ActionMode::DryRun => ("DRY RUN", Color32::from_rgb(200, 230, 255), Color32::from_rgb(32, 60, 95)),
        ActionMode::Delete => ("DELETE MODE", Color32::from_rgb(255, 180, 180), Color32::from_rgb(105, 30, 30)),
        ActionMode::HardLink => ("LINK MODE", Color32::from_rgb(210, 255, 226), Color32::from_rgb(30, 85, 50)),
    };
    Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0, fg))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::symmetric(10.0, 4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).strong().size(11.0).color(fg));
        });
}
