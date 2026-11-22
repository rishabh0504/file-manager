//! Archive file move operations.

use std::path::PathBuf;
use crate::app::{SortwiseApp, AppView};
use super::move_ops::{fast_move, unique_file_dest};

pub fn execute_move_archive_files(app: &mut SortwiseApp) {
    let Some(target_dir) = &app.archive_files_move_target else { return; };
    if !target_dir.is_dir() { return; }

    let to_move: Vec<PathBuf> = app.archive_files_selected.iter().cloned().collect();
    let total = to_move.len();
    if total == 0 { return; }

    let confirm = rfd::MessageDialog::new().set_level(rfd::MessageLevel::Info).set_title("Confirm Move")
        .set_description(format!("Move {} archive file(s) to:\n{}", total, target_dir.display()))
        .set_buttons(rfd::MessageButtons::YesNo).show();

    if confirm != rfd::MessageDialogResult::Yes { return; }

    app.archive_files_moving = true;
    if let Ok(mut p) = app.archive_files_move_progress.lock() { *p = (0, total, String::new()); }

    let (tx, rx) = std::sync::mpsc::channel();
    app.archive_files_move_rx = Some(rx);

    let target_dir = target_dir.clone();
    let progress = std::sync::Arc::clone(&app.archive_files_move_progress);

    std::thread::spawn(move || {
        let mut success_count = 0;
        let mut fail_count = 0;

        for (idx, p) in to_move.iter().enumerate() {
            let fname = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if let Ok(mut prog) = progress.lock() { *prog = (idx, total, fname.clone()); }

            if !p.is_file() { fail_count += 1; continue; }

            let dest = unique_file_dest(&target_dir, p);
            let moved = fast_move(p, &dest).is_ok();
            if moved { success_count += 1; } else { fail_count += 1; }
        }

        if let Ok(mut prog) = progress.lock() { *prog = (total, total, String::new()); }
        let _ = tx.send((success_count, fail_count));
    });
}

pub fn poll_archive_files_move_result(app: &mut SortwiseApp) {
    if let Some(rx) = &app.archive_files_move_rx {
        if let Ok((success_count, fail_count)) = rx.try_recv() {
            app.archive_files_moving = false;
            app.archive_files_move_rx = None;

            rfd::MessageDialog::new().set_level(rfd::MessageLevel::Info).set_title("Move Completed")
                .set_description(format!("Successfully moved {} files.\nFailed to move {} files.", success_count, fail_count))
                .set_buttons(rfd::MessageButtons::Ok).show();

            app.archive_files_results.clear();
            app.archive_files_selected.clear();
            app.view = AppView::Scan;
        }
    }
}
