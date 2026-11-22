//! Staging: folder queues, movable items, settings persistence and queue submit.
use super::*;

impl SortwiseApp {
    /// Prompt for one or more destination folders and create new queues named after them.
    pub fn explorer_new_queue(&mut self) {
        if let Some(folders) = rfd::FileDialog::new().pick_folders() {
            if folders.is_empty() {
                return;
            }
            let mut created_count = 0;
            let mut last_name = String::new();
            for dir in folders {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Queue".to_string());
                self.explorer_queues.push(FileQueue {
                    name: name.clone(),
                    target: dir,
                    items: Vec::new(),
                });
                last_name = name;
                created_count += 1;
            }
            if created_count == 1 {
                self.explorer_status = Some(format!("Created queue “{last_name}”."));
            } else if created_count > 1 {
                self.explorer_status = Some(format!("Created {created_count} queues."));
            }
            if !self.explorer_queues.is_empty() {
                self.explorer_preview_queue_pick = Some(self.explorer_queues.len() - 1);
            }
        }
    }

    /// Stage a file or folder into queue[idx] (deduped by path).
    pub fn explorer_add_to_queue(&mut self, idx: usize, path: PathBuf) {
        if idx >= self.explorer_queues.len() {
            return;
        }
        let fname = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let qname = self.explorer_queues[idx].name.clone();
        if self.explorer_queues[idx].items.contains(&path) {
            self.explorer_status = Some(format!("“{fname}” is already in “{qname}”."));
            return;
        }
        let from = self.unstage_elsewhere(idx, &path);
        self.explorer_queues[idx].items.push(path);
        self.explorer_status = Some(match from {
            Some(old) => format!("Moved “{fname}” from “{old}” to “{qname}”."),
            None => format!("Added “{fname}” to “{qname}”."),
        });
    }

    /// A path lives in at most one place, so it can never be moved twice. Removes it from every
    /// queue except `keep_idx` and from Items; returns where it was, for the status message.
    pub(crate) fn unstage_elsewhere(&mut self, keep_idx: usize, path: &Path) -> Option<String> {
        let mut from = None;
        for (i, q) in self.explorer_queues.iter_mut().enumerate() {
            if i != keep_idx && q.items.iter().any(|p| p == path) {
                q.items.retain(|p| p != path);
                from = Some(q.name.clone());
            }
        }
        if self.movable_items.iter().any(|m| m.path == path) {
            self.movable_items.retain(|m| m.path != path);
            from.get_or_insert_with(|| "Items".to_string());
        }
        from
    }

    /// Check if a path is currently staged in queue[idx].
    pub fn explorer_is_in_queue(&self, idx: usize, path: &Path) -> bool {
        self.explorer_queues
            .get(idx)
            .map(|q| q.items.iter().any(|p| p == path))
            .unwrap_or(false)
    }

    /// Find the index of the first queue containing the given path, if any.
    pub fn explorer_find_queue_for_path(&self, path: &Path) -> Option<usize> {
        self.explorer_queues
            .iter()
            .position(|q| q.items.iter().any(|p| p == path))
    }

    /// Toggle queue staging for a file or folder (stages or removes).
    pub fn explorer_toggle_queue(&mut self, idx: usize, path: PathBuf) {
        let is_in = self.explorer_is_in_queue(idx, &path);
        let fname = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if is_in {
            let qname = self.explorer_queues.get(idx).map(|q| q.name.clone()).unwrap_or_default();
            self.explorer_remove_from_queue(idx, path);
            self.explorer_status = Some(format!("Removed “{fname}” from “{qname}”."));
        } else {
            self.explorer_add_to_queue(idx, path);
        }
    }

    /// Remove a staged file from queue[idx] (does not touch the real file).
    pub fn explorer_remove_from_queue(&mut self, idx: usize, path: PathBuf) {
        if let Some(q) = self.explorer_queues.get_mut(idx) {
            q.items.retain(|p| p != &path);
        }
    }

    /// Delete a queue entirely (staging list only; never deletes real files).
    pub fn explorer_delete_queue(&mut self, idx: usize) {
        if idx < self.explorer_queues.len() {
            let name = self.explorer_queues[idx].name.clone();
            self.explorer_queues.remove(idx);
            if self.explorer_queues.is_empty() {
                self.explorer_active_queue = None;
            } else {
                match self.explorer_active_queue {
                    Some(active) if active == idx => {
                        self.explorer_active_queue = Some(idx.min(self.explorer_queues.len() - 1));
                    }
                    Some(active) if active > idx => {
                        self.explorer_active_queue = Some(active - 1);
                    }
                    _ => {}
                }
            }
            // Keep the dropdown pick valid.
            self.explorer_preview_queue_pick = None;
            self.explorer_status = Some(format!("Deleted queue “{name}”."));
        }
    }

    // -------------------------------------------------------------------------
    // Movable Queue logic (independent runtime triage & individual item moves)
    // -------------------------------------------------------------------------

