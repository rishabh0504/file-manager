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

mod explorer;
mod files;
mod fs_ops;
mod preview;
mod scan;
mod staging;
mod types;
mod ui_loop;
#[cfg(test)]
mod tests;

pub use types::*;
pub(crate) use fs_ops::*;

pub struct SortwiseApp {
    pub mode: AppMode,
    // ---- Explorer state ----
    pub explorer_dir: PathBuf,
    pub explorer_entries: Vec<ExplorerEntry>,
    pub explorer_loaded_dir: Option<PathBuf>,
    pub explorer_back: Vec<PathBuf>,
    pub explorer_fwd: Vec<PathBuf>,
    pub explorer_selected: Option<usize>,
    /// Ctrl/Cmd-click multi-selection in the Explorer (paths; cleared on navigation).
    pub explorer_multi: std::collections::HashSet<PathBuf>,
    /// Entry index where the last plain or Ctrl click landed; the Shift-click range starts here.
    pub explorer_anchor: Option<usize>,
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
    pub extract_banner: Option<ExtractBanner>,
    /// Sortwise mark for the top bar, decoded once on first frame.
    pub brand_logo: Option<eframe::egui::TextureHandle>,
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
    // ---- Files tab (recursive flat list) state ----
    /// Folder chosen in the Files tab. `None` until the tab is first opened (then = explorer folder).
    pub files_view_dir: Option<PathBuf>,
    /// All Files-tab filters, independent of the Explorer toolbar.
    pub files_filter: FilesFilter,
    /// Set by Refresh: re-walk the same folder, keeping checked files.
    pub explorer_files_rescan: bool,
    /// True while the shared preview modal was opened from the Files tab. Its arrow keys
    /// then step through the filtered, sorted Files list instead of the Explorer list.
    pub files_preview_active: bool,
    /// Filter or Sort parent tab.
    pub files_panel_mode: FilesPanelMode,
    /// Folder the current `explorer_files` listing belongs to.
    pub explorer_files_root: Option<PathBuf>,
    /// Unfiltered recursive listing for `explorer_files_root`, sorted by size desc.
    pub explorer_files: Vec<sortwise::recursive_files::RecursiveFile>,
    pub explorer_files_loading: bool,
    pub explorer_files_truncated: bool,
    pub explorer_files_rx: Option<Receiver<sortwise::recursive_files::Listing>>,
    pub explorer_files_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub explorer_files_checked: HashSet<PathBuf>,
    pub explorer_files_sort: ExplorerFilesSort,
    pub explorer_files_sort_asc: bool,
    /// Indices into `explorer_files` after filter + sort. Rebuilt when `explorer_files_view_key` changes.
    pub explorer_files_view: Vec<usize>,
    /// (filter, sort, ascending, listing generation, staged count) the current view was built from.
    pub explorer_files_view_key: Option<(FilesFilter, ExplorerFilesSort, bool, u64, u64)>,
    /// Bumped whenever `explorer_files` changes; invalidates `explorer_files_view`.
    pub explorer_files_gen: u64,
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
    /// Movable queue: separate runtime staging list independent of existing folder queues.
    pub movable_items: Vec<MovableItem>,
    /// Toggle visibility of the Movable Queue review modal.
    pub movable_queue_open: bool,
    /// Whether movable queue window is expanded/maximized to fullscreen
    /// Currently playing path in movable queue (for inline player preview)
    pub movable_playing_path: Option<PathBuf>,
    /// Last chosen queue name in movable queue view to auto-fill subsequent items.
    pub movable_last_chosen_queue: Option<String>,
    /// Queue index currently chosen in the preview-modal dropdown.
    pub explorer_preview_queue_pick: Option<usize>,
    /// True while a Submit move is running in the background.
    pub explorer_queue_submitting: bool,
    pub explorer_queue_submit_rx: Option<std::sync::mpsc::Receiver<(usize, usize)>>,
    /// Real-time submit progress: (files_done, total_files, current_filename).
    pub explorer_queue_submit_progress: std::sync::Arc<std::sync::Mutex<(usize, usize, String)>>,
    /// Queue index being submitted (so we can clear its moved items on completion).
    pub explorer_queue_submitting_idx: Option<usize>,
    // ---- Folder Modal state ----
    pub folder_modal_open: bool,
    pub folder_modal_path: Option<PathBuf>,
    pub folder_modal_entries: Vec<ExplorerEntry>,
    pub folder_modal_loaded_path: Option<PathBuf>,
    pub folder_modal_filter: String,
    pub folder_modal_selected: Option<usize>,
    // ---- Dedupe workflow state ----
    pub view: AppView,
    pub roots: Vec<PathBuf>,
    /// Source (A) reference roots for CompareSourceDest mode.
    pub source_roots: Vec<PathBuf>,
    /// Destination (B) roots where duplicates will be found for CompareSourceDest mode.
    pub dest_roots: Vec<PathBuf>,
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
    /// Shared so panels can hold a cheap handle per frame (P-1). Mutate via `make_mut` (copy-on-write).
    pub last_result: Option<Arc<ScanResult>>,
    /// Bumped whenever `last_result` changes; keys the group-list cache (P-3).
    pub results_epoch: u64,
    /// Bulk action waiting for confirmation (Phase 1, U1–U3).
    pub pending_action: Option<PendingAction>,
    /// Memoized filter + sort result for the group list (P-3).
    pub dup_list_cache: DupListCache,
    /// Group-list kind filter; `None` shows every kind (U11).
    pub results_kind_filter: Option<sortwise::heuristics::MediaKind>,
    /// Duplicate groups streamed from a running scan (P-6), newest batches appended.
    pub live_groups: Vec<sortwise::types::DuplicateGroup>,
    pub live_rx: Option<Receiver<Vec<sortwise::types::DuplicateGroup>>>,
    /// When the current scan started, for elapsed and ETA in the progress view (U12).
    pub scan_started_at: Option<Instant>,
    /// Sizes for movable-queue rows, so the window does not stat files every frame (P-5).
    pub movable_size_cache: std::collections::HashMap<PathBuf, u64>,
    /// Last settings written to disk; compared each frame to decide whether to save (U16).
    pub saved_settings: sortwise::settings::Settings,
    /// Shortcut list in the results view (R3).
    pub shortcuts_open: bool,
    /// Set by `--snapshot`; see `SnapshotPlan`.
    pub snapshot: Option<SnapshotPlan>,
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
    pub thumbnail_failed: std::collections::HashSet<PathBuf>,
    pub navigation_epoch: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub thumbnail_worker_tx: crossbeam_channel::Sender<ThumbnailRequest>,
    pub thumbnail_response_rx: crossbeam_channel::Receiver<ThumbnailResponse>,
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

impl Default for SortwiseApp {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let (thumbnail_worker_tx, worker_rx) = crossbeam_channel::bounded::<ThumbnailRequest>(64);
        let (response_tx, thumbnail_response_rx) = crossbeam_channel::unbounded::<ThumbnailResponse>();
        let navigation_epoch = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1));

        let num_workers = std::cmp::max(2, num_cpus::get().min(4));
        for _ in 0..num_workers {
            let rx = worker_rx.clone();
            let tx = response_tx.clone();
            let epoch_atomic = navigation_epoch.clone();
            std::thread::Builder::new()
                .name("thumb-worker".into())
                .spawn(move || {
                    while let Ok(req) = rx.recv() {
                        if req.epoch != epoch_atomic.load(std::sync::atomic::Ordering::Relaxed) {
                            continue;
                        }
                        let res = if req.is_video {
                            crate::ui::explorer_panel::load_video_thumb(&req.path)
                        } else {
                            crate::ui::explorer_panel::load_image_thumb(&req.path)
                        };
                        let _ = tx.send(ThumbnailResponse {
                            path: req.path,
                            epoch: req.epoch,
                            image: res.ok(),
                        });
                    }
                })
                .ok();
        }
        Self {
            mode: AppMode::default(),
            explorer_dir: home,
            explorer_entries: Vec::new(),
            explorer_loaded_dir: None,
            explorer_back: Vec::new(),
            explorer_fwd: Vec::new(),
            explorer_selected: None,
            explorer_multi: std::collections::HashSet::new(),
            explorer_anchor: None,
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
            extract_banner: None,
            brand_logo: None,
            explorer_status: None,
            explorer_search_rx: None,
            explorer_search_token: None,
            explorer_searching: false,
            explorer_is_recursive_search: false,
            explorer_search_typed_at: None,
            explorer_search_pending: false,
            explorer_search_checked: HashSet::new(),
            files_view_dir: None,
            files_filter: FilesFilter::default(),
            explorer_files_rescan: false,
            files_preview_active: false,
            files_panel_mode: FilesPanelMode::default(),
            explorer_files_root: None,
            explorer_files: Vec::new(),
            explorer_files_loading: false,
            explorer_files_truncated: false,
            explorer_files_rx: None,
            explorer_files_cancel: None,
            explorer_files_checked: HashSet::new(),
            explorer_files_sort: ExplorerFilesSort::Size,
            explorer_files_sort_asc: false,
            explorer_files_view: Vec::new(),
            explorer_files_view_key: None,
            explorer_files_gen: 0,
            #[cfg(feature = "mpv")]
            mpv: None,
            mpv_loaded_path: None,
            explorer_queues: Vec::new(),
            explorer_queue_view_open: false,
            explorer_active_queue: None,
            movable_items: Vec::new(),
            movable_queue_open: false,
            movable_playing_path: None,
            movable_last_chosen_queue: None,
            explorer_preview_queue_pick: None,
            explorer_queue_submitting: false,
            explorer_queue_submit_rx: None,
            explorer_queue_submit_progress: std::sync::Arc::new(std::sync::Mutex::new((0, 0, String::new()))),
            explorer_queue_submitting_idx: None,
            folder_modal_open: false,
            folder_modal_path: None,
            folder_modal_entries: Vec::new(),
            folder_modal_loaded_path: None,
            folder_modal_filter: String::new(),
            folder_modal_selected: None,
            view: AppView::Scan,
            roots: Vec::new(),
            source_roots: Vec::new(),
            dest_roots: Vec::new(),
            preferred_dirs_text: String::new(),
            exclude_globs_text: String::new(),
            excluded_subdirs_text: String::new(),
            skip_hidden: false,
            skip_system_dirs: true,
            min_file_size: 4 * 1024,
            action_mode: ActionMode::DryRun,
            use_hash_cache: true,
            skip_logs_and_locks: false,
            io_profile: IoProfile::default(),
            token: CancellationToken::new(),
            progress: Arc::new(Mutex::new(ScanProgress::default())),
            result_rx: None,
            scan_work: ScanWorkKind::default(),
            last_result: None,
            results_epoch: 0,
            pending_action: None,
            dup_list_cache: DupListCache::default(),
            shortcuts_open: false,
            snapshot: None,
            results_kind_filter: None,
            scan_started_at: None,
            live_groups: Vec::new(),
            live_rx: None,
            movable_size_cache: std::collections::HashMap::new(),
            saved_settings: sortwise::settings::Settings::default(),
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
            thumbnail_failed: std::collections::HashSet::new(),
            navigation_epoch,
            thumbnail_worker_tx,
            thumbnail_response_rx,
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
