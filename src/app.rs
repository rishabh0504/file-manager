use crate::ui::{
    progress_panel, results_dup_list_panel, results_main_panel, scan_panel,
};
use eframe::egui::{self, TextureHandle};
use smart_file_manager::cache::HashCache;
use smart_file_manager::cancel::CancellationToken;
use smart_file_manager::pipeline::{refresh_duplicates_from_result, run_dedup};
use smart_file_manager::scanner::parallel_scan_roots;
use smart_file_manager::types::{
    ActionMode, FolderConfig, FolderProgress, FolderStatus, IoProfile, ScanConfig,
    ScanProgress, ScanResult,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Idle time after the last keystroke before recursive search fires.
const SEARCH_DEBOUNCE: Duration = Duration::from_secs(2);

/// Media / archive extensions used by recursive search and folder→queue expand.
const MEDIA_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "heic", "mp4", "mov", "m4v",
    "webm", "mkv", "avi", "wmv", "mpg", "mpeg", "mp3", "wav", "flac", "aac", "ogg",
    "m4a", "zip", "rar", "7z", "tar", "gz", "bz2", "xz",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    Scan,
    Scanning,
    Results,
}

/// Top-level workspace: a file browser, duplicate-finder, or empty-folders finder workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    Explorer,
    Dedupe,
    EmptyFolders,
    ArchiveFiles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerSortKey {
    #[default]
    Name,
    Size,
    Modified,
    Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerViewMode {
    #[default]
    List,
    Grid,
}

/// Header type filter for the Explorer listing (All / Folders / Images / Videos / Zips).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerTypeFilter {
    #[default]
    All,
    Folders,
    Image,
    Video,
    Archive,
}

#[derive(Debug, Clone)]
pub struct ExplorerEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

/// A named staging list bound to a destination folder. Files are added to the
/// queue's `items` while browsing (no move yet); a Submit action moves them all
/// into `target`.
#[derive(Debug, Clone)]
pub struct FileQueue {
    pub name: String,
    pub target: PathBuf,
    pub items: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScanWorkKind {
    #[default]
    FullScan,
    /// Re-hash only paths from the current duplicate report.
    RefreshFindings,
}

/// How duplicate groups are ordered in the results sidebar (after filter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DupGroupSortOrder {
    /// Original order from the scan (`#1`, `#2`, …).
    #[default]
    ScanOrder,
    /// Largest reclaimable space first.
    WastedBytesDesc,
    WastedBytesAsc,
    /// Most copies in a group first.
    FileCountDesc,
    FileCountAsc,
    /// Stable order by hash string.
    HashAsc,
}

