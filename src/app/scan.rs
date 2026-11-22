//! Scanning: duplicates, compare, empty folders and archives (start, poll, apply).
use super::*;

impl SortwiseApp {
    pub(crate) fn build_scan_config(&self) -> ScanConfig {
        let preferred: Vec<PathBuf> = self
            .preferred_dirs_text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect();
        let exclude_globs: Vec<String> = self
            .exclude_globs_text
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let excluded_subdirs: Vec<PathBuf> = self
            .excluded_subdirs_text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect();

        let (combined_roots, asymmetric_split_index) = if self.mode == AppMode::CompareSourceDest {
            let mut all = self.source_roots.clone();
            let split_idx = all.len();
            all.extend(self.dest_roots.clone());
            (all, Some(split_idx))
        } else {
            (self.roots.clone(), None)
        };

        let folder_configs: Vec<FolderConfig> = combined_roots
            .iter()
            .map(|r| FolderConfig {
                root: r.clone(),
                excluded_subdirs: excluded_subdirs.clone(),
            })
            .collect();

        ScanConfig {
            roots: combined_roots.clone(),
            folder_configs,
            skip_hidden: self.skip_hidden,
            skip_system_dirs: self.skip_system_dirs,
            min_file_size: self.min_file_size,
            action_mode: self.action_mode,
            preferred_dirs: preferred,
            root_order: combined_roots,
            exclude_globs,
            use_hash_cache: self.use_hash_cache,
            skip_logs_and_locks: self.skip_logs_and_locks,
            io_profile: self.io_profile,
            asymmetric_split_index,
            match_media_payload: self.match_media_payload,
        }
    }

    pub fn start_scan(&mut self) {
        self.apply_message = None;
        self.apply_errors = None;
        self.clear_preview_cache();
        self.results_selected_group = None;
        self.results_filter.clear();
        self.view = AppView::Scanning;
        self.scan_started_at = Some(Instant::now());
        self.set_last_result(None);
        self.scan_work = ScanWorkKind::FullScan;
        self.token = CancellationToken::new();

        let cfg = self.build_scan_config();
        let roots = cfg.roots.clone();

        {
            let mut p = self.progress.lock().unwrap();
            *p = ScanProgress::default();
            p.folders = roots
                .iter()
                .map(|r| FolderProgress {
                    folder_path: r.clone(),
                    files_total: 0,
                    bytes_total: 0,
                    files_done: 0,
                    bytes_done: 0,
                    status: FolderStatus::Scanning,
                })
                .collect();
        }

        let (tx, rx) = std::sync::mpsc::channel();
        self.result_rx = Some(rx);
        let (live_tx, live_rx) = std::sync::mpsc::channel();
        self.live_rx = Some(live_rx);
        self.live_groups.clear();

        let token = self.token.clone_token();
        let progress = Arc::clone(&self.progress);

        std::thread::spawn(move || {
            let progress_scan = Arc::clone(&progress);
            let entries = parallel_scan_roots(&roots, &cfg, &token, move |n, path| {
                let mut g = progress_scan.lock().unwrap();
                g.phase = sortwise::types::ProgressPhase::Enumerating {
                    files_found: n,
                    current_path: path,
                };
            });

            {
                let mut g = progress.lock().unwrap();
                for f in &mut g.folders {
                    f.status = FolderStatus::Hashing;
                }
            }

            let cache = if cfg.use_hash_cache {
                Some(HashCache::load())
            } else {
                None
            };

            let res = run_dedup_streaming(
                entries,
                &cfg,
                &token,
                Arc::clone(&progress),
                cache,
                live_tx,
            );

            {
                let mut g = progress.lock().unwrap();
                for f in &mut g.folders {
                    f.status = FolderStatus::Done { groups_found: 0 };
                }
            }

            let _ = tx.send(res);
        });
    }

    /// Re-hash and re-group only the files listed in the current duplicate report (no full tree walk).
    pub fn start_refresh_findings(&mut self) {
        let Some(prev) = self.last_result.clone() else {
            return;
        };
        if prev.duplicate_groups.is_empty() {
            return;
        }
        self.apply_message = None;
        self.apply_errors = None;
        self.view = AppView::Scanning;
        self.scan_work = ScanWorkKind::RefreshFindings;
        self.token = CancellationToken::new();
        {
            let mut p = self.progress.lock().unwrap();
            *p = ScanProgress::default();
            p.phase = sortwise::types::ProgressPhase::Processing {
                stage_label: "Preparing refresh".into(),
                stage: 0,
                stages_total: 4,
            };
        }

        let (tx, rx) = std::sync::mpsc::channel();
        self.result_rx = Some(rx);

        let cfg = self.build_scan_config();
        let token = self.token.clone_token();
        let progress = Arc::clone(&self.progress);

        std::thread::spawn(move || {
            let cache = if cfg.use_hash_cache {
                Some(HashCache::load())
            } else {
                None
            };

            let res = refresh_duplicates_from_result(&prev, &cfg, &token, progress, cache);

            let _ = tx.send(res);
        });
    }

