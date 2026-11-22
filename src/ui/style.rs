//! Visual design system: colour tokens (light default, dark option), spacing,
//! reusable frames and widgets. Colours are read from `pal()`; never hard-code RGB in panels.
use eframe::egui::{self, Color32, Frame, Margin, RichText, Rounding, Stroke, Vec2};
use sortwise::types::ActionMode;
use std::sync::atomic::{AtomicBool, Ordering};

/// One set of colour tokens. Light and dark share the same roles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub canvas: Color32,
    pub surface: Color32,
    pub surface_raised: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    /// Text and icons placed on a filled accent, danger or success background.
    pub on_accent: Color32,
    /// Navigation sidebar background.
    pub sidebar: Color32,
    /// Hover fill for rows, nav items and ghost buttons.
    pub hover: Color32,

    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_on_soft: Color32,

    pub danger: Color32,
    pub danger_soft: Color32,
    pub danger_on_soft: Color32,

    pub success: Color32,
    pub success_soft: Color32,
    pub success_on_soft: Color32,

    pub warning: Color32,
    pub warning_soft: Color32,
    pub warning_on_soft: Color32,
}

pub const R_SM: f32 = 6.0;
pub const R_MD: f32 = 8.0;
pub const R_LG: f32 = 12.0;
pub const H_CONTROL: f32 = 30.0;
pub const H_TOOLBAR: f32 = 48.0;
/// Below this window width the nav sidebar becomes an icon rail and two-column pages stack.
pub const NARROW_W: f32 = 900.0;

/// `true` when the window is narrow enough to switch to the compact layout.
pub fn is_narrow(ctx: &egui::Context) -> bool {
    ctx.screen_rect().width() < NARROW_W
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const LIGHT: Palette = Palette {
    canvas: rgb(245, 245, 247),
    surface: rgb(255, 255, 255),
    surface_raised: rgb(242, 242, 245),
    border: rgb(228, 228, 232),
    border_strong: rgb(201, 201, 209),
    text: rgb(24, 24, 27),
    text_muted: rgb(99, 99, 110),
    on_accent: rgb(255, 255, 255),
    sidebar: rgb(239, 239, 242),
    hover: rgb(233, 233, 238),

    accent: rgb(37, 99, 235),
    accent_soft: rgb(234, 241, 254),
    accent_on_soft: rgb(29, 64, 175),

    danger: rgb(200, 40, 48),
    danger_soft: rgb(253, 236, 236),
    danger_on_soft: rgb(153, 27, 27),

    success: rgb(22, 128, 78),
    success_soft: rgb(226, 246, 236),
    success_on_soft: rgb(21, 94, 60),

    warning: rgb(183, 121, 31),
    warning_soft: rgb(253, 245, 224),
    warning_on_soft: rgb(120, 70, 10),
};

pub const DARK: Palette = Palette {
    canvas: rgb(17, 17, 19),
    surface: rgb(24, 24, 27),
    surface_raised: rgb(34, 34, 38),
    border: rgb(46, 46, 51),
    border_strong: rgb(69, 69, 77),
    text: rgb(237, 237, 240),
    text_muted: rgb(160, 160, 172),
    on_accent: rgb(255, 255, 255),
    sidebar: rgb(20, 20, 22),
    hover: rgb(38, 38, 43),

    accent: rgb(37, 99, 235),
    accent_soft: rgb(28, 39, 64),
    accent_on_soft: rgb(170, 200, 255),

    danger: rgb(200, 40, 48),
    danger_soft: rgb(58, 28, 32),
    danger_on_soft: rgb(255, 180, 180),

    success: rgb(48, 164, 108),
    success_soft: rgb(24, 52, 38),
    success_on_soft: rgb(190, 240, 210),

    warning: rgb(240, 160, 60),
    warning_soft: rgb(72, 52, 20),
    warning_on_soft: rgb(255, 220, 160),
};

/// `true` while the dark palette is active. Light is the default.
static DARK_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Active palette for the current theme.
pub fn pal() -> Palette {
    if DARK_ACTIVE.load(Ordering::Relaxed) { DARK } else { LIGHT }
}

/// Choose the theme before the window exists (used by `--dark`).
pub fn preset_dark(dark: bool) {
    DARK_ACTIVE.store(dark, Ordering::Relaxed);
}

/// `true` when the dark palette is active.
pub fn is_dark() -> bool {
    DARK_ACTIVE.load(Ordering::Relaxed)
}

/// Flip between light and dark from the shell toggle. Reapplies the full style so visuals match.
pub fn toggle_dark(ctx: &egui::Context) {
    preset_dark(!is_dark());
    configure_app_style(ctx);
}

/// Switch theme. Call before the first frame, or from a toggle, then repaint.
pub fn set_dark(ctx: &egui::Context, dark: bool) {
    DARK_ACTIVE.store(dark, Ordering::Relaxed);
    ctx.set_theme(if dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
}

fn visuals(p: Palette, dark: bool) -> egui::Visuals {
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.window_fill = p.surface;
    v.panel_fill = p.canvas;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface_raised;
    v.override_text_color = Some(p.text);
    v.widgets.noninteractive.bg_fill = p.surface;
    v.widgets.noninteractive.weak_bg_fill = p.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.bg_fill = p.surface;
    v.widgets.inactive.weak_bg_fill = p.surface;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border_strong);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.expansion = 0.0;
    v.widgets.active.bg_fill = p.border;
    v.widgets.active.weak_bg_fill = p.border;
    v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.active.expansion = 0.0;
    v.widgets.open.bg_fill = p.surface_raised;
    v.widgets.open.weak_bg_fill = p.surface_raised;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.accent);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = Rounding::same(R_SM);
    }
    v.window_rounding = Rounding::same(R_LG);
    v.menu_rounding = Rounding::same(R_MD);
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 8.0),
        blur: 24.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if dark { 120 } else { 28 }),
    };
    v.popup_shadow = egui::epaint::Shadow {
        offset: Vec2::new(0.0, 4.0),
        blur: 12.0,
        spread: 0.0,
        color: Color32::from_black_alpha(if dark { 100 } else { 22 }),
    };
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.striped = false;
    v.hyperlink_color = p.accent_on_soft;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v
}