pub struct SmartFileManagerApp {
    pub mode: AppMode,
    // ---- Explorer state ----
    pub explorer_dir: PathBuf,
    pub explorer_entries: Vec<ExplorerEntry>,
    pub explorer_loaded_dir: Option<PathBuf>,
    pub explorer_back: Vec<PathBuf>,
    pub explorer_fwd: Vec<PathBuf>,
    pub explorer_selected: Option<usize>,
    pub explorer_error: Option<String>,
    pub explorer_show_hidden: bool,
    pub explorer_sort_key: ExplorerSortKey,
    pub explorer_sort_asc: bool,
    pub explorer_view_mode: ExplorerViewMode,
    pub explorer_type_filter: ExplorerTypeFilter,
    pub explorer_filter: String,
    pub explorer_last_filter: String,
    pub explorer_group_by_kind: bool,
    pub explorer_preview_open: bool,
    pub explorer_preview_path: Option<PathBuf>,
    pub explorer_preview_rotation: i32,
    /// After extracting an archive, ask whether to send it to the Trash.
    pub explorer_pending_delete_zip: Option<PathBuf>,
    pub explorer_status: Option<String>,
    // ---- Recursive search state ----
    pub explorer_search_rx: Option<Receiver<Vec<ExplorerEntry>>>,
    pub explorer_search_token: Option<CancellationToken>,
    pub explorer_searching: bool,
    /// When true, explorer_entries already holds globally-filtered recursive results;
    /// file_table must NOT apply a second name-filter pass.
    pub explorer_is_recursive_search: bool,
    /// Last keystroke time while waiting for the debounce window.
    pub explorer_search_typed_at: Option<Instant>,
    /// True while waiting for [`SEARCH_DEBOUNCE`] idle before firing search.
    pub explorer_search_pending: bool,
    /// Checked paths among collapsed recursive-search results (Select All / per-row).
    pub explorer_search_checked: HashSet<PathBuf>,
    #[cfg(feature = "mpv")]
    pub mpv: Option<crate::ui::mpv_player::MpvPlayer>,
    pub mpv_loaded_path: Option<PathBuf>,
    // ---- Queue (staged move) state ----
    /// Named staging lists, each bound to a destination folder.
    pub explorer_queues: Vec<FileQueue>,
    /// Header "Queue" button toggles the queue detail view.
    pub explorer_queue_view_open: bool,
    /// Which queue's contents the detail modal is showing.
    pub explorer_active_queue: Option<usize>,
    /// Queue index currently chosen in the preview-modal dropdown.
    pub explorer_preview_queue_pick: Option<usize>,
    /// True while a Submit move is running in the background.
    pub explorer_queue_submitting: bool,
    pub explorer_queue_submit_rx: Option<std::sync::mpsc::Receiver<(usize, usize)>>,
    /// Real-time submit progress: (files_done, total_files, current_filename).
    pub explorer_queue_submit_progress: std::sync::Arc<std::sync::Mutex<(usize, usize, String)>>,
    /// Queue index being submitted (so we can clear its moved items on completion).
    pub explorer_queue_submitting_idx: Option<usize>,
    // ---- Dedupe workflow state ----
    pub view: AppView,
    pub roots: Vec<PathBuf>,
    pub preferred_dirs_text: String,
    pub exclude_globs_text: String,
    /// Full paths of subtrees to exclude (applied to every selected root’s tree).
    pub excluded_subdirs_text: String,
    pub skip_hidden: bool,
    pub skip_system_dirs: bool,
    pub min_file_size: u64,
    pub action_mode: ActionMode,
    pub use_hash_cache: bool,
    pub skip_logs_and_locks: bool,
    pub io_profile: IoProfile,
    pub token: CancellationToken,
    pub progress: Arc<Mutex<ScanProgress>>,
    pub result_rx: Option<Receiver<ScanResult>>,
    pub scan_work: ScanWorkKind,
    pub last_result: Option<ScanResult>,
    pub show_error_log: bool,
    pub apply_message: Option<String>,
    pub apply_errors: Option<String>,
    /// Results: selected duplicate group index.
    pub results_selected_group: Option<usize>,
    /// Tracks the last selected group to clear selection when changing groups.
    pub results_last_selected_group: Option<usize>,
    /// Selected files for bulk operations.
    pub results_selected_files: std::collections::HashSet<PathBuf>,
    /// Which file in the group is selected for preview (row index).
    pub results_selected_file_in_group: usize,
    /// Filter substring for the group list (paths / hash).
    pub results_filter: String,
    /// Sidebar sort order for duplicate groups.
    pub dup_group_sort: DupGroupSortOrder,
    /// Cached preview path (invalidates texture + text when changed).
    pub preview_path: Option<PathBuf>,
    pub preview_rotation: i32,
    pub preview_texture: Option<TextureHandle>,
    pub preview_scroll_text: Option<String>,
    pub preview_subtitle: Option<String>,
    /// ffmpeg → RGB frames for in-panel video (muted).
    pub inline_video: Option<crate::ui::video_inline::InlineVideoDecoder>,
    /// After keyboard navigation in the duplicate list, scroll this group index into view once.
    pub dup_list_scroll_to_group: Option<usize>,
    pub thumbnail_cache: std::collections::HashMap<PathBuf, TextureHandle>,
    pub thumbnail_loading: std::collections::HashSet<PathBuf>,
    pub thumbnail_tx: crossbeam_channel::Sender<(PathBuf, eframe::egui::ColorImage)>,
    pub thumbnail_rx: crossbeam_channel::Receiver<(PathBuf, eframe::egui::ColorImage)>,
    // ---- Folder twins overlap fields ----
    pub folder_overlap: std::collections::HashMap<(PathBuf, PathBuf), crate::ui::folder_intelligence::FolderOverlapEntry>,
    pub dedupe_twins_modal_open: bool,
    pub twins_trash_selections: std::collections::HashMap<(PathBuf, PathBuf), (bool, bool)>,
    pub results_filter_folders: Option<(PathBuf, PathBuf)>,
    pub twins_compare_pair: Option<(PathBuf, PathBuf, usize)>,
    pub video_volume: f64,

