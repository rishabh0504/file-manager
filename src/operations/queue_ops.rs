//! Queue staging and submission operations.

use std::path::{Path, PathBuf};
use crate::app::SortwiseApp;
use crate::types::queue::FileQueue;
use super::move_ops::{fast_move, fast_move_dir, unique_file_dest, unique_dir_dest};

pub fn explorer_new_queue(app: &mut SortwiseApp) {
    if let Some(folders) = rfd::FileDialog::new().pick_folders() {
        if folders.is_empty() { return; }
        let mut created_count = 0;
        let mut last_name = String::new();
        for dir in folders {
            let name = dir.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Queue".to_string());
            app.explorer_queues.push(FileQueue {
                name: name.clone(),
                target: dir,
                items: Vec::new(),
            });
            last_name = name;
            created_count += 1;
        }
        if created_count == 1 {
            app.explorer_status = Some(format!("Created queue \"{last_name}\"."));
        } else if created_count > 1 {
            app.explorer_status = Some(format!("Created {created_count} queues."));
        }
        if !app.explorer_queues.is_empty() {
            app.explorer_preview_queue_pick = Some(app.explorer_queues.len() - 1);
        }
    }
}

pub fn explorer_add_to_queue(app: &mut SortwiseApp, idx: usize, path: PathBuf) {
    let Some(q) = app.explorer_queues.get_mut(idx) else { return };
    let fname = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if q.items.contains(&path) {
        app.explorer_status = Some(format!("\"{fname}\" is already in \"{}\".", q.name));
    } else {
        q.items.push(path);
        app.explorer_status = Some(format!("Added \"{fname}\" to \"{}\".", q.name));
    }
}

pub fn explorer_is_in_queue(app: &SortwiseApp, idx: usize, path: &Path) -> bool {
    app.explorer_queues.get(idx).map(|q| q.items.iter().any(|p| p == path)).unwrap_or(false)
}

pub fn explorer_find_queue_for_path(app: &SortwiseApp, path: &Path) -> Option<usize> {
    app.explorer_queues.iter().position(|q| q.items.iter().any(|p| p == path))
}

pub fn explorer_toggle_queue(app: &mut SortwiseApp, idx: usize, path: PathBuf) {
    let is_in = explorer_is_in_queue(app, idx, &path);
    let fname = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if is_in {
        let qname = app.explorer_queues.get(idx).map(|q| q.name.clone()).unwrap_or_default();
        explorer_remove_from_queue(app, idx, path);
        app.explorer_status = Some(format!("Removed \"{fname}\" from \"{qname}\"."));
    } else {
        explorer_add_to_queue(app, idx, path);
    }
}

pub fn explorer_remove_from_queue(app: &mut SortwiseApp, idx: usize, path: PathBuf) {
    if let Some(q) = app.explorer_queues.get_mut(idx) {
        q.items.retain(|p| p != &path);
    }
}

pub fn explorer_delete_queue(app: &mut SortwiseApp, idx: usize) {
    if idx < app.explorer_queues.len() {
        let name = app.explorer_queues[idx].name.clone();
        app.explorer_queues.remove(idx);
        if app.explorer_queues.is_empty() {
            app.explorer_active_queue = None;
        } else {
            match app.explorer_active_queue {
                Some(active) if active == idx => {
                    app.explorer_active_queue = Some(idx.min(app.explorer_queues.len() - 1));
                }
                Some(active) if active > idx => {
                    app.explorer_active_queue = Some(active - 1);
                }
                _ => {}
            }
        }
        app.explorer_preview_queue_pick = None;
        app.explorer_status = Some(format!("Deleted queue \"{name}\"."));
    }
}

pub fn explorer_change_queue_target(app: &mut SortwiseApp, idx: usize) {
    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
        if let Some(q) = app.explorer_queues.get_mut(idx) {
            q.target = dir;
            app.explorer_status = Some(format!("Updated folder for \"{}\".", q.name));
        }
    }
}

pub fn submit_all_queues(app: &mut SortwiseApp) {
    let total_files: usize = app.explorer_queues.iter().map(|q| q.items.len()).sum();
    if total_files == 0 {
        app.explorer_status = Some("All queues are empty --- nothing to move.".to_string());
        return;
    }

    let confirm = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Info)
        .set_title("Submit All Queues")
        .set_description(format!(
            "Move all {} file(s) across {} queue(s) to their respective target folders?",
            total_files, app.explorer_queues.len()
        ))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();
    if confirm != rfd::MessageDialogResult::Yes { return; }

    app.explorer_queue_submitting = true;
    app.explorer_queue_submitting_idx = None;
    if let Ok(mut p) = app.explorer_queue_submit_progress.lock() {
        *p = (0, total_files, String::new());
    }

    let queue_snapshots: Vec<(PathBuf, Vec<PathBuf>)> = app.explorer_queues.iter()
        .map(|q| (q.target.clone(), q.items.clone())).collect();

    let (tx, rx) = std::sync::mpsc::channel();
    app.explorer_queue_submit_rx = Some(rx);
    let progress = std::sync::Arc::clone(&app.explorer_queue_submit_progress);

    std::thread::spawn(move || {
        let mut success_count = 0;
        let mut fail_count = 0;
        let mut current_idx = 0;

        for (target_dir, items) in queue_snapshots {
            if !target_dir.is_dir() {
                let _ = std::fs::create_dir_all(&target_dir);
            }

            for p in items {
                let fname = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if let Ok(mut prog) = progress.lock() {
                    *prog = (current_idx, total_files, fname.clone());
                }
                current_idx += 1;

                let moved = if p.is_dir() {
                    let dest = unique_dir_dest(&target_dir, &fname);
                    fast_move_dir(&p, &dest).is_ok()
                } else if p.is_file() {
                    let dest = unique_file_dest(&target_dir, &p);
                    fast_move(&p, &dest).is_ok()
                } else {
                    false
                };

                if moved { success_count += 1; } else { fail_count += 1; }
            }
        }

        if let Ok(mut prog) = progress.lock() {
            *prog = (total_files, total_files, String::new());
        }
        let _ = tx.send((success_count, fail_count));
    });
}

pub fn poll_queue_submit_result(app: &mut SortwiseApp) {
    if let Some(rx) = &app.explorer_queue_submit_rx {
        if let Ok((success_count, fail_count)) = rx.try_recv() {
            app.explorer_queue_submitting = false;
            app.explorer_queue_submit_rx = None;

            if let Some(idx) = app.explorer_queue_submitting_idx.take() {
                if let Some(q) = app.explorer_queues.get_mut(idx) {
                    q.items.clear();
                }
            } else {
                for q in &mut app.explorer_queues {
                    q.items.clear();
                }
            }

            let msg = if fail_count == 0 {
                format!("Moved {success_count} file(s) across queues.")
            } else {
                format!("Moved {success_count} file(s); {fail_count} failed.")
            };
            app.explorer_status = Some(msg);
            app.explorer_files_prune_missing();
            app.explorer_reload();
        }
    }
}