pub fn configure_app_style(ctx: &egui::Context) {
    // Light is the default. Force the theme so styling always applies on any OS setting.
    set_dark(ctx, DARK_ACTIVE.load(Ordering::Relaxed));
    ctx.set_visuals_of(egui::Theme::Light, visuals(LIGHT, false));
    ctx.set_visuals_of(egui::Theme::Dark, visuals(DARK, true));

    // Setup native system emoji fallbacks
    let mut fonts = egui::FontDefinitions::default();
    // Phosphor icon font (see `ui::icons`).
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

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

    // Arrows (← → ↑ ↓) and small triangles (▲ ▼ ▾) are not in egui's proportional fonts, so
    // text showed them as boxes. Hack, already bundled, has them: use it as a fallback.
    if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        vec.push("Hack".to_owned());
    }

    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|s| {
        use egui::{FontFamily, FontId, TextStyle};
        s.text_styles = [
            (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        ]
        .into();
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(12.0, 5.0);
        s.spacing.interact_size = Vec2::new(30.0, H_CONTROL - 6.0);
        s.spacing.window_margin = Margin::same(16.0);
        s.spacing.menu_margin = Margin::same(6.0);
        s.spacing.combo_height = 480.0;
        s.spacing.text_edit_width = 220.0;
        s.spacing.scroll.bar_width = 8.0;
        s.spacing.scroll.floating = true;
    });
}

pub fn section_title(text: &str) -> RichText {
    RichText::new(text).strong().size(14.0).color(pal().text)
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).size(12.0).color(pal().text_muted)
}

/// Uppercase group label used in the sidebar and section headers.
pub fn eyebrow(text: &str) -> RichText {
    RichText::new(text.to_uppercase()).size(10.5).strong().color(pal().text_muted)
}

fn card_frame(p: Palette) -> Frame {
    Frame::none()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .rounding(Rounding::same(R_MD))
        .inner_margin(Margin::same(16.0))
}

#[allow(dead_code)]
pub fn card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_frame(pal()).show(ui, add_contents).inner
}

pub fn panel_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_frame(pal()).inner_margin(Margin::same(14.0)).show(ui, add_contents).inner
}

pub fn stat_chip(ui: &mut egui::Ui, label: &str, value: &str) {
    let p = pal();
    Frame::none()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .rounding(Rounding::same(R_SM))
        .inner_margin(Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(value).strong().size(13.0).color(p.text));
                ui.label(RichText::new(label).size(11.5).color(p.text_muted));
            });
        });
}

pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = pal();
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(p.on_accent))
            .fill(p.accent)
            .stroke(Stroke::NONE)
            .rounding(Rounding::same(R_SM))
            .min_size(Vec2::new(0.0, H_CONTROL)),
    )
}

pub fn secondary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = pal();
    ui.add(
        egui::Button::new(RichText::new(text).color(p.text))
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border_strong))
            .rounding(Rounding::same(R_SM))
            .min_size(Vec2::new(0.0, H_CONTROL)),
    )
}

/// Borderless button that only shows a fill on hover (toolbar actions).
#[allow(dead_code)]
pub fn ghost_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = pal();
    ui.add(
        egui::Button::new(RichText::new(text).color(p.text))
            .frame(false)
            .min_size(Vec2::new(0.0, H_CONTROL)),
    )
}

pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = pal();
    ui.add(
        egui::Button::new(RichText::new(text).strong().color(p.on_accent))
            .fill(p.danger)
            .stroke(Stroke::NONE)
            .rounding(Rounding::same(R_SM))
            .min_size(Vec2::new(0.0, H_CONTROL)),
    )
}

/// Filter chip: one line, accent tint when selected.
pub fn chip(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let p = pal();
    let (fg, bg, stroke) = if selected {
        (p.accent_on_soft, p.accent_soft, Stroke::new(1.0, p.accent))
    } else {
        (p.text, p.surface, Stroke::new(1.0, p.border))
    };
    ui.add(
        egui::Button::new(RichText::new(text).size(12.0).color(fg))
            .fill(bg)
            .stroke(stroke)
            .rounding(Rounding::same(R_SM))
            .wrap_mode(egui::TextWrapMode::Extend)
            .min_size(Vec2::new(0.0, 26.0)),
    )
}