    // ---- Empty Folders state ----
    pub empty_folders_results: Vec<PathBuf>,
    pub empty_folders_selected: std::collections::HashSet<PathBuf>,
    pub empty_folders_scanning: bool,
    pub empty_folders_rx: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    pub empty_folders_progress_count: std::sync::Arc<std::sync::Mutex<u64>>,
    pub empty_folders_progress_path: std::sync::Arc<std::sync::Mutex<PathBuf>>,

    // ---- Archive Files state ----
    pub archive_files_results: Vec<PathBuf>,
    pub archive_files_selected: std::collections::HashSet<PathBuf>,
    pub archive_files_scanning: bool,
    pub archive_files_rx: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    pub archive_files_progress_count: std::sync::Arc<std::sync::Mutex<u64>>,
    pub archive_files_progress_path: std::sync::Arc<std::sync::Mutex<PathBuf>>,
    pub archive_files_move_target: Option<PathBuf>,
    pub archive_files_moving: bool,
    pub archive_files_move_rx: Option<std::sync::mpsc::Receiver<(usize, usize)>>,
    /// Real-time progress during a move: (files_done, total_files, current_filename)
    pub archive_files_move_progress: std::sync::Arc<std::sync::Mutex<(usize, usize, String)>>,
}

impl Default for SmartFileManagerApp {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let (thumbnail_tx, thumbnail_rx) = crossbeam_channel::unbounded();
        Self {
            mode: AppMode::default(),
            explorer_dir: home,
            explorer_entries: Vec::new(),
            explorer_loaded_dir: None,
            explorer_back: Vec::new(),
            explorer_fwd: Vec::new(),
            explorer_selected: None,
            explorer_error: None,
            explorer_show_hidden: true,
            explorer_sort_key: ExplorerSortKey::default(),
            explorer_sort_asc: true,
            explorer_view_mode: ExplorerViewMode::default(),
            explorer_type_filter: ExplorerTypeFilter::default(),
            explorer_filter: String::new(),
            explorer_last_filter: String::new(),
            explorer_group_by_kind: false,
            explorer_preview_open: false,
            explorer_preview_path: None,
            explorer_preview_rotation: 0,
            explorer_pending_delete_zip: None,
            explorer_status: None,
            explorer_search_rx: None,
            explorer_search_token: None,
            explorer_searching: false,
            explorer_is_recursive_search: false,
            explorer_search_typed_at: None,
            explorer_search_pending: false,
            explorer_search_checked: HashSet::new(),
            #[cfg(feature = "mpv")]
            mpv: None,
            mpv_loaded_path: None,
            explorer_queues: Vec::new(),
            explorer_queue_view_open: false,
            explorer_active_queue: None,
            explorer_preview_queue_pick: None,
            explorer_queue_submitting: false,
            explorer_queue_submit_rx: None,
            explorer_queue_submit_progress: std::sync::Arc::new(std::sync::Mutex::new((0, 0, String::new()))),
            explorer_queue_submitting_idx: None,
            view: AppView::Scan,
            roots: Vec::new(),
            preferred_dirs_text: String::new(),
            exclude_globs_text: String::new(),
            excluded_subdirs_text: String::new(),
            skip_hidden: true,
            skip_system_dirs: true,
            min_file_size: 1,
            action_mode: ActionMode::DryRun,
            use_hash_cache: true,
            skip_logs_and_locks: false,
            io_profile: IoProfile::default(),
            token: CancellationToken::new(),
            progress: Arc::new(Mutex::new(ScanProgress::default())),
            result_rx: None,
            scan_work: ScanWorkKind::default(),
            last_result: None,
            show_error_log: false,
            apply_message: None,
            apply_errors: None,
            results_selected_group: None,
            results_last_selected_group: None,
            results_selected_files: std::collections::HashSet::new(),
            results_selected_file_in_group: 0,
            results_filter: String::new(),
            dup_group_sort: DupGroupSortOrder::default(),
            preview_path: None,
            preview_rotation: 0,
            preview_texture: None,
            preview_scroll_text: None,
            preview_subtitle: None,
            inline_video: None,
            dup_list_scroll_to_group: None,
            thumbnail_cache: std::collections::HashMap::new(),
            thumbnail_loading: std::collections::HashSet::new(),
            thumbnail_tx,
            thumbnail_rx,
            folder_overlap: std::collections::HashMap::new(),
            dedupe_twins_modal_open: false,
            twins_trash_selections: std::collections::HashMap::new(),
            results_filter_folders: None,
            twins_compare_pair: None,
            video_volume: 0.0,
            empty_folders_results: Vec::new(),
            empty_folders_selected: std::collections::HashSet::new(),
            empty_folders_scanning: false,
            empty_folders_rx: None,
            empty_folders_progress_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
            empty_folders_progress_path: std::sync::Arc::new(std::sync::Mutex::new(PathBuf::new())),

            archive_files_results: Vec::new(),
            archive_files_selected: std::collections::HashSet::new(),
            archive_files_scanning: false,
            archive_files_rx: None,
            archive_files_progress_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
            archive_files_progress_path: std::sync::Arc::new(std::sync::Mutex::new(PathBuf::new())),
            archive_files_move_target: None,
            archive_files_moving: false,
            archive_files_move_rx: None,
            archive_files_move_progress: std::sync::Arc::new(std::sync::Mutex::new((0, 0, String::new()))),
        }
    }
}