    /// Move streamed groups into `live_groups`. Called while a scan runs.
    pub(crate) fn poll_live_groups(&mut self) {
        if let Some(rx) = &self.live_rx {
            while let Ok(batch) = rx.try_recv() {
                self.live_groups.extend(batch);
            }
        }
    }

    /// Show the groups found so far while the scan keeps running (P-6).
    pub fn show_live_results(&mut self) {
        let groups = self.live_groups.clone();
        let mut partial = ScanResult::empty_partial();
        partial.stats.duplicate_groups = groups.len() as u64;
        partial.stats.total_wasted_bytes = groups.iter().map(|g| g.total_wasted_bytes).sum();
        partial.duplicate_groups = groups;
        let has_groups = !partial.duplicate_groups.is_empty();
        self.set_last_result(Some(partial));
        self.results_selected_group = has_groups.then_some(0);
        self.results_selected_file_in_group = 0;
        self.view = AppView::Results;
    }

    pub(crate) fn poll_result(&mut self) {
        if let Some(rx) = &self.result_rx {
            let res = match rx.try_recv() {
                Ok(v) => v,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.result_rx = None;
                    self.live_rx = None;
                    self.scan_work = ScanWorkKind::FullScan;
                    self.view = if self.last_result.is_some() { AppView::Results } else { AppView::Scan };
                    worker_lost_dialog("The scan");
                    return;
                }
            };
            {
                let prior_path = self.last_result.as_ref().and_then(|r| {
                    let gi = self.results_selected_group?;
                    r.duplicate_groups
                        .get(gi)?
                        .files
                        .get(self.results_selected_file_in_group)
                        .map(|f| f.path.clone())
                });

                self.set_last_result(Some(res));
                if let Some(ref r) = self.last_result {
                    self.folder_overlap = crate::ui::folder_intelligence::compute_folder_overlap(r);
                }
                self.result_rx = None;
                self.live_rx = None;
                self.live_groups.clear();
                self.view = AppView::Results;
                self.scan_work = ScanWorkKind::FullScan;

                if let Some(p) = prior_path {
                    if let Some(r) = self.last_result.as_ref() {
                        let mut found = false;
                        for (gi, g) in r.duplicate_groups.iter().enumerate() {
                            if let Some(fi) = g.files.iter().position(|f| f.path == p) {
                                self.results_selected_group = Some(gi);
                                self.results_selected_file_in_group = fi;
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            if let Some(first) = r.duplicate_groups.first() {
                                self.results_selected_group = Some(0);
                                self.results_selected_file_in_group = first.keep_index;
                            } else {
                                self.results_selected_group = None;
                                self.results_selected_file_in_group = 0;
                            }
                        }
                    }
                } else if let Some(first) = self
                    .last_result
                    .as_ref()
                    .and_then(|r| r.duplicate_groups.first())
                {
                    self.results_selected_group = Some(0);
                    self.results_selected_file_in_group = first.keep_index;
                } else {
                    self.results_selected_group = None;
                    self.results_selected_file_in_group = 0;
                }
                self.clear_preview_cache();
            }
        }
    }

    pub fn start_empty_folders_scan(&mut self) {
        self.empty_folders_results.clear();
        self.empty_folders_selected.clear();
        self.empty_folders_scanning = true;
        self.view = AppView::Scanning;
        self.token = CancellationToken::new();

        let (tx, rx) = std::sync::mpsc::channel();
        self.empty_folders_rx = Some(rx);

        let roots = self.roots.clone();
        let token = self.token.clone_token();
        let progress_count = Arc::clone(&self.empty_folders_progress_count);
        let progress_path = Arc::clone(&self.empty_folders_progress_path);

        {
            *progress_count.lock().unwrap() = 0;
            *progress_path.lock().unwrap() = PathBuf::new();
        }

        std::thread::spawn(move || {
            let res = sortwise::scanner::find_empty_folders(&roots, &token, |count, path| {
                if let Ok(mut c) = progress_count.lock() {
                    *c = count;
                }
                if let Ok(mut p) = progress_path.lock() {
                    *p = path;
                }
            });
            let _ = tx.send(res);
        });
    }

