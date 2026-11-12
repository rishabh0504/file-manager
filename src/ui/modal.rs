//! One modal template for every dialog (UI-Redesign-Plan §5).
//!
//! Layout: 48 px header (title, close), body, 56 px footer.
//! The footer lays out right to left: add the primary action first, then the secondary,
//! so the secondary ends up on the left.
//! Esc and the close icon both report `closed`; the caller decides what closing means.

use crate::ui::icons::Icons;
use crate::ui::style::{icon_button, pal};
use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, Rounding, ScrollArea, Stroke, Vec2};

const HEADER_H: f32 = 48.0;
const FOOTER_H: f32 = 56.0;
const PADDING: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalSize {
    /// Confirm dialogs: 460 px wide, height follows the content (capped).
    Narrow,
    /// Simple dialogs: 560 × 420, fixed.
    Standard,
    /// Previews and comparisons: 880 × 600, fixed (clamped to 92% of the screen).
    Wide,
    /// Folder contents and other browse-heavy views: 80% × 86% of the screen, never below 880 × 600
    /// and never past 92%. Scales with large displays instead of opening tiny.
    Large,
    /// Media player: 96% of the window, so the picture gets the room. Pair with `show_bare`.
    Screen,
}

/// Show a modal whose body scrolls inside the modal. Returns `true` when closed this frame.
pub fn show(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: ModalSize,
    add_body: impl FnOnce(&mut egui::Ui),
    add_footer: impl FnOnce(&mut egui::Ui),
) -> bool {
    show_inner(ctx, id, title, size, true, true, add_body, Some(add_footer))
}

/// Like `show`, but the body is not wrapped in a scroll area. Use when the body already scrolls
/// (for example a grid) or lays itself out to the available height.
pub fn show_fill(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: ModalSize,
    add_body: impl FnOnce(&mut egui::Ui),
    add_footer: impl FnOnce(&mut egui::Ui),
) -> bool {
    show_inner(ctx, id, title, size, false, true, add_body, Some(add_footer))
}

/// No header bar and no footer: the body owns the whole window and draws its own toolbar.
/// Used by the media player, where a second header only took space from the picture.
pub fn show_bare(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: ModalSize,
    add_body: impl FnOnce(&mut egui::Ui),
) -> bool {
    show_inner(ctx, id, title, size, false, false, add_body, None::<fn(&mut egui::Ui)>)
}

fn show_inner(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    size: ModalSize,
    scroll_body: bool,
    header: bool,
    add_body: impl FnOnce(&mut egui::Ui),
    add_footer: Option<impl FnOnce(&mut egui::Ui)>,
) -> bool {
    let p = pal();
    let footer_h = if add_footer.is_some() { FOOTER_H } else { 0.0 };
    let mut closed = ctx.input(|i| i.key_pressed(egui::Key::Escape));

    // Scrim: dims the page and blocks clicks to it.
    egui::Area::new(egui::Id::new((id, "scrim")))
        .order(egui::Order::Middle)
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(true)
        .show(ctx, |ui| {
            let screen = ui.ctx().screen_rect();
            ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(102));
        });

    let frame = Frame::none()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .rounding(Rounding::same(10.0))
        .inner_margin(Margin::ZERO);

    let mut window = egui::Window::new(title)
        .id(egui::Id::new((id, "window")))
        .title_bar(false)
        .collapsible(false)
        .frame(frame)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO);
    window = match size {
        ModalSize::Narrow => window.resizable(false).default_width(460.0),
        ModalSize::Standard => window.resizable(false).fixed_size([560.0, 420.0]),
        ModalSize::Wide => {
            // Fixed, not resizable: content that wants more width would otherwise stretch the window.
            let screen = ctx.screen_rect().size();
            window.resizable(false).fixed_size([(880.0f32).min(screen.x * 0.92), (600.0f32).min(screen.y * 0.92)])
        }
        ModalSize::Large => {
            let screen = ctx.screen_rect().size();
            let w = (screen.x * 0.8).max(880.0).min(screen.x * 0.92);
            let h = (screen.y * 0.86).max(600.0).min(screen.y * 0.92);
            window.resizable(false).fixed_size([w, h])
        }
        ModalSize::Screen => {
            let screen = ctx.screen_rect().size();
            window.resizable(false).fixed_size([screen.x * 0.96, screen.y * 0.96])
        }
    };

    window.show(ctx, |ui| {
        let width = ui.available_width();
        ui.set_max_width(width);
        let body_max = match size {
            ModalSize::Narrow => 480.0,
            _ => f32::INFINITY,
        };

        if header {
            ui.allocate_ui_with_layout(Vec2::new(width, HEADER_H), Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(PADDING);
                ui.label(egui::RichText::new(title).strong().size(16.0).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(8.0);
                    if icon_button(ui, Icons::CLOSE, false).on_hover_text("Close (Esc)").clicked() {
                        closed = true;
                    }
                });
            });
            ui.separator();
        }

        // Body
        let body_h = (ui.available_height() - footer_h).clamp(0.0, body_max);
        // Bare windows (no header) run edge to edge, so the content gets every pixel.
        let pad = if header { PADDING } else { 0.0 };
        Frame::none().inner_margin(Margin::same(pad)).show(ui, |ui| {
            if scroll_body {
                ScrollArea::vertical()
                    .max_height(body_h - 2.0 * pad)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_min_width(width - 2.0 * pad);
                        add_body(ui);
                    });
            } else {
                // Fixed region, not min_size: a min size would grow the window past the screen.
                ui.allocate_ui(Vec2::new(width - 2.0 * pad, (body_h - 2.0 * pad).max(0.0)), |ui| {
                    ui.set_width(width - 2.0 * pad);
                    add_body(ui);
                });
            }
        });

        // Footer
        if let Some(add_footer) = add_footer {
            ui.separator();
            ui.allocate_ui_with_layout(Vec2::new(width, FOOTER_H), Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(PADDING);
                add_footer(ui);
            });
        }
    });

    closed
}