impl SmartFileManagerApp {
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

        let folder_configs: Vec<FolderConfig> = self
            .roots
            .iter()
            .map(|r| FolderConfig {
                root: r.clone(),
                excluded_subdirs: excluded_subdirs.clone(),
            })
            .collect();

        ScanConfig {
            roots: self.roots.clone(),
            folder_configs,
            skip_hidden: self.skip_hidden,
            skip_system_dirs: self.skip_system_dirs,
            min_file_size: self.min_file_size,
            action_mode: self.action_mode,
            preferred_dirs: preferred,
            root_order: self.roots.clone(),
            exclude_globs,
            use_hash_cache: self.use_hash_cache,
            skip_logs_and_locks: self.skip_logs_and_locks,
            io_profile: self.io_profile,
        }
    }

    pub fn start_scan(&mut self) {
        self.apply_message = None;
        self.apply_errors = None;
        self.clear_preview_cache();
        self.results_selected_group = None;
        self.results_filter.clear();
        self.view = AppView::Scanning;
        self.last_result = None;
        self.scan_work = ScanWorkKind::FullScan;
        self.token = CancellationToken::new();
        {
            let mut p = self.progress.lock().unwrap();
            *p = ScanProgress::default();
            p.folders = self
                .roots
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

        let cfg = self.build_scan_config();
        let roots = self.roots.clone();
        let token = self.token.clone_token();
        let progress = Arc::clone(&self.progress);

        std::thread::spawn(move || {
            let entries = parallel_scan_roots(&roots, &cfg, &token, |n, path| {
                let mut g = progress.lock().unwrap();
                g.phase = smart_file_manager::types::ProgressPhase::Enumerating {
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

            let res = run_dedup(
                entries,
                &cfg,
                &token,
                Arc::clone(&progress),
                cache,
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
            p.phase = smart_file_manager::types::ProgressPhase::Processing {
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

                self.last_result = Some(res);
                if let Some(ref r) = self.last_result {
                    self.folder_overlap = crate::ui::folder_intelligence::compute_folder_overlap(r);
                }
                self.result_rx = None;
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
            let res = smart_file_manager::scanner::find_empty_folders(&roots, &token, |count, path| {
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

    pub fn delete_selected_empty_folders(&mut self) {
        let to_delete = self.empty_folders_results.clone();
        for p in to_delete {
            if self.empty_folders_selected.contains(&p) && p.is_dir() {
                // Try to remove system files like .DS_Store first so remove_dir succeeds
                if let Ok(entries) = std::fs::read_dir(&p) {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            let fname = entry.file_name();
                            let fname_str = fname.to_string_lossy();
                            if fname_str.starts_with('.') || fname_str == "Thumbs.db" {
                                let _ = std::fs::remove_file(entry.path());
                            }
                        }
                    }
                }
                let _ = std::fs::remove_dir(&p);
            }
        }
        self.empty_folders_results.clear();
        self.empty_folders_selected.clear();
        self.view = AppView::Scan;
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
            let res = smart_file_manager::scanner::find_archive_files(&roots, &token, |count, path| {
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
        if let Some(rx) = &self.archive_files_move_rx {
            if let Ok((success_count, fail_count)) = rx.try_recv() {
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

                self.archive_files_results.clear();
                self.archive_files_selected.clear();
                self.view = AppView::Scan;
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

                let file_name = match p.file_name() {
                    Some(n) => n,
                    None => { fail_count += 1; continue; }
                };

                // Resolve a free destination name (never overwrite)
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
                let ext  = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
                let mut dest = target_dir.join(file_name);
                let mut counter = 1u32;
                // Keep incrementing until we find a name that doesn't exist
                while dest.exists() {
                    let new_name = if ext.is_empty() {
                        format!("{}_{}", stem, counter)
                    } else {
                        format!("{}_{}.{}", stem, counter, ext)
                    };
                    dest = target_dir.join(&new_name);
                    counter += 1;
                }

                // fast_move: rename → APFS clone → buffered stream
                // If it still fails (race condition), retry once with a fresh counter slot
                let moved = if fast_move(&p, &dest).is_ok() {
                    true
                } else {
                    // Last-resort retry with a new unique name
                    let fallback_name = if ext.is_empty() {
                        format!("{}_{}", stem, counter)
                    } else {
                        format!("{}_{}.{}", stem, counter, ext)
                    };
                    let fallback = target_dir.join(&fallback_name);
                    fast_move(&p, &fallback).is_ok()
                };

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

    /// Prompt for a destination folder and create a new queue named after it.
    pub fn explorer_new_queue(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
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
            self.explorer_status = Some(format!("Created queue “{name}”."));
        }
    }

    /// Stage a file into queue[idx] (deduped by path).
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
            if self.explorer_active_queue == Some(idx) {
                self.explorer_active_queue = None;
                self.explorer_queue_view_open = false;
            }
            // Keep the dropdown pick valid.
            self.explorer_preview_queue_pick = None;
            self.explorer_status = Some(format!("Deleted queue “{name}”."));
        }
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

    /// Move all staged files in queue[idx] into its target folder. Mirrors
    /// `execute_move_archive_files`: confirm → background thread → collision-safe
    /// destinations → `fast_move` → progress via Mutex → counts over a channel.
    pub fn submit_queue(&mut self, idx: usize) {
        let Some(q) = self.explorer_queues.get(idx) else { return };
        let target_dir = q.target.clone();
        if !target_dir.is_dir() {
            self.explorer_error = Some(format!(
                "Queue folder no longer exists: {}",
                target_dir.display()
            ));
            return;
        }
        let to_move: Vec<PathBuf> = q.items.clone();
        let total = to_move.len();
        if total == 0 {
            self.explorer_status = Some("Queue is empty — nothing to move.".to_string());
            return;
        }

        let confirm = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Info)
            .set_title("Submit Queue")
            .set_description(format!(
                "Move {} file(s) to:\n{}",
                total,
                target_dir.display()
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if confirm != rfd::MessageDialogResult::Yes {
            return;
        }

        self.explorer_queue_submitting = true;
        self.explorer_queue_submitting_idx = Some(idx);
        if let Ok(mut p) = self.explorer_queue_submit_progress.lock() {
            *p = (0, total, String::new());
        }

        let (tx, rx) = std::sync::mpsc::channel();
        self.explorer_queue_submit_rx = Some(rx);
        let progress = std::sync::Arc::clone(&self.explorer_queue_submit_progress);

        std::thread::spawn(move || {
            let mut success_count = 0;
            let mut fail_count = 0;

            for (i, p) in to_move.iter().enumerate() {
                let fname = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Ok(mut prog) = progress.lock() {
                    *prog = (i, total, fname.clone());
                }

                if !p.is_file() {
                    fail_count += 1;
                    continue;
                }
                let file_name = match p.file_name() {
                    Some(n) => n,
                    None => { fail_count += 1; continue; }
                };

                // Resolve a free destination name (never overwrite).
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
                let mut dest = target_dir.join(file_name);
                let mut counter = 1u32;
                while dest.exists() {
                    let new_name = if ext.is_empty() {
                        format!("{}_{}", stem, counter)
                    } else {
                        format!("{}_{}.{}", stem, counter, ext)
                    };
                    dest = target_dir.join(&new_name);
                    counter += 1;
                }

                let moved = if fast_move(p, &dest).is_ok() {
                    true
                } else {
                    let fallback_name = if ext.is_empty() {
                        format!("{}_{}", stem, counter)
                    } else {
                        format!("{}_{}.{}", stem, counter, ext)
                    };
                    let fallback = target_dir.join(&fallback_name);
                    fast_move(p, &fallback).is_ok()
                };

                if moved { success_count += 1; } else { fail_count += 1; }
            }

            if let Ok(mut prog) = progress.lock() {
                *prog = (total, total, String::new());
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

                // Clear the moved items from the submitted queue.
                if let Some(idx) = self.explorer_queue_submitting_idx.take() {
                    if let Some(q) = self.explorer_queues.get_mut(idx) {
                        q.items.clear();
                    }
                }

                let msg = if fail_count == 0 {
                    format!("Moved {success_count} file(s) to the queue folder.")
                } else {
                    format!("Moved {success_count} file(s); {fail_count} failed.")
                };
                self.explorer_status = Some(msg);
                self.explorer_reload();
            }
        }
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
    pub fn explorer_ensure_loaded(&mut self) {
        if self.explorer_loaded_dir.as_ref() == Some(&self.explorer_dir) {
            return;
        }
        self.explorer_reload();
    }

    pub fn explorer_reload(&mut self) {
        self.thumbnail_cache.clear();
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
                    if !self.explorer_show_hidden && name.starts_with('.') {
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
                ExplorerSortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                ExplorerSortKey::Size => a.size.cmp(&b.size),
                ExplorerSortKey::Modified => a.modified.cmp(&b.modified),
                ExplorerSortKey::Kind => {
                    let ea = a.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let eb = b.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    ea.to_lowercase().cmp(&eb.to_lowercase())
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
                self.explorer_status = Some(format!("Extracted to “{stem}”."));
                self.explorer_pending_delete_zip = Some(zip.to_path_buf());
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
        self.explorer_preview_path = Some(path);
        self.explorer_preview_open = true;
        self.explorer_preview_rotation = 0;
    }

    /// Send the current Explorer folder into the dedupe workflow.
    pub fn scan_explorer_dir(&mut self) {
        self.roots = vec![self.explorer_dir.clone()];
        self.mode = AppMode::Dedupe;
        self.view = AppView::Scan;
        self.last_result = None;
        self.results_selected_group = None;
        self.clear_preview_cache();
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
        self.explorer_preview_open = false;
        self.explorer_preview_path = None;
    }
}


impl eframe::App for SmartFileManagerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll background thumbnails
        let mut got_any = false;
        while let Ok((path, img)) = self.thumbnail_rx.try_recv() {
            let id = format!("sfm_thumb_{}", path.display());
            let handle = ctx.load_texture(id, img, egui::TextureOptions::LINEAR);
            self.thumbnail_cache.insert(path, handle);
            got_any = true;
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

        if self.view == AppView::Scanning {
            self.poll_result();
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
            if filter_changed {
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
                    .fill(egui::Color32::from_rgb(28, 28, 36))
                    .inner_margin(egui::Margin::symmetric(18.0, 0.0))
                    .stroke(egui::Stroke::new(1.0, crate::ui::style::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(egui::RichText::new("🗂").size(20.0));
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Smart File Manager")
                            .strong()
                            .size(16.0)
                            .color(egui::Color32::from_rgb(236, 239, 252)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        mode_switch(self, ui);
                        if mode == AppMode::Explorer {
                            ui.add_space(10.0);
                            let staged: usize =
                                self.explorer_queues.iter().map(|q| q.items.len()).sum();
                            let label = if staged > 0 {
                                format!("🗂 Queue ({staged})")
                            } else {
                                "🗂 Queue".to_string()
                            };
                            if ui.button(label).clicked() {
                                self.explorer_queue_view_open = !self.explorer_queue_view_open;
                                if self.explorer_queue_view_open
                                    && self.explorer_active_queue.is_none()
                                    && !self.explorer_queues.is_empty()
                                {
                                    self.explorer_active_queue = Some(0);
                                }
                            }
                        }
                    });
                });
            });

        let results_ready =
            mode == AppMode::Dedupe && matches!(self.view, AppView::Results) && self.last_result.is_some();

        let show_sidebar = match mode {
            AppMode::Explorer => true,
            AppMode::Dedupe => results_ready,
            AppMode::EmptyFolders => false,
            AppMode::ArchiveFiles => false,
        };

        if show_sidebar {
            egui::SidePanel::left("sidebar")
                .resizable(true)
                .default_width(if results_ready { 300.0 } else { 268.0 })
                .min_width(236.0)
                .max_width(560.0)
                .frame(
                    egui::Frame::none()
                        .fill(crate::ui::style::NAV_FILL)
                        .inner_margin(egui::Margin::same(14.0))
                        .stroke(egui::Stroke::new(1.0, crate::ui::style::BORDER)),
                )
                .show(ctx, |ui| {
                    match mode {
                        AppMode::Explorer => crate::ui::explorer_sidebar(self, ui),
                        AppMode::Dedupe if results_ready => {
                            results_dup_list_panel(self, ui, ctx);
                        }
                        _ => {}
                    }
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(crate::ui::style::CANVAS_FILL)
                    .inner_margin(egui::Margin::symmetric(22.0, 18.0)),
            )
            .show(ctx, |ui| {
                match mode {
                    AppMode::Explorer => crate::ui::explorer_main_panel(self, ui, ctx),
                    AppMode::Dedupe => match self.view {
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
                crate::ui::player_window(self, ctx);
                crate::ui::queue_window(self, ctx);
            });
    }
}

fn mode_switch(app: &mut SmartFileManagerApp, ui: &mut egui::Ui) {
    let seg = |ui: &mut egui::Ui, label: &str, active: bool| -> bool {
        let (fg, bg) = if active {
            (
                egui::Color32::from_rgb(248, 250, 255),
                crate::ui::style::ACCENT,
            )
        } else {
            (
                egui::Color32::from_rgb(196, 202, 222),
                egui::Color32::from_rgb(38, 40, 50),
            )
        };
        ui.add(
            egui::Button::new(egui::RichText::new(label).strong().color(fg))
                .fill(bg)
                .min_size(egui::vec2(0.0, 34.0)),
        )
        .clicked()
    };
    ui.spacing_mut().item_spacing.x = 8.0;
    
    let w = 90.0;
    ui.allocate_ui(egui::vec2(w, 32.0), |ui| {
        ui.set_width(w);
        if seg(ui, "📦 Archives", app.mode == AppMode::ArchiveFiles) {
            app.mode = AppMode::ArchiveFiles;
            app.view = AppView::Scan;
        }
    });
    ui.allocate_ui(egui::vec2(w, 32.0), |ui| {
        ui.set_width(w);
        if seg(ui, "📁 Empty", app.mode == AppMode::EmptyFolders) {
            app.mode = AppMode::EmptyFolders;
            app.view = AppView::Scan;
        }
    });
    ui.allocate_ui(egui::vec2(w, 32.0), |ui| {
        ui.set_width(w);
        if seg(ui, "🔍 Dedupe", app.mode == AppMode::Dedupe) {
            app.mode = AppMode::Dedupe;
        }
    });
    ui.allocate_ui(egui::vec2(w, 32.0), |ui| {
        ui.set_width(w);
        if seg(ui, "🗂 Explorer", app.mode == AppMode::Explorer) {
            app.mode = AppMode::Explorer;
        }
    });
}

/// Move a file as fast as possible:
/// 1. `rename(2)` — atomic, instant on same filesystem.
/// 2. `cp -c` — APFS copy-on-write clone (macOS), instant regardless of file size.
/// 3. `cp`    — regular copy for non-APFS volumes, then delete source.
fn fast_move(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    // Fastest path: atomic rename (same volume)
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }

    // macOS: try APFS clone via `cp -c` (copy-on-write, ~instant)
    #[cfg(target_os = "macos")]
    {
        let clone = std::process::Command::new("cp")
            .args(["-c", "--"])
            .arg(src)
            .arg(dst)
            .status();
        if clone.map(|s| s.success()).unwrap_or(false) {
            std::fs::remove_file(src)?;
            return Ok(());
        }
    }

    // Generic fallback: stream copy + delete (other platforms / non-APFS)
    {
        use std::io::{BufReader, BufWriter, Read, Write};
        const BUF: usize = 4 * 1024 * 1024; // 4 MB
        let mut r = BufReader::with_capacity(BUF, std::fs::File::open(src)?);
        let mut w = BufWriter::with_capacity(BUF, std::fs::File::create(dst)?);
        let mut buf = vec![0u8; BUF];
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 { break; }
            w.write_all(&buf[..n])?;
        }
        w.flush()?;
    }
    std::fs::remove_file(src)?;
    Ok(())
}

/// Drop search hits that live under a matched folder (folder is the selectable unit).
fn collapse_search_results(entries: Vec<ExplorerEntry>) -> Vec<ExplorerEntry> {
    let mut dir_paths: Vec<PathBuf> = entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| e.path.clone())
        .collect();
    // Shallowest first so parents win over nested matching folders.
    dir_paths.sort_by_key(|p| p.components().count());

    let mut keep_dirs: Vec<PathBuf> = Vec::new();
    for d in dir_paths {
        let under_kept = keep_dirs.iter().any(|k| path_is_strict_descendant(&d, k));
        if !under_kept {
            keep_dirs.push(d);
        }
    }

    entries
        .into_iter()
        .filter(|e| {
            if e.is_dir {
                keep_dirs.iter().any(|k| k == &e.path)
            } else {
                !keep_dirs
                    .iter()
                    .any(|k| path_is_strict_descendant(&e.path, k))
            }
        })
        .collect()
}

/// True for known OS/dev junk that should never appear in Explorer listings.
pub fn is_system_entry(name: &str, is_dir: bool) -> bool {
    let lower = name.to_lowercase();
    if is_dir {
        matches!(
            lower.as_str(),
            "node_modules"
                | ".git"
                | "__pycache__"
                | ".trash"
                | "$recycle.bin"
                | "proc"
                | "sys"
                | "dev"
        )
    } else {
        matches!(
            lower.as_str(),
            ".ds_store" | "thumbs.db" | "desktop.ini" | ".localized"
        )
    }
}

fn path_is_strict_descendant(child: &Path, ancestor: &Path) -> bool {
    child.starts_with(ancestor) && child != ancestor
}

/// Walk `root` for media/archive files (used when staging a checked search folder).
fn collect_media_files_under(root: &Path, show_hidden: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
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
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if is_system_entry(&name, false) {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        if MEDIA_EXTS.contains(&ext.as_str()) {
            out.push(path.to_path_buf());
        }
    }
    out
}
