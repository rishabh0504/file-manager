mod app;
mod ui;

use app::SmartFileManagerApp;
use eframe::egui;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 720.0])
            .with_min_inner_size([640.0, 480.0]),
        // The embedded MPV player renders via egui_glow::CallbackFn which
        // requires an active OpenGL context — so we must use the Glow backend.
        #[cfg(feature = "mpv")]
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Smart File Manager",
        native_options,
        Box::new(|cc| {
            crate::ui::style::configure_app_style(&cc.egui_ctx);
            Ok(Box::new(SmartFileManagerApp::default()))
        }),
    )
}
