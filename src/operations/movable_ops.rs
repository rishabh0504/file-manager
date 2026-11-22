//! Movable queue operations.

use std::path::{Path, PathBuf};
use crate::app::{SortwiseApp, MovableItem};
use super::move_ops::{fast_move, fast_move_dir, unique_file_dest, unique_dir_dest};

pub fn is_in_movable(app: &SortwiseApp, path: &Path) -> bool {
    app.movable_items.iter().any(|item| item.path == path)
}

pub fn stage_paths(app: &mut SortwiseApp, paths: &[PathBuf]) -> usize {
    let default_q = app.movable_last_chosen_queue.clone();
    let mut added = 0;
    for path in paths {
        if app.movable_items.iter().any(|item| &item.path == path) { continue; }
        app.movable_items.push(MovableItem { path: path.clone(), target_queue: default_q.clone() });
        added += 1;
    }
    added
}

pub fn toggle_movable(app: &mut SortwiseApp, path: PathBuf) {
    if let Some(pos) = app.movable_items.iter().position(|item| item.path == path) {
        app.movable_items.remove(pos);
        app.explorer_status = Some("Removed from movable queue.".to_string());
    } else {
        let default_q = app.movable_last_chosen_queue.clone();
        app.movable_items.push(MovableItem { path, target_queue: default_q });
        app.explorer_status = Some("Added to movable queue.".to_string());
    }
}

pub fn remove_from_movable(app: &mut SortwiseApp, idx: usize) {
    if idx < app.movable_items.len() { app.movable_items.remove(idx); }
}

pub fn clear_movable_items(app: &mut SortwiseApp) {
    app.movable_items.clear();
}

pub fn move_single_movable_item(app: &mut SortwiseApp, idx: usize) -> bool {
    if idx >= app.movable_items.len() { return false; }
    let (p, q_name) = {
        let item = &app.movable_items[idx];
        let Some(ref q) = item.target_queue else { app.explorer_status = Some("Select a queue first.".to_string()); return false; };
        (item.path.clone(), q.clone())
    };

    let target_dir = match app.explorer_queues.iter().find(|q| q.name == q_name) {
        Some(q) => q.target.clone(),
        None => { app.explorer_status = Some(format!("Queue "{q_name}" not found.")); return false; }
    };

    if !target_dir.is_dir() { let _ = std::fs::create_dir_all(&target_dir); }

    let fname = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let was_dir = p.is_dir();
    let moved = if p.is_dir() {
        let dest = unique_dir_dest(&target_dir, &fname);
        fast_move_dir(&p, &dest).is_ok()
    } else if p.is_file() {
        let dest = unique_file_dest(&target_dir, &p);
        fast_move(&p, &dest).is_ok()
    } else {
        false
    };

    if moved {
        app.movable_items.remove(idx);
        app.explorer_files_forget_path(&p, was_dir);
        app.explorer_status = Some(format!("Moved "{fname}" → "{q_name}"."));
        app.explorer_entries.retain(|e| e.path != p);
        if app.folder_modal_open { app.folder_modal_entries.retain(|e| e.path != p); }
        app.thumbnail_cache.remove(&p);
        app.thumbnail_loading.remove(&p);
        if app.explorer_preview_path.as_ref() == Some(&p) { app.close_explorer_preview(); }
        true
    } else {
        app.explorer_status = Some(format!("Failed to move "{fname}".")); false
    }
}

pub fn move_all_movable_items(app: &mut SortwiseApp) -> (usize, usize) {
    if app.movable_items.is_empty() { return (0, 0); }
    let mut success_count = 0;
    let mut fail_count = 0;
    let mut i = 0;
    while i < app.movable_items.len() {
        if app.movable_items[i].target_queue.is_none() { i += 1; continue; }
        if move_single_movable_item(app, i) { success_count += 1; } else { fail_count += 1; i += 1; }
    }
    app.explorer_reload();
    (success_count, fail_count)
}
