//! Sortwise brand assets. The mark is bundled from `assets/` at build time.

use eframe::egui;

const LOGO_PNG: &[u8] = include_bytes!("../../assets/sortwise-icon.png");

/// Decode the logo into a GPU texture. Returns None (and logs why) if the bundled PNG is unreadable,
/// so the top bar falls back to the folder glyph.
pub fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let icon = match eframe::icon_data::from_png_bytes(LOGO_PNG) {
        Ok(icon) => icon,
        Err(e) => {
            eprintln!("Sortwise: logo not loaded: {e}");
            return None;
        }
    };
    let size = [icon.width as usize, icon.height as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &icon.rgba);
    Some(ctx.load_texture("sortwise-logo", image, egui::TextureOptions::LINEAR))
}
