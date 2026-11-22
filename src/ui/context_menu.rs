//! Right-click context menu handler.

use eframe::egui;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuAction {
    AddToQueue(usize),
    MoveToTrash,
    CopyPath,
    OpenInFinder,
    ShowInPreview,
    RemoveFromQueue,
}

pub fn show_file_context_menu(
    ui: &mut egui::Ui,
    path: &PathBuf,
    queue_count: usize,
) -> Option<ContextMenuAction> {
    let mut action = None;

    if ui.button("📋 Add to Queue").clicked() {
        if queue_count > 0 {
            action = Some(ContextMenuAction::AddToQueue(0));
        }
        ui.close_menu();
    }

    if ui.button("🗑 Move to Trash").clicked() {
        action = Some(ContextMenuAction::MoveToTrash);
        ui.close_menu();
    }

    if ui.button("📋 Copy Path").clicked() {
        ui.output_mut(|o| o.copied_text = path.display().to_string());
        action = Some(ContextMenuAction::CopyPath);
        ui.close_menu();
    }

    if ui.button("👁 Preview").clicked() {
        action = Some(ContextMenuAction::ShowInPreview);
        ui.close_menu();
    }

    #[cfg(target_os = "macos")]
    {
        if ui.button("🔍 Show in Finder").clicked() {
            action = Some(ContextMenuAction::OpenInFinder);
            ui.close_menu();
        }
    }

    action
}

pub fn show_queue_context_menu(
    ui: &mut egui::Ui,
    queue_name: &str,
) -> Option<&'static str> {
    if ui.button("✏ Rename Queue").clicked() {
        ui.close_menu();
        return Some("rename");
    }

    if ui.button("🗑 Delete Queue").clicked() {
        ui.close_menu();
        return Some("delete");
    }

    if ui.button("📁 Change Target").clicked() {
        ui.close_menu();
        return Some("change_target");
    }

    None
}