    pub fn current_settings(&self) -> sortwise::settings::Settings {
        sortwise::settings::Settings {
            version: sortwise::settings::CURRENT_VERSION,
            skip_system_dirs: self.skip_system_dirs,
            skip_hidden: self.skip_hidden,
            min_file_size: self.min_file_size,
            use_hash_cache: self.use_hash_cache,
            skip_logs_and_locks: self.skip_logs_and_locks,
            io_profile: self.io_profile,
        }
    }

    /// Apply loaded settings. The action mode is never restored (see settings.rs).
    pub fn apply_settings(&mut self, s: &sortwise::settings::Settings) {
        self.skip_system_dirs = s.skip_system_dirs;
        self.skip_hidden = s.skip_hidden;
        self.min_file_size = s.min_file_size;
        self.use_hash_cache = s.use_hash_cache;
        self.skip_logs_and_locks = s.skip_logs_and_locks;
        self.io_profile = s.io_profile;
    }

    /// Size of a movable-queue item, read once and cached (P-5).
    pub fn movable_size(&mut self, path: &Path) -> Option<u64> {
        if let Some(&size) = self.movable_size_cache.get(path) {
            return Some(size);
        }
        let size = std::fs::metadata(path).ok()?.len();
        self.movable_size_cache.insert(path.to_path_buf(), size);
        Some(size)
    }

    pub fn is_in_movable(&self, path: &Path) -> bool {
        self.movable_items.iter().any(|item| item.path == path)
    }

    /// Stage every given file that is not staged yet. Already-staged paths are left alone.
    pub fn stage_paths(&mut self, paths: &[PathBuf]) -> usize {
        let default_q = self.movable_last_chosen_queue.clone();
        let mut added = 0;
        for path in paths {
            if self.movable_items.iter().any(|item| &item.path == path) {
                continue;
            }
            self.movable_items.push(MovableItem {
                path: path.clone(),
                target_queue: default_q.clone(),
            });
            added += 1;
        }
        added
    }

    pub fn toggle_movable(&mut self, path: PathBuf) {
        if let Some(pos) = self.movable_items.iter().position(|item| item.path == path) {
            self.movable_items.remove(pos);
            self.explorer_status = Some("Removed from movable queue.".to_string());
        } else {
            let default_q = self.movable_last_chosen_queue.clone();
            self.movable_items.push(MovableItem {
                path,
                target_queue: default_q,
            });
            self.explorer_status = Some("Added to movable queue.".to_string());
        }
    }

    pub fn remove_from_movable(&mut self, idx: usize) {
        if idx < self.movable_items.len() {
            self.movable_items.remove(idx);
        }
    }

    pub fn clear_movable_items(&mut self) {
        self.movable_items.clear();
    }

    /// Drop a path that just left its folder from every listing, cache and preview.
    fn forget_moved_path(&mut self, p: &Path, was_dir: bool) {
        self.explorer_files_forget_path(p, was_dir);
        let gone = |x: &Path| x == p || (was_dir && x.starts_with(p));
        self.explorer_entries.retain(|e| !gone(&e.path));
        if self.folder_modal_open {
            self.folder_modal_entries.retain(|e| !gone(&e.path));
        }
        self.thumbnail_cache.retain(|k, _| !gone(k));
        self.thumbnail_loading.retain(|k| !gone(k));
        if self.explorer_preview_path.as_deref().is_some_and(gone) {
            self.close_explorer_preview();
        }
    }

