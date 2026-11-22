//! Keyboard and context menu integration for explorer panel.

use crate::app::SortwiseApp;
use crate::ui::context_menu::ContextMenuAction;
use crate::ui::keyboard::{handle_key_press, KeyAction};
use crate::operations::{toggle_movable, explorer_add_to_queue};
use eframe::egui;
use crate::ui::preview::{open_in_default_app, reveal_in_folder};
use sortwise::resolver::trash_single_path;
use std::path::{Path, PathBuf};

/// Handle keyboard shortcuts in explorer view.
pub fn handle_explorer_keys(app: &mut SortwiseApp, ctx: &egui::Context) -> Option<ExplorerKeyAction> {
    match handle_key_press(ctx) {
        Some(KeyAction::Delete) => {
            if !app.explorer_selected.is_empty() {
                return Some(ExplorerKeyAction::DeleteSelected);
            }
        }
        Some(KeyAction::Copy) => {
            if !app.explorer_selected.is_empty() {
                return Some(ExplorerKeyAction::CopySelected);
            }
        }
        Some(KeyAction::Cut) => {
            if !app.explorer_selected.is_empty() {
                return Some(ExplorerKeyAction::CutToMovable);
            }
        }
        Some(KeyAction::Paste) => {
            return Some(ExplorerKeyAction::PasteMovable);
        }
        Some(KeyAction::SelectAll) => {
            return Some(ExplorerKeyAction::SelectAll);
        }
        Some(KeyAction::ClearSearch) => {
            return Some(ExplorerKeyAction::ClearSearch);
        }
        Some(KeyAction::StartSearch) => {
            return Some(ExplorerKeyAction::FocusSearch);
        }
        Some(KeyAction::StopSearch) => {
            return Some(ExplorerKeyAction::StopSearch);
        }
        None => {}
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerKeyAction {
    DeleteSelected,
    CopySelected,
    CutToMovable,
    PasteMovable,
    SelectAll,
    ClearSearch,
    FocusSearch,
    StopSearch,
}

/// Enhanced context menu for files with queue integration.
pub fn show_enhanced_file_context_menu(
    ui: &mut egui::Ui,
    app: &SortwiseApp,
    path: &PathBuf,
    queue_count: usize,
) -> Option<ExplorerContextAction> {
    let mut action = None;

    // Queue operations
    if queue_count > 0 {
        ui.label(egui::RichText::new("📋 Queues").small().color(egui::Color32::GRAY));
        for (idx, queue) in app.explorer_queues.iter().enumerate() {
            if ui.button(format!("  Add to: {}", queue.name)).clicked() {
                action = Some(ExplorerContextAction::AddToQueue(idx, path.clone()));
                ui.close_menu();
            }
        }
        ui.separator();
    }

    // Movable operations
    if ui.button("🔗 Add to Movable Queue").clicked() {
        action = Some(ExplorerContextAction::MoveToMovable(path.clone()));
        ui.close_menu();
    }

    // Open operations
    if ui.button("👁 Preview").clicked() {
        action = Some(ExplorerContextAction::Preview(path.clone()));
        ui.close_menu();
    }

    if ui.button("🗂 Open in Default").clicked() {
        action = Some(ExplorerContextAction::OpenDefault(path.clone()));
        ui.close_menu();
    }

    ui.separator();

    // File operations
    if ui.button("📋 Copy Path").clicked() {
        ui.output_mut(|o| o.copied_text = path.display().to_string());
        action = Some(ExplorerContextAction::CopyPath);
        ui.close_menu();
    }

    #[cfg(target_os = "macos")]
    {
        if ui.button("🔍 Show in Finder").clicked() {
            action = Some(ExplorerContextAction::ShowInFinder(path.clone()));
            ui.close_menu();
        }
    }

    if ui.button("🗑 Move to Trash").clicked() {
        action = Some(ExplorerContextAction::MoveToTrash(path.clone()));
        ui.close_menu();
    }

    action
}

#[derive(Debug, Clone)]
pub enum ExplorerContextAction {
    AddToQueue(usize, PathBuf),
    MoveToMovable(PathBuf),
    Preview(PathBuf),
    OpenDefault(PathBuf),
    CopyPath,
    ShowInFinder(PathBuf),
    MoveToTrash(PathBuf),
}

/// Apply keyboard action to app state.
pub fn apply_key_action(app: &mut SortwiseApp, action: ExplorerKeyAction) {
    match action {
        ExplorerKeyAction::DeleteSelected => {
            if !app.explorer_selected.is_empty() {
                app.explorer_status = Some(format!("Deleting {} items...", app.explorer_selected.len()));
            }
        }
        ExplorerKeyAction::CopySelected => {
            let paths = app.explorer_selected.iter()
                .filter_map(|&idx| app.explorer_entries.get(idx).map(|e| e.path.clone()))
                .collect::<Vec<_>>()
                .join("\n");
            if !paths.is_empty() {
                // Copy to system clipboard (handled via egui output in caller)
                app.explorer_status = Some(format!("Copied {} item(s) to clipboard.", app.explorer_selected.len()));
            }
        }
        ExplorerKeyAction::CutToMovable => {
            if !app.explorer_selected.is_empty() {
                for &idx in &app.explorer_selected {
                    if let Some(entry) = app.explorer_entries.get(idx) {
                        toggle_movable(app, entry.path.clone());
                    }
                }
                app.explorer_status = Some(format!("Added {} item(s) to movable queue.", app.explorer_selected.len()));
            }
        }
        ExplorerKeyAction::PasteMovable => {
            if !app.movable_items.is_empty() {
                app.explorer_status = Some(format!("Ready to move {} item(s).", app.movable_items.len()));
            }
        }
        ExplorerKeyAction::SelectAll => {
            app.explorer_selected = (0..app.explorer_entries.len()).collect();
            app.explorer_status = Some(format!("Selected {} items.", app.explorer_entries.len()));
        }
        ExplorerKeyAction::ClearSearch => {
            app.explorer_search_input.clear();
            app.explorer_filtered_entries = app.explorer_entries.clone();
            app.explorer_status = Some("Search cleared.".to_string());
        }
        ExplorerKeyAction::FocusSearch => {
            app.explorer_status = Some("Search focused - start typing.".to_string());
        }
        ExplorerKeyAction::StopSearch => {
            app.explorer_search_input.clear();
            app.explorer_status = Some("Search stopped.".to_string());
        }
    }
}

/// Apply context menu action to app state.
pub fn apply_context_action(app: &mut SortwiseApp, action: ExplorerContextAction) {
    match action {
        ExplorerContextAction::AddToQueue(queue_idx, path) => {
            explorer_add_to_queue(app, queue_idx, path);
        }
        ExplorerContextAction::MoveToMovable(path) => {
            toggle_movable(app, path);
        }
        ExplorerContextAction::Preview(path) => {
            app.explorer_preview_path = Some(path);
        }
        ExplorerContextAction::OpenDefault(path) => {
            let _ = open_in_default_app(&path);
        }
        ExplorerContextAction::CopyPath => {
            // Already handled in menu (UI copies to clipboard)
        }
        ExplorerContextAction::ShowInFinder(path) => {
            let _ = reveal_in_folder(&path);
        }
        ExplorerContextAction::MoveToTrash(path) => {
            let _ = sortwise::resolver::trash_single_path(&path);
            app.explorer_status = Some("Item moved to trash.".to_string());
        }
    }
}
