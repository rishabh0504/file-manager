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
        let Some(q) = self.explorer_queues.get_mut(idx) else { return };
        let fname = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if q.items.contains(&path) {
            self.explorer_status = Some(format!("“{fname}” is already in “{}”.", q.name));
        } else {
            q.items.push(path);
            self.explorer_status = Some(format!("Added “{fname}” to “{}”.", q.name));
        }
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

    /// Move a single item from the movable queue to its assigned target queue folder.
    /// Preserves existing files: uses unique_file_dest and unique_dir_dest to increment
    /// filename if destination file already exists.
    pub fn move_single_movable_item(&mut self, idx: usize) -> bool {
        if idx >= self.movable_items.len() {
            return false;
        }
        let (p, q_name) = {
            let item = &self.movable_items[idx];
            let Some(ref q) = item.target_queue else {
                self.explorer_status = Some("Select a queue first.".to_string());
                return false;
            };
            (item.path.clone(), q.clone())
        };

        let target_dir = match self.explorer_queues.iter().find(|q| q.name == q_name) {
            Some(q) => q.target.clone(),
            None => {
                self.explorer_status = Some(format!("Queue “{q_name}” not found."));
                return false;
            }
        };

        if !target_dir.is_dir() {
            let _ = std::fs::create_dir_all(&target_dir);
        }

        let fname = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

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
            self.movable_items.remove(idx);
            self.explorer_files_forget_path(&p, was_dir);
            self.explorer_status = Some(format!("Moved “{fname}” → “{q_name}”."));
            // Clean up caches & entries if it was currently in this directory
            self.explorer_entries.retain(|e| e.path != p);
            if self.folder_modal_open {
                self.folder_modal_entries.retain(|e| e.path != p);
            }
            self.thumbnail_cache.remove(&p);
            self.thumbnail_loading.remove(&p);
            if self.explorer_preview_path.as_ref() == Some(&p) {
                self.close_explorer_preview();
            }
            true
        } else {
            self.explorer_status = Some(format!("Failed to move “{fname}”."));
            false
        }
    }

    /// Move all items in the movable queue that have a valid target queue assigned.
    pub fn move_all_movable_items(&mut self) -> (usize, usize) {
        if self.movable_items.is_empty() {
            return (0, 0);
        }

        let mut success_count = 0;
        let mut fail_count = 0;
        let mut i = 0;

        while i < self.movable_items.len() {
            if self.movable_items[i].target_queue.is_none() {
                i += 1;
                continue;
            }
            if self.move_single_movable_item(i) {
                success_count += 1;
                // do not increment i since remove shifts elements
            } else {
                fail_count += 1;
                i += 1;
            }
        }

        self.explorer_reload();
        (success_count, fail_count)
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
            if let Ok((success_count, fail_count)) = rx.try_recv() {
                self.explorer_queue_submitting = false;
                self.explorer_queue_submit_rx = None;

                // Clear the moved items from the submitted queue (or all queues if submitting_idx was None).
                if let Some(idx) = self.explorer_queue_submitting_idx.take() {
                    if let Some(q) = self.explorer_queues.get_mut(idx) {
                        q.items.clear();
                    }
                } else {
                    for q in &mut self.explorer_queues {
                        q.items.clear();
                    }
                }

                let msg = if fail_count == 0 {
                    format!("Moved {success_count} file(s) across queues.")
                } else {
                    format!("Moved {success_count} file(s); {fail_count} failed.")
                };
                self.explorer_status = Some(msg);
                self.explorer_files_prune_missing();
                self.explorer_reload();
            }
        }
    }
}