    /// Move staged Items to their queue folders on a background thread, so a large
    /// cross-drive move never freezes the window. `only` limits it to one item.
    pub fn start_movable_move(&mut self, only: Option<usize>) {
        if self.movable_moving {
            return;
        }
        let jobs: Vec<(PathBuf, PathBuf, bool)> = self
            .movable_items
            .iter()
            .enumerate()
            .filter(|(i, _)| only.is_none_or(|o| o == *i))
            .filter_map(|(_, item)| {
                let q = item.target_queue.as_ref()?;
                let target = self.explorer_queues.iter().find(|fq| &fq.name == q)?.target.clone();
                Some((item.path.clone(), target, item.path.is_dir()))
            })
            .collect();
        if jobs.is_empty() {
            self.explorer_status = Some("Choose a queue for the item first.".to_string());
            return;
        }

        let total = jobs.len();
        self.movable_moving = true;
        if let Ok(mut p) = self.movable_move_progress.lock() {
            *p = (0, total, String::new());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.movable_move_rx = Some(rx);
        let progress = Arc::clone(&self.movable_move_progress);
        std::thread::spawn(move || {
            let mut results = Vec::with_capacity(total);
            for (i, (path, target, was_dir)) in jobs.into_iter().enumerate() {
                if let Ok(mut p) = progress.lock() {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    *p = (i, total, name);
                }
                let moved = move_into_dir(&path, &target);
                results.push((path, was_dir, moved));
            }
            if let Ok(mut p) = progress.lock() {
                *p = (total, total, String::new());
            }
            let _ = tx.send(results);
        });
    }

    /// Collect a finished background Items move: drop moved items, keep failed ones.
    pub fn poll_movable_move(&mut self) {
        let Some(rx) = &self.movable_move_rx else {
            return;
        };
        let results = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Worker died: keep every item that is still at its source.
                self.movable_moving = false;
                self.movable_move_rx = None;
                self.movable_items.retain(|m| m.path.symlink_metadata().is_ok());
                self.explorer_status = Some("Moving stopped unexpectedly. Items not moved are still staged.".into());
                self.explorer_reload();
                return;
            }
        };
        self.movable_moving = false;
        self.movable_move_rx = None;
        let mut ok = 0usize;
        let mut failed = 0usize;
        for (path, was_dir, moved) in results {
            if moved {
                ok += 1;
                self.movable_items.retain(|m| m.path != path);
                self.forget_moved_path(&path, was_dir);
            } else {
                failed += 1;
            }
        }
        self.explorer_status = Some(if failed == 0 {
            format!("Moved {ok} item(s).")
        } else {
            format!("Moved {ok} item(s); {failed} failed and are still staged.")
        });
        self.explorer_reload();
    }

    /// Re-pick the destination folder for queue[idx].
    pub fn explorer_change_queue_target(&mut self, idx: usize) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            if let Some(q) = self.explorer_queues.get_mut(idx) {
                q.target = dir;
                self.explorer_status = Some(format!("Updated folder for “{}”.", q.name));
            }
        }
    }


    /// Move all staged items across ALL queues into their respective target folders in one batch.
    pub fn submit_all_queues(&mut self) {
        let total_files: usize = self.explorer_queues.iter().map(|q| q.items.len()).sum();
        if total_files == 0 {
            self.explorer_status = Some("All queues are empty — nothing to move.".to_string());
            return;
        }

        let confirm = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Info)
            .set_title("Submit All Queues")
            .set_description(format!(
                "Move all {} file(s) across {} queue(s) to their respective target folders?",
                total_files,
                self.explorer_queues.len()
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if confirm != rfd::MessageDialogResult::Yes {
            return;
        }

        self.explorer_queue_submitting = true;
        self.explorer_queue_submitting_idx = None; // None signals "all queues"
        if let Ok(mut p) = self.explorer_queue_submit_progress.lock() {
            *p = (0, total_files, String::new());
        }

        let queue_snapshots: Vec<(PathBuf, Vec<PathBuf>)> = self
            .explorer_queues
            .iter()
            .map(|q| (q.target.clone(), q.items.clone()))
            .collect();

        let (tx, rx) = std::sync::mpsc::channel();
        self.explorer_queue_submit_rx = Some(rx);
        let progress = std::sync::Arc::clone(&self.explorer_queue_submit_progress);

        std::thread::spawn(move || {
            let mut success_count = 0;
            let mut fail_count = 0;
            let mut current_idx = 0;

            for (target_dir, items) in queue_snapshots {
                if !target_dir.is_dir() {
                    let _ = std::fs::create_dir_all(&target_dir);
                }

                for p in items {
                    let fname = p
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if let Ok(mut prog) = progress.lock() {
                        *prog = (current_idx, total_files, fname);
                    }
                    current_idx += 1;

                    let moved = move_into_dir(&p, &target_dir);

                    if moved {
                        success_count += 1;
                    } else {
                        fail_count += 1;
                    }
                }
            }

            if let Ok(mut prog) = progress.lock() {
                *prog = (total_files, total_files, String::new());
            }
            let _ = tx.send((success_count, fail_count));
        });
    }

    /// Poll the background submit thread; on completion clear moved items and
    /// refresh the listing. Mirrors `poll_archive_files_move_result`.
    pub fn poll_queue_submit_result(&mut self) {
        if let Some(rx) = &self.explorer_queue_submit_rx {
            let (success_count, fail_count) = match rx.try_recv() {
                Ok(counts) => counts,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Worker died without reporting: unblock the UI and keep what was not moved.
                    self.explorer_queue_submitting = false;
                    self.explorer_queue_submit_rx = None;
                    self.explorer_queue_submitting_idx = None;
                    for q in &mut self.explorer_queues {
                        q.items.retain(|p| p.symlink_metadata().is_ok());
                    }
                    self.explorer_status = Some("Moving stopped unexpectedly. Items not moved are still queued.".into());
                    self.explorer_files_prune_missing();
                    self.explorer_reload();
                    return;
                }
            };
            {
                self.explorer_queue_submitting = false;
                self.explorer_queue_submit_rx = None;

                // Drop only the items that left their source; failed ones stay queued for a retry.
                let submitted: Vec<usize> = match self.explorer_queue_submitting_idx.take() {
                    Some(idx) => vec![idx],
                    None => (0..self.explorer_queues.len()).collect(),
                };
                for idx in submitted {
                    if let Some(q) = self.explorer_queues.get_mut(idx) {
                        q.items.retain(|p| p.symlink_metadata().is_ok());
                    }
                }

                let msg = if fail_count == 0 {
                    format!("Moved {success_count} file(s) across queues.")
                } else {
                    format!("Moved {success_count} file(s); {fail_count} failed and are still queued.")
                };
                self.explorer_status = Some(msg);
                self.explorer_files_prune_missing();
                self.explorer_reload();
            }
        }
    }
}