    pub(crate) fn poll_empty_folders_result(&mut self) {
        if let Some(rx) = &self.empty_folders_rx {
            let res = match rx.try_recv() {
                Ok(v) => v,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.empty_folders_scanning = false;
                    self.empty_folders_rx = None;
                    self.view = AppView::Scan;
                    worker_lost_dialog("The empty-folder scan");
                    return;
                }
            };
            {
                self.empty_folders_selected = res.iter().cloned().collect();
                self.empty_folders_results = res;
                self.empty_folders_scanning = false;
                self.empty_folders_rx = None;
                self.view = AppView::Results;
            }
        }
    }

    pub fn start_archive_files_scan(&mut self) {
        self.archive_files_results.clear();
        self.archive_files_selected.clear();
        self.archive_files_scanning = true;
        self.view = AppView::Scanning;
        self.token = CancellationToken::new();

        let (tx, rx) = std::sync::mpsc::channel();
        self.archive_files_rx = Some(rx);

        let roots = self.roots.clone();
        let token = self.token.clone_token();
        let progress_count = Arc::clone(&self.archive_files_progress_count);
        let progress_path = Arc::clone(&self.archive_files_progress_path);

        {
            *progress_count.lock().unwrap() = 0;
            *progress_path.lock().unwrap() = PathBuf::new();
        }

        std::thread::spawn(move || {
            let res = sortwise::scanner::find_archive_files(&roots, &token, |count, path| {
                if let Ok(mut c) = progress_count.lock() {
                    *c = count;
                }
                if let Ok(mut p) = progress_path.lock() {
                    *p = path;
                }
            });
            let _ = tx.send(res);
        });
    }

    pub(crate) fn poll_archive_files_result(&mut self) {
        if let Some(rx) = &self.archive_files_rx {
            let res = match rx.try_recv() {
                Ok(v) => v,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.archive_files_scanning = false;
                    self.archive_files_rx = None;
                    self.view = AppView::Scan;
                    worker_lost_dialog("The archive scan");
                    return;
                }
            };
            {
                self.archive_files_results = res;
                // By default, select all found archive files
                self.archive_files_selected = self.archive_files_results.iter().cloned().collect();
                self.archive_files_scanning = false;
                self.archive_files_rx = None;
                self.view = AppView::Results;
            }
        }
    }

    pub fn poll_archive_files_move_result(&mut self) {
        if let Some(rx) = &self.archive_files_move_rx {
            let (success_count, fail_count) = match rx.try_recv() {
                Ok(v) => v,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.archive_files_moving = false;
                    self.archive_files_move_rx = None;
                    self.archive_files_results.retain(|p| p.symlink_metadata().is_ok());
                    self.archive_files_selected.retain(|p| p.symlink_metadata().is_ok());
                    worker_lost_dialog("Moving archive files");
                    return;
                }
            };
            {
                self.archive_files_moving = false;
                self.archive_files_move_rx = None;

                // Completion Dialog
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Info)
                    .set_title("Move Completed")
                    .set_description(format!(
                        "Successfully moved {} files.\nFailed to move {} files.",
                        success_count, fail_count
                    ))
                    .set_buttons(rfd::MessageButtons::Ok)
                    .show();

                // Moved files leave the list; failed ones stay so they can be retried.
                self.archive_files_results.retain(|p| p.symlink_metadata().is_ok());
                self.archive_files_selected.retain(|p| p.symlink_metadata().is_ok());
                if self.archive_files_results.is_empty() {
                    self.view = AppView::Scan;
                }
            }
        }
    }

    pub fn execute_move_archive_files(&mut self) {
        let Some(target_dir) = &self.archive_files_move_target else { return; };
        if !target_dir.is_dir() { return; }

        let to_move: Vec<PathBuf> = self.archive_files_selected.iter().cloned().collect();
        let total = to_move.len();
        if total == 0 { return; }

        // Confirmation Dialog
        let confirm = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Info)
            .set_title("Confirm Move")
            .set_description(format!(
                "Move {} archive file(s) to:\n{}",
                total,
                target_dir.display()
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();

        if confirm != rfd::MessageDialogResult::Yes {
            return;
        }

        self.archive_files_moving = true;

        // Reset progress
        if let Ok(mut p) = self.archive_files_move_progress.lock() {
            *p = (0, total, String::new());
        }

        let (tx, rx) = std::sync::mpsc::channel();
        self.archive_files_move_rx = Some(rx);

        let target_dir = target_dir.clone();
        let progress = std::sync::Arc::clone(&self.archive_files_move_progress);

        std::thread::spawn(move || {
            let mut success_count = 0;
            let mut fail_count = 0;

            for (idx, p) in to_move.iter().enumerate() {
                // Update progress: which file we're on
                let fname = p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Ok(mut prog) = progress.lock() {
                    *prog = (idx, total, fname.clone());
                }

                if !p.is_file() {
                    fail_count += 1;
                    continue;
                }

                let dest = unique_file_dest(&target_dir, p);
                let moved = fast_move(p, &dest).is_ok();
                if moved { success_count += 1; } else { fail_count += 1; }
            }

            // Mark done
            if let Ok(mut prog) = progress.lock() {
                *prog = (total, total, String::new());
            }

            let _ = tx.send((success_count, fail_count));
        });
    }

    // -----------------------------------------------------------------------
    // Queue (staged move) operations
    // -----------------------------------------------------------------------
}

/// A background worker ended without sending its result (it panicked). Tell the user.
fn worker_lost_dialog(what: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Something went wrong")
        .set_description(format!("{what} stopped unexpectedly. Nothing else was changed. Please try again."))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
