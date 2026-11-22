use crate::types::*;
use crate::app::state::{ExplorerState, QueueState, DuplicatesState, ScanState, UIState};
use crate::types::{OperationHistory, AppSettings};
use crate::ui::NotificationManager;
use crate::constants::*;
use crate::ui::{
    progress_panel, results_dup_list_panel, results_main_panel, scan_panel,
};
use eframe::egui::{self, TextureHandle};
use sortwise::cache::HashCache;
use sortwise::cancel::CancellationToken;
use sortwise::pipeline::{refresh_duplicates_from_result, run_dedup_streaming};
use sortwise::scanner::parallel_scan_roots;
use sortwise::types::{
    ActionMode, FolderConfig, FolderProgress, FolderStatus, IoProfile, ScanConfig,
    ScanProgress, ScanResult,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
// Re-export app types for UI access
pub use crate::types::{AppView, AppMode, ExplorerSortKey, ExplorerViewMode, ExplorerFilesSort, ExplorerTypeFilter, ExplorerEntry, FileQueue, MovableItem, GroupOutcome, ScanWorkKind, DupGroupSortOrder, ThumbnailRequest, SnapshotPlan, ThumbnailResponse, PendingAction, ExtractBanner, DupListCache};


pub struct SortwiseApp {
    pub explorer: ExplorerState,
    pub queue: QueueState,
    pub duplicates: DuplicatesState,
    pub scan: ScanState,
    pub ui: UIState,
    
    // Phase 3: Operation history, settings, notifications
    pub operation_history: OperationHistory,
    pub settings: AppSettings,
    pub notifications: NotificationManager,
}


impl Default for SortwiseApp {
    fn default() -> Self {
        Self {
            explorer: ExplorerState::default(),
            queue: QueueState::default(),
            duplicates: DuplicatesState::default(),
            scan: ScanState::default(),
            ui: UIState::default(),
            operation_history: OperationHistory::new(500),
            settings: AppSettings::load().unwrap_or_default(),
            notifications: NotificationManager::new(),
        }
    }
}

impl Drop for SortwiseApp {
    fn drop(&mut self) {
        let _ = self.settings.save();
    }
}


impl SortwiseApp {
    fn build_scan_config(&self) -> ScanConfig {
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
    fn poll_live_groups(&mut self) {
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

    fn poll_result(&mut self) {
        if let Some(rx) = &self.result_rx {
            if let Ok(res) = rx.try_recv() {
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

    fn poll_empty_folders_result(&mut self) {
        if let Some(rx) = &self.empty_folders_rx {
            if let Ok(res) = rx.try_recv() {
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

    fn poll_archive_files_result(&mut self) {
        if let Some(rx) = &self.archive_files_rx {
            if let Ok(res) = rx.try_recv() {
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
        crate::operations::poll_archive_files_move_result(self);
    }

    pub fn execute_move_archive_files(&mut self) {
        crate::operations::execute_move_archive_files(self);
    }

    // -----------------------------------------------------------------------
    // Queue (staged move) operations
    // -----------------------------------------------------------------------

    /// Prompt for one or more destination folders and create new queues named after them.
    pub fn explorer_new_queue(&mut self) {
        crate::operations::explorer_new_queue(self);
    }

    /// Stage a file or folder into queue[idx] (deduped by path).
    pub fn explorer_add_to_queue(&mut self, idx: usize, path: PathBuf) {
        crate::operations::explorer_add_to_queue(self, idx, path);
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
        crate::operations::explorer_toggle_queue(self, idx, path);
    }

    /// Remove a staged file from queue[idx] (does not touch the real file).
    pub fn explorer_remove_from_queue(&mut self, idx: usize, path: PathBuf) {
        crate::operations::explorer_remove_from_queue(self, idx, path);
    }

    /// Delete a queue entirely (staging list only; never deletes real files).
    pub fn explorer_delete_queue(&mut self, idx: usize) {
        crate::operations::explorer_delete_queue(self, idx);
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
        crate::operations::is_in_movable(self, path)
    }

    /// Stage every given file that is not staged yet. Already-staged paths are left alone.
    pub fn stage_paths(&mut self, paths: &[PathBuf]) -> usize {
        crate::operations::stage_paths(self, paths)
    }

    pub fn toggle_movable(&mut self, path: PathBuf) {
        crate::operations::toggle_movable(self, path);
    }

    pub fn remove_from_movable(&mut self, idx: usize) {
        crate::operations::remove_from_movable(self, idx);
    }

    pub fn clear_movable_items(&mut self) {
        crate::operations::clear_movable_items(self);
    }

    /// Move a single item from the movable queue to its assigned target queue folder.
    /// Preserves existing files: uses unique_file_dest and unique_dir_dest to increment
    /// filename if destination file already exists.
    pub fn move_single_movable_item(&mut self, idx: usize) -> bool {
        crate::operations::move_single_movable_item(self, idx)
    }

    /// Move all items in the movable queue that have a valid target queue assigned.
    pub fn move_all_movable_items(&mut self) -> (usize, usize) {
        crate::operations::move_all_movable_items(self)
    }

    /// Re-pick the destination folder for queue[idx].
    pub fn explorer_change_queue_target(&mut self, idx: usize) {
        crate::operations::explorer_change_queue_target(self, idx);
    }


    /// Move all staged items across ALL queues into their respective target folders in one batch.
    pub fn submit_all_queues(&mut self) {
        crate::operations::submit_all_queues(self);
    }

    /// Poll the background submit thread; on completion clear moved items and
    /// refresh the listing. Mirrors `poll_archive_files_move_result`.
    pub fn poll_queue_submit_result(&mut self) {
        crate::operations::poll_queue_submit_result(self);
    }

    pub fn explorer_navigate(&mut self, path: PathBuf) {
        if path == self.explorer_dir {
            return;
        }
        self.explorer_cancel_search();
        self.explorer_close_preview();
        self.explorer_back.push(self.explorer_dir.clone());
        self.explorer_fwd.clear();
        self.explorer_dir = path;
        self.explorer_selected = None;
        self.explorer_multi.clear();
        self.explorer_anchor = None;
        self.explorer_filter.clear();
        self.explorer_last_filter.clear();
        self.explorer_loaded_dir = None;
    }

    pub fn explorer_go_back(&mut self) {
        if let Some(prev) = self.explorer_back.pop() {
            self.explorer_cancel_search();
            self.explorer_close_preview();
            self.explorer_fwd.push(self.explorer_dir.clone());
            self.explorer_dir = prev;
            self.explorer_selected = None;
        self.explorer_multi.clear();
        self.explorer_anchor = None;
            self.explorer_filter.clear();
            self.explorer_last_filter.clear();
            self.explorer_loaded_dir = None;
        }
    }

    pub fn explorer_go_forward(&mut self) {
        if let Some(next) = self.explorer_fwd.pop() {
            self.explorer_cancel_search();
            self.explorer_close_preview();
            self.explorer_back.push(self.explorer_dir.clone());
            self.explorer_dir = next;
            self.explorer_selected = None;
        self.explorer_multi.clear();
        self.explorer_anchor = None;
            self.explorer_filter.clear();
            self.explorer_last_filter.clear();
            self.explorer_loaded_dir = None;
        }
    }

    pub fn explorer_go_up(&mut self) {
        if let Some(parent) = self.explorer_dir.parent().map(|p| p.to_path_buf()) {
            self.explorer_navigate(parent);
        }
    }

    /// Stop any open preview/player cleanly without dropping the MPV render context.
    pub fn explorer_close_preview(&mut self) {
        self.explorer_preview_open = false;
        self.explorer_preview_path = None;
        self.preview_path = None;
        self.preview_texture = None;
        self.preview_scroll_text = None;
        self.preview_subtitle = None;
        self.inline_video = None;
        #[cfg(feature = "mpv")]
        {
            if let Some(p) = self.mpv.as_ref() {
                p.stop();
            }
            self.mpv_loaded_path = None;
        }
    }

    /// Cancel any in-flight recursive search and leave search mode.
    pub fn explorer_cancel_search(&mut self) {
        self.explorer_abort_in_flight_search();
        self.explorer_is_recursive_search = false;
        self.explorer_search_pending = false;
        self.explorer_search_typed_at = None;
        self.explorer_search_checked.clear();
    }

    /// Stop a running search worker without clearing displayed recursive results.
    fn explorer_abort_in_flight_search(&mut self) {
        if let Some(token) = self.explorer_search_token.take() {
            token.cancel();
        }
        self.explorer_search_rx = None;
        self.explorer_searching = false;
    }

    /// Check every collapsed search result (Select All).
    pub fn explorer_search_select_all(&mut self) {
        self.explorer_search_checked = self
            .explorer_entries
            .iter()
            .map(|e| e.path.clone())
            .collect();
    }

    pub fn explorer_search_clear_checks(&mut self) {
        self.explorer_search_checked.clear();
    }

    /// Stage all checked search units into queue[idx]. Files are staged as-is;
    /// folders are expanded to media/archive files under them (submit is file-only).
    pub fn explorer_add_checked_to_queue(&mut self, idx: usize) {
        if self.explorer_queues.get(idx).is_none() {
            return;
        }
        let checked: Vec<PathBuf> = self.explorer_search_checked.iter().cloned().collect();
        if checked.is_empty() {
            self.explorer_status = Some("Nothing selected to add.".to_string());
            return;
        }

        let show_hidden = self.explorer_show_hidden;
        let mut to_stage: Vec<PathBuf> = Vec::new();
        let mut folders_expanded = 0usize;
        for path in checked {
            let is_dir = self
                .explorer_entries
                .iter()
                .find(|e| e.path == path)
                .map(|e| e.is_dir)
                .unwrap_or_else(|| path.is_dir());
            if is_dir {
                folders_expanded += 1;
                to_stage.extend(collect_media_files_under(&path, show_hidden));
            } else {
                to_stage.push(path);
            }
        }

        let Some(q) = self.explorer_queues.get_mut(idx) else { return };
        let qname = q.name.clone();
        let mut added = 0usize;
        let mut skipped = 0usize;
        for path in to_stage {
            if q.items.contains(&path) {
                skipped += 1;
            } else {
                q.items.push(path);
                added += 1;
            }
        }

        self.explorer_status = Some(if folders_expanded > 0 {
            format!(
                "Added {added} file(s) to “{qname}” (from {folders_expanded} folder(s){}).",
                if skipped > 0 {
                    format!(", {skipped} already queued")
                } else {
                    String::new()
                }
            )
        } else {
            format!(
                "Added {added} file(s) to “{qname}”{}.",
                if skipped > 0 {
                    format!(" ({skipped} already queued)")
                } else {
                    String::new()
                }
            )
        });
    }

    /// Spawn a background thread that uses `rg` (with walkdir fallback) to
    /// recursively find images, videos, audio, archives and folders whose names
    /// contain the current filter query.  Results replace `explorer_entries`.
    pub fn explorer_search_recursive(&mut self) {
        self.explorer_abort_in_flight_search();
        self.explorer_search_pending = false;
        self.explorer_search_typed_at = None;

        let token = CancellationToken::new();
        self.explorer_search_token = Some(token.clone_token());
        self.explorer_searching = true;
        self.explorer_is_recursive_search = true;

        let root = self.explorer_dir.clone();
        let query = self.explorer_filter.trim().to_lowercase();
        let show_hidden = self.explorer_show_hidden;
        let (tx, rx): (Sender<Vec<ExplorerEntry>>, Receiver<Vec<ExplorerEntry>>) =
            std::sync::mpsc::channel();
        self.explorer_search_rx = Some(rx);

        std::thread::spawn(move || {
            // -----------------------------------------------------------------
            // rg glob — exhaustive union of every extension category() handles
            // -----------------------------------------------------------------
            const MEDIA_GLOB: &str = "*.{png,jpg,jpeg,gif,webp,bmp,tiff,heic,\
 mp4,mov,m4v,webm,mkv,avi,wmv,mpg,mpeg,\
 mp3,wav,flac,aac,ogg,m4a,\
 zip,rar,7z,tar,gz,bz2,xz}";

            let mut entries: Vec<ExplorerEntry> = Vec::new();

            // -----------------------------------------------------------------
            // 1. File search via rg --files
            // -----------------------------------------------------------------
            let rg_result = std::process::Command::new("rg")
                .arg("--files")
                .args(if show_hidden { vec!["--hidden"] } else { vec![] })
                .args(["--glob", MEDIA_GLOB, "--"])
                .arg(&root)
                .output();

            match rg_result {
                Ok(output) if output.status.success() || !output.stdout.is_empty() => {
                    for line in output.stdout.split(|&b| b == b'\n') {
                        if token.is_cancelled() {
                            return;
                        }
                        let path_str = match std::str::from_utf8(line) {
                            Ok(s) => s.trim(),
                            Err(_) => continue,
                        };
                        if path_str.is_empty() {
                            continue;
                        }
                        let path = std::path::PathBuf::from(path_str);
                        // Filter by query against file name only
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        if !query.is_empty() && !name.contains(&query) {
                            continue;
                        }
                        if let Ok(meta) = std::fs::metadata(&path) {
                            entries.push(ExplorerEntry {
                                name: path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default(),
                                path: path.clone(),
                                is_dir: false,
                                size: meta.len(),
                                modified: meta.modified().ok(),
                            });
                        }
                    }
                }
                _ => {
                    // -----------------------------------------------------------
                    // Fallback: walkdir with manual extension filter
                    // -----------------------------------------------------------
                    for entry in walkdir::WalkDir::new(&root)
                        .follow_links(false)
                        .into_iter()
                        .filter_entry(|e| {
                            if token.is_cancelled() {
                                return false;
                            }
                            let n = e.file_name().to_string_lossy();
                            if is_system_entry(&n, e.file_type().is_dir()) {
                                return false;
                            }
                            if !show_hidden && n.starts_with('.') {
                                return false;
                            }
                            true
                        })
                        .flatten()
                    {
                        if token.is_cancelled() {
                            return;
                        }
                        if !entry.file_type().is_file() {
                            continue;
                        }
                        let path = entry.path();
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|s| s.to_lowercase())
                            .unwrap_or_default();
                        if !MEDIA_EXTS.contains(&ext.as_str()) {
                            continue;
                        }
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        if !query.is_empty() && !name.contains(&query) {
                            continue;
                        }
                        if let Ok(meta) = entry.metadata() {
                            entries.push(ExplorerEntry {
                                name: path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default(),
                                path: path.to_path_buf(),
                                is_dir: false,
                                size: meta.len(),
                                modified: meta.modified().ok(),
                            });
                        }
                    }
                }
            }

            // -----------------------------------------------------------------
            // 2. Folder search via walkdir (rg --files skips directories)
            // -----------------------------------------------------------------
            for entry in walkdir::WalkDir::new(&root)
                .follow_links(false)
                .min_depth(1)
                .into_iter()
                .filter_entry(|e| {
                    if token.is_cancelled() {
                        return false;
                    }
                    let n = e.file_name().to_string_lossy();
                    if is_system_entry(&n, e.file_type().is_dir()) {
                        return false;
                    }
                    if !show_hidden && n.starts_with('.') {
                        return false;
                    }
                    true
                })
                .flatten()
            {
                if token.is_cancelled() {
                    return;
                }
                if !entry.file_type().is_dir() {
                    continue;
                }
                let path = entry.path();
                let name_str = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !query.is_empty() && !name_str.contains(&query) {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    entries.push(ExplorerEntry {
                        name: path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        path: path.to_path_buf(),
                        is_dir: true,
                        size: 0,
                        modified: meta.modified().ok(),
                    });
                }
            }

            if !token.is_cancelled() {
                entries.retain(|e| !is_system_entry(&e.name, e.is_dir));
                let collapsed = collapse_search_results(entries);
                let _ = tx.send(collapsed);
            }
        });
    }

    /// Re-read the current directory if it changed (cheap no-op otherwise).
    /// Files view: start a background recursive listing when the folder changes, and
    /// collect a finished listing. Call once per frame while the Files view is shown.
    pub fn explorer_files_ensure(&mut self) {
        use std::sync::mpsc::TryRecvError;
        if let Some(rx) = &self.explorer_files_rx {
            match rx.try_recv() {
                Ok(listing) => {
                    self.explorer_files = listing.files;
                    self.explorer_files_truncated = listing.truncated;
                    self.explorer_files_loading = false;
                    self.explorer_files_rx = None;
                    self.explorer_files_gen += 1;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.explorer_files_loading = false;
                    self.explorer_files_rx = None;
                }
            }
        }
        if self.explorer_files_root.as_ref() == Some(&self.explorer_dir) {
            return;
        }
        if let Some(old) = self.explorer_files_cancel.take() {
            old.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let root = self.explorer_dir.clone();
        let skip_system = self.skip_system_dirs;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut listing = sortwise::recursive_files::list_files(
                &root,
                skip_system,
                sortwise::recursive_files::DEFAULT_CAP,
                &worker_cancel,
            );
            sortwise::recursive_files::sort_by_size_desc(&mut listing.files);
            let _ = tx.send(listing);
        });
        self.explorer_files_root = Some(self.explorer_dir.clone());
        self.explorer_files_cancel = Some(cancel);
        self.explorer_files_rx = Some(rx);
        self.explorer_files_loading = true;
        self.explorer_files.clear();
        self.explorer_files_truncated = false;
        self.explorer_files_checked.clear();
        self.explorer_files_gen += 1;
    }

    /// Files view: put files into the Movable Queue with `queue` as destination.
    /// Already-staged files only have their destination changed. Moves nothing on disk.
    pub fn explorer_files_stage(&mut self, paths: &[PathBuf], queue: Option<String>) -> usize {
        let mut added = 0;
        for path in paths {
            if let Some(item) = self.movable_items.iter_mut().find(|i| &i.path == path) {
                item.target_queue = queue.clone();
            } else {
                self.movable_items.push(MovableItem {
                    path: path.clone(),
                    target_queue: queue.clone(),
                });
                added += 1;
            }
        }
        if queue.is_some() {
            self.movable_last_chosen_queue = queue.clone();
        }
        let dest = queue.map(|q| format!(" to “{q}”")).unwrap_or_default();
        self.explorer_status = Some(format!("Queued {} file(s){dest}.", paths.len()));
        added
    }

    pub fn explorer_ensure_loaded(&mut self) {
        if self.explorer_loaded_dir.as_ref() == Some(&self.explorer_dir) {
            return;
        }
        self.explorer_reload();
    }

    pub fn explorer_reload(&mut self) {
        self.navigation_epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.thumbnail_loading.clear();
        self.explorer_error = None;
        let mut entries = Vec::new();
        match std::fs::read_dir(&self.explorer_dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    let meta = e.metadata().ok();
                    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    // Always include hidden; never include known system junk.
                    if is_system_entry(&name, is_dir) {
                        continue;
                    }
                    // In Folders mode, always include hidden child folders; in other modes respect explorer_show_hidden.
                    if !self.explorer_show_hidden && self.explorer_view_mode != ExplorerViewMode::Folders && name.starts_with('.') {
                        continue;
                    }
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    let modified = meta.as_ref().and_then(|m| m.modified().ok());
                    entries.push(ExplorerEntry {
                        name,
                        path: e.path(),
                        is_dir,
                        size,
                        modified,
                    });
                }
            }
            Err(err) => {
                self.explorer_error = Some(err.to_string());
            }
        }
        self.explorer_entries = entries;
        self.explorer_loaded_dir = Some(self.explorer_dir.clone());
        self.explorer_sort_entries();
    }

    pub fn explorer_sort_entries(&mut self) {
        let key = self.explorer_sort_key;
        let asc = self.explorer_sort_asc;
        self.explorer_entries.sort_by(|a, b| {
            // Folders always group before files.
            if a.is_dir != b.is_dir {
                return b.is_dir.cmp(&a.is_dir);
            }
            let ord = match key {
                ExplorerSortKey::Name => a.name.chars().map(|c| c.to_ascii_lowercase()).cmp(b.name.chars().map(|c| c.to_ascii_lowercase())),
                ExplorerSortKey::Size => a.size.cmp(&b.size),
                ExplorerSortKey::Modified => a.modified.cmp(&b.modified),
                ExplorerSortKey::Kind => {
                    let ea = a.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let eb = b.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    ea.chars().map(|c| c.to_ascii_lowercase()).cmp(eb.chars().map(|c| c.to_ascii_lowercase()))
                }
            };
            if asc {
                ord
            } else {
                ord.reverse()
            }
        });
    }

    pub fn explorer_set_sort(&mut self, key: ExplorerSortKey) {
        if self.explorer_sort_key == key {
            self.explorer_sort_asc = !self.explorer_sort_asc;
        } else {
            self.explorer_sort_key = key;
            self.explorer_sort_asc = true;
        }
        self.explorer_sort_entries();
    }

    /// Create a uniquely-named new folder in the current directory.
    pub fn explorer_new_folder(&mut self) {
        let base = self.explorer_dir.clone();
        let mut target = base.join("New Folder");
        let mut n = 2;
        while target.exists() {
            target = base.join(format!("New Folder {n}"));
            n += 1;
        }
        match std::fs::create_dir(&target) {
            Ok(()) => {
                self.explorer_status = Some(format!(
                    "Created {}",
                    target.file_name().unwrap_or_default().to_string_lossy()
                ));
                self.explorer_reload();
            }
            Err(e) => self.explorer_status = Some(format!("Could not create folder: {e}")),
        }
    }

    /// Extract a `.zip` into a sibling folder. On success, queues a prompt to
    /// move the original archive to the Trash.
    pub fn explorer_extract_zip(&mut self, zip: &std::path::Path) {
        let parent = zip.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| self.explorer_dir.clone());
        let stem = zip
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "extracted".to_string());
        let mut dest = parent.join(&stem);
        let mut n = 2;
        while dest.exists() {
            dest = parent.join(format!("{stem} ({n})"));
            n += 1;
        }
        if let Err(e) = std::fs::create_dir_all(&dest) {
            self.explorer_status = Some(format!("Extract failed: {e}"));
            return;
        }

        #[cfg(windows)]
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                    zip.display(),
                    dest.display()
                ),
            ])
            .status();
        #[cfg(not(windows))]
        let status = std::process::Command::new("/usr/bin/unzip")
            .arg("-o")
            .arg(zip)
            .arg("-d")
            .arg(&dest)
            .status();

        match status {
            Ok(s) if s.success() => {
                self.explorer_status = None;
                self.extract_banner = Some(ExtractBanner {
                    zip: zip.to_path_buf(),
                    dest: dest.clone(),
                    dir: self.explorer_dir.clone(),
                });
                self.explorer_reload();
            }
            Ok(s) => {
                self.explorer_status = Some(format!("Extract failed (exit {:?}).", s.code()));
            }
            Err(e) => {
                self.explorer_status = Some(format!("Extract failed: {e}"));
            }
        }
    }

    pub fn explorer_open_preview(&mut self, path: PathBuf) {
        self.clear_preview_cache();
        if let Some(q_idx) = self.explorer_find_queue_for_path(&path) {
            self.explorer_preview_queue_pick = Some(q_idx);
        }
        self.explorer_preview_path = Some(path);
        self.explorer_preview_open = true;
        self.explorer_preview_rotation = 0;
    }

    pub fn folder_modal_open_dir(&mut self, folder: PathBuf) {
        self.folder_modal_path = Some(folder);
        self.folder_modal_open = true;
        self.folder_modal_filter.clear();
        self.folder_modal_selected = None;
        self.folder_modal_reload();
    }

    pub fn folder_modal_close(&mut self) {
        self.folder_modal_open = false;
        self.folder_modal_path = None;
        self.folder_modal_entries.clear();
        self.folder_modal_loaded_path = None;
        self.folder_modal_filter.clear();
        self.folder_modal_selected = None;
        self.close_explorer_preview();
    }

    pub fn folder_modal_reload(&mut self) {
        let Some(path) = self.folder_modal_path.clone() else { return };
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&path) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let meta = e.metadata().ok();
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                if is_system_entry(&name, is_dir) {
                    continue;
                }
                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = meta.as_ref().and_then(|m| m.modified().ok());
                entries.push(ExplorerEntry {
                    name,
                    path: e.path(),
                    is_dir,
                    size,
                    modified,
                });
            }
        }
        entries.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            }
        });
        self.folder_modal_entries = entries;
        self.folder_modal_loaded_path = Some(path);
    }

    pub fn folder_modal_prev(&mut self) {
        let Some(cur_path) = self.folder_modal_path.clone() else { return };
        let folders: Vec<PathBuf> = self
            .explorer_entries
            .iter()
            .filter(|e| e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        if let Some(pos) = folders.iter().position(|p| p == &cur_path) {
            if pos > 0 {
                self.folder_modal_open_dir(folders[pos - 1].clone());
            }
        }
    }

    pub fn folder_modal_next(&mut self) {
        let Some(cur_path) = self.folder_modal_path.clone() else { return };
        let folders: Vec<PathBuf> = self
            .explorer_entries
            .iter()
            .filter(|e| e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        if let Some(pos) = folders.iter().position(|p| p == &cur_path) {
            if pos + 1 < folders.len() {
                self.folder_modal_open_dir(folders[pos + 1].clone());
            }
        }
    }

    pub fn folder_modal_open_in_grid(&mut self) {
        if let Some(path) = self.folder_modal_path.clone() {
            self.folder_modal_close();
            self.explorer_navigate(path);
        }
    }

    /// Replace the scan result. Bumps the results epoch and drops any pending
    /// bulk action, since its plan was computed against the old result.
    pub fn set_last_result(&mut self, result: Option<ScanResult>) {
        self.last_result = result.map(Arc::new);
        self.results_epoch = self.results_epoch.wrapping_add(1);
        self.pending_action = None;
    }

    pub fn clear_preview_cache(&mut self) {
        self.preview_path = None;
        self.preview_rotation = 0;
        self.preview_texture = None;
        self.preview_scroll_text = None;
        self.preview_subtitle = None;
        self.inline_video = None;
        #[cfg(feature = "mpv")]
        {
            if let Some(p) = self.mpv.as_ref() {
                p.stop();
            }
        }
        self.mpv_loaded_path = None;
        // Do not touch explorer_preview_open / explorer_preview_path —
        // those close the modal; callers that want dismiss set them explicitly.
    }

    pub fn close_explorer_preview(&mut self) {
        self.explorer_preview_open = false;
        self.explorer_preview_path = None;
        self.explorer_preview_rotation = 0;
        self.clear_preview_cache();
    }
}


impl eframe::App for SortwiseApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(plan) = self.snapshot.as_mut() {
            // Let layout and fonts settle, then ask egui for its own rendered frame.
            plan.frames += 1;
            if plan.frames >= 40 && !plan.requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                plan.requested = true;
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                let [w, h] = image.size;
                match image::save_buffer(&plan.path, &bytes, w as u32, h as u32, image::ColorType::Rgba8) {
                    Ok(()) => eprintln!("snapshot written: {}", plan.path.display()),
                    Err(e) => eprintln!("snapshot failed: {e}"),
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
        // Persist scan preferences when they change (U16). A failed write is reported once,
        // and the attempt is not repeated every frame.
        let current = self.current_settings();
        if current != self.saved_settings {
            if let Some(path) = sortwise::settings::settings_path() {
                if let Err(e) = sortwise::settings::save_to(&path, &current) {
                    self.apply_errors = Some(format!("Could not save settings: {e}"));
                }
            }
            self.saved_settings = current;
        }

        // Poll background thumbnails (batched to max 16 per frame)
        let mut upload_count = 0;
        let mut got_any = false;
        let cur_epoch = self.navigation_epoch.load(std::sync::atomic::Ordering::Relaxed);
        while upload_count < 16 {
            if let Ok(resp) = self.thumbnail_response_rx.try_recv() {
                self.thumbnail_loading.remove(&resp.path);
                if resp.epoch == cur_epoch {
                    if let Some(img) = resp.image {
                        let id = format!("sfm_thumb_{}", resp.path.display());
                        let handle = ctx.load_texture(id, img, egui::TextureOptions::LINEAR);
                        // In-memory LRU cap: keep max 1,000 textures
                        if self.thumbnail_cache.len() >= 1000 {
                            let keys_to_remove: Vec<PathBuf> =
                                self.thumbnail_cache.keys().take(100).cloned().collect();
                            for k in keys_to_remove {
                                self.thumbnail_cache.remove(&k);
                            }
                        }
                        self.thumbnail_cache.insert(resp.path, handle);
                        upload_count += 1;
                        got_any = true;
                    } else {
                        self.thumbnail_failed.insert(resp.path);
                    }
                }
            } else {
                break;
            }
        }
        if got_any {
            ctx.request_repaint();
        }

        self.poll_archive_files_move_result();
        self.poll_queue_submit_result();

        // Keep repainting while a move is in progress so the progress bar updates
        if self.archive_files_moving || self.explorer_queue_submitting {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Poll the duplicate scan whenever it runs, even if the user has opened the
        // partial results view (P-6), so the final result is never missed.
        self.poll_live_groups();
        if self.result_rx.is_some() {
            self.poll_result();
        }
        if self.view == AppView::Scanning {
            self.poll_empty_folders_result();
            self.poll_archive_files_result();
        }
        if self.mode == AppMode::Explorer {
            self.explorer_ensure_loaded();
        }

        // Poll recursive search results
        if self.explorer_searching {
            let got = if let Some(rx) = &self.explorer_search_rx {
                rx.try_recv().ok()
            } else {
                None
            };
            if let Some(entries) = got {
                self.explorer_entries = entries;
                self.explorer_sort_entries();
                self.explorer_searching = false;
                self.explorer_search_rx = None;
                self.explorer_search_select_all();
                ctx.request_repaint();
            } else {
                // Keep UI responsive while the worker runs.
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        // Debounced recursive search: fire only after 2s idle since last keystroke.
        if self.mode == AppMode::Explorer {
            let filter_changed = self.explorer_filter != self.explorer_last_filter;
            self.explorer_last_filter = self.explorer_filter.clone();
            // Files view filters in memory; it must not start a disk-wide recursive search.
            if filter_changed && self.explorer_view_mode != ExplorerViewMode::Files {
                if self.explorer_filter.trim().is_empty() {
                    self.explorer_cancel_search();
                    self.explorer_reload();
                } else {
                    // Abort in-flight work; wait for idle before scanning again.
                    self.explorer_abort_in_flight_search();
                    self.explorer_search_pending = true;
                    self.explorer_search_typed_at = Some(Instant::now());
                }
            }

            if self.explorer_search_pending {
                if let Some(typed_at) = self.explorer_search_typed_at {
                    let elapsed = typed_at.elapsed();
                    if elapsed >= SEARCH_DEBOUNCE {
                        if !self.explorer_filter.trim().is_empty() {
                            self.explorer_search_recursive();
                        } else {
                            self.explorer_search_pending = false;
                            self.explorer_search_typed_at = None;
                        }
                    } else {
                        ctx.request_repaint_after(SEARCH_DEBOUNCE - elapsed);
                    }
                }
            }
        }

        let mode = self.mode;
        egui::TopBottomPanel::top("top")
            .exact_height(52.0)
            .frame(
                egui::Frame::none()
                    .fill(crate::ui::style::pal().surface)
                    .inner_margin(egui::Margin::symmetric(18.0, 0.0))
                    .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if self.brand_logo.is_none() {
                        self.brand_logo = crate::ui::brand::load_logo(ui.ctx());
                    }
                    match self.brand_logo.as_ref() {
                        Some(logo) => {
                            ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(24.0, 24.0)));
                        }
                        None => {
                            ui.label(egui::RichText::new(crate::ui::icons::Icons::FOLDER_OPEN).size(20.0));
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Sortwise")
                            .strong()
                            .size(16.0)
                            .color(crate::ui::style::pal().text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        mode_switch(self, ui);
                        if mode == AppMode::Explorer {
                            ui.add_space(10.0);
                            // One entry point for everything set aside (U8): loose items and folder queues.
                            let movable_count = self.movable_items.len();
                            let staged_folder: usize = self.explorer_queues.iter().map(|q| q.items.len()).sum();
                            let staged_total = movable_count + staged_folder;
                            let open = self.movable_queue_open || self.explorer_queue_view_open;
                            let label = if staged_total > 0 {
                                format!("{}  Staging · {staged_total}", crate::ui::icons::Icons::INBOX)
                            } else {
                                format!("{}  Staging", crate::ui::icons::Icons::INBOX)
                            };
                            let fill = if open {
                                crate::ui::style::pal().accent
                            } else if staged_total > 0 {
                                crate::ui::style::pal().success_soft
                            } else {
                                crate::ui::style::pal().surface_raised
                            };
                            let text_color = if open {
                                crate::ui::style::pal().on_accent
                            } else if staged_total > 0 {
                                crate::ui::style::pal().success_on_soft
                            } else {
                                crate::ui::style::pal().text
                            };
                            let btn = egui::Button::new(
                                egui::RichText::new(label).strong().size(13.5).color(text_color),
                            )
                            .fill(fill)
                            .min_size(egui::vec2(0.0, 32.0));
                            if ui
                                .add(btn)
                                .on_hover_text("Files set aside before moving: loose items and folder queues")
                                .clicked()
                            {
                                if open {
                                    self.movable_queue_open = false;
                                    self.explorer_queue_view_open = false;
                                } else {
                                    // Opens on Folder queues, the first tab.
                                    self.explorer_queue_view_open = true;
                                }
                            }
                        }
                    });
                });
            });

        // Status bar (26 px). Added before the side and central panels, as egui requires.
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(
                egui::Frame::none()
                    .fill(crate::ui::style::pal().surface)
                    .inner_margin(egui::Margin::symmetric(12.0, 0.0))
                    .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border)),
            )
            .show(ctx, |ui| crate::ui::status_bar::status_bar(self, ui));

        let results_ready =
            (mode == AppMode::Dedupe || mode == AppMode::CompareSourceDest)
                && matches!(self.view, AppView::Results)
                && self.last_result.is_some();

        let show_sidebar = match mode {
            AppMode::Explorer => true,
            AppMode::Dedupe | AppMode::CompareSourceDest => results_ready,
            AppMode::EmptyFolders => false,
            AppMode::ArchiveFiles => false,
        };

        if show_sidebar {
            // Explorer: 200 px places panel, icons only in narrow windows (< 760 px).
            // The sidebar also collapses while the staging panel is open, so the toolbar keeps its room.
            let staging_open = self.movable_queue_open || self.explorer_queue_view_open;
            let explorer_narrow =
                mode == AppMode::Explorer && (ctx.screen_rect().width() < 760.0 || staging_open);
            let sidebar = egui::SidePanel::left("sidebar");
            let sidebar = if explorer_narrow {
                sidebar.resizable(false).exact_width(56.0)
            } else {
                sidebar
                    .resizable(true)
                    .default_width(if results_ready { 300.0 } else if mode == AppMode::Explorer { 200.0 } else { 220.0 })
                    .min_width(if results_ready { 236.0 } else if mode == AppMode::Explorer { 160.0 } else { 180.0 })
                    .max_width(560.0)
            };
            sidebar
                .frame(
                    egui::Frame::none()
                        .fill(crate::ui::style::pal().canvas)
                        .inner_margin(egui::Margin::same(if results_ready { 14.0 } else { 10.0 }))
                        .stroke(egui::Stroke::new(1.0, crate::ui::style::pal().border)),
                )
                .show(ctx, |ui| {
                    match mode {
                        AppMode::Explorer => crate::ui::explorer_sidebar(self, ui),
                        AppMode::Dedupe | AppMode::CompareSourceDest if results_ready => {
                            results_dup_list_panel(self, ui, ctx);
                        }
                        _ => {}
                    }
                });
        }

        // Staging panel: a right side panel, so it must be added before the central panel.
        crate::ui::staging_panel(self, ctx);

        let central_margin = if mode == AppMode::Explorer {
            egui::Margin::symmetric(12.0, 10.0)
        } else {
            egui::Margin::symmetric(22.0, 18.0)
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(crate::ui::style::pal().canvas)
                    .inner_margin(central_margin),
            )
            .show(ctx, |ui| {
                match mode {
                    AppMode::Explorer => crate::ui::explorer_main_panel(self, ui),
                    AppMode::Dedupe | AppMode::CompareSourceDest => match self.view {
                        AppView::Scan => scan_panel(self, ui),
                        AppView::Scanning => progress_panel(self, ui, ctx),
                        AppView::Results => results_main_panel(self, ui, ctx),
                    },
                    AppMode::EmptyFolders => match self.view {
                        AppView::Scan => crate::ui::empty_folders_scan_panel(self, ui),
                        AppView::Scanning => crate::ui::empty_folders_progress_panel(self, ui, ctx),
                        AppView::Results => crate::ui::empty_folders_results_panel(self, ui),
                    },
                    AppMode::ArchiveFiles => match self.view {
                        AppView::Scan => crate::ui::archive_files_scan_panel(self, ui),
                        AppView::Scanning => crate::ui::archive_files_progress_panel(self, ui, ctx),
                        AppView::Results => crate::ui::archive_files_results_panel(self, ui),
                    },
                }
                // Render global player modal globally so it functions in all modes/views
                crate::ui::folder_contents_modal(self, ctx);
                crate::ui::player_window(self, ctx);
                crate::ui::confirm_pending_action(self, ctx);
            });
        
        // Render Phase 3 notifications (toasts)
        self.notifications.render(ctx);
    }
}


// Helper functions extracted to src/helpers.rs
pub use crate::helpers::*;
