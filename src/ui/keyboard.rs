//! Keyboard shortcuts handler.

use eframe::egui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    Delete,
    Copy,
    Cut,
    Paste,
    SelectAll,
    ClearSearch,
    StartSearch,
    StopSearch,
}

pub fn handle_key_press(ctx: &egui::Context) -> Option<KeyAction> {
    if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
        return Some(KeyAction::Delete);
    }

    if ctx.input(|i| i.key_pressed(egui::Key::c) && i.modifiers.ctrl) {
        return Some(KeyAction::Copy);
    }

    if ctx.input(|i| i.key_pressed(egui::Key::x) && i.modifiers.ctrl) {
        return Some(KeyAction::Cut);
    }

    if ctx.input(|i| i.key_pressed(egui::Key::v) && i.modifiers.ctrl) {
        return Some(KeyAction::Paste);
    }

    if ctx.input(|i| i.key_pressed(egui::Key::a) && i.modifiers.ctrl) {
        return Some(KeyAction::SelectAll);
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return Some(KeyAction::ClearSearch);
    }

    None
}

pub fn get_keyboard_shortcuts() -> &'static str {
    r#"
⌨️ KEYBOARD SHORTCUTS

Navigation:
  • Tab       - Move between sections
  • ↑/↓       - Navigate file list
  • Enter     - Open/Select item

Operations:
  • Delete    - Send to trash
  • Cmd+C     - Copy selected
  • Cmd+X     - Cut to movable queue
  • Cmd+V     - Paste/Move
  • Cmd+A     - Select all
  • Escape    - Clear search

File Manager:
  • Cmd+N     - New queue
  • Cmd+F     - Focus search
  • Cmd+D     - Start scan
"#
}