/// Small rounded status pill (e.g. KEEP / DUP badges in the copies list).
pub fn pill(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    Frame::none()
        .fill(bg)
        .rounding(Rounding::same(R_SM))
        .inner_margin(Margin::symmetric(7.0, 2.0))
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
    let p = pal();
    let (text, fg, bg) = match variant {
        PillVariant::Keep => ("KEEP", p.success_on_soft, p.success_soft),
        PillVariant::Dup => ("DUP", p.warning_on_soft, p.warning_soft),
        PillVariant::Warning => ("WARN", p.warning_on_soft, p.warning_soft),
        PillVariant::Info => ("INFO", p.accent_on_soft, p.accent_soft),
    };
    pill(ui, text, fg, bg);
}

pub fn path_monospace(path: &std::path::Path) -> RichText {
    RichText::new(path.display().to_string())
        .font(egui::FontId::monospace(11.5))
        .color(pal().accent_on_soft)
}

pub fn folder_chip(ui: &mut egui::Ui, folder_name: &str) -> egui::Response {
    let p = pal();
    Frame::none()
        .fill(p.surface_raised)
        .stroke(Stroke::new(1.0, p.border))
        .rounding(Rounding::same(5.0))
        .inner_margin(Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("\u{E24A}").size(11.0));
                ui.label(RichText::new(folder_name).strong().size(11.5).color(p.text));
            });
        })
        .response
}

pub fn overlap_heat_row(ui: &mut egui::Ui, folder_a: &str, folder_b: &str, overlap_pct: f32, shared_groups: usize) {
    let p = pal();
    ui.horizontal(|ui| {
        let label = format!("{} ↔ {}", folder_a, folder_b);
        ui.allocate_ui(Vec2::new(ui.available_width() - 110.0, 20.0), |ui| {
            ui.label(RichText::new(label).size(11.0).color(p.text_muted));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("({shared_groups})")).size(10.5).color(p.text_muted));

            // Overlap pill: danger for near-identical folders, warning for partial, plain for low.
            let (fg, bg) = if overlap_pct >= 80.0 {
                (p.danger_on_soft, p.danger_soft)
            } else if overlap_pct >= 40.0 {
                (p.warning_on_soft, p.warning_soft)
            } else {
                (p.success_on_soft, p.success_soft)
            };
            pill(ui, &format!("{:.0}%", overlap_pct), fg, bg);
        });
    });
}

pub fn icon_button(ui: &mut egui::Ui, icon: &str, active: bool) -> egui::Response {
    let p = pal();
    let (fg, bg) = if active { (p.accent_on_soft, p.accent_soft) } else { (p.text, Color32::TRANSPARENT) };
    ui.add(
        egui::Button::new(RichText::new(icon).size(15.0).color(fg))
            .fill(bg)
            .stroke(Stroke::NONE)
            .rounding(Rounding::same(R_SM))
            .min_size(Vec2::new(H_CONTROL, H_CONTROL)),
    )
}

pub fn mode_pill(ui: &mut egui::Ui, mode: ActionMode) {
    let p = pal();
    let (text, fg, bg) = match mode {
        ActionMode::DryRun => ("DRY RUN", p.accent_on_soft, p.accent_soft),
        ActionMode::Delete => ("TRASH MODE", p.danger_on_soft, p.danger_soft),
        ActionMode::HardLink => ("LINK MODE", p.success_on_soft, p.success_soft),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance of an sRGB colour.
    fn luminance(c: Color32) -> f64 {
        let lin = |v: u8| {
            let s = v as f64 / 255.0;
            if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn text_tokens_meet_wcag_aa_on_every_surface() {
        for (name, p) in [("light", LIGHT), ("dark", DARK)] {
            for bg in [p.canvas, p.surface, p.surface_raised] {
                assert!(contrast(p.text, bg) >= 4.5, "{name}: text on {bg:?}");
                assert!(contrast(p.text_muted, bg) >= 4.5, "{name}: text_muted on {bg:?}");
            }
            assert!(contrast(p.on_accent, p.accent) >= 4.5, "{name}: on_accent on accent");
            assert!(contrast(p.on_accent, p.danger) >= 4.5, "{name}: on_accent on danger");
            assert!(contrast(p.accent, p.surface) >= 3.0, "{name}: accent as UI graphic");
        }
    }

    #[test]
    fn pill_text_meets_aa_on_its_tint() {
        for (name, p) in [("light", LIGHT), ("dark", DARK)] {
            for (fg, bg, role) in [
                (p.accent_on_soft, p.accent_soft, "accent"),
                (p.danger_on_soft, p.danger_soft, "danger"),
                (p.success_on_soft, p.success_soft, "success"),
                (p.warning_on_soft, p.warning_soft, "warning"),
            ] {
                assert!(contrast(fg, bg) >= 4.5, "{name}: {role} pill text on tint");
            }
        }
    }
}
