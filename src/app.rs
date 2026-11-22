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
    CompareSourceDest,
    EmptyFolders,
    ArchiveFiles,
    /// Header "Files" tab: flat recursive list of every file under `files_view_dir`.
    AllFiles,
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
    Folders,
}

/// Parent tab of the Files view: Filter or Sort. Only one set of controls is shown at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilesPanelMode {
    #[default]
    Filter,
    Sort,
}

/// Sort key for the Files view. Default: size, descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerFilesSort {
    #[default]
    Size,
    Name,
    Modified,
    Type,
}

/// Where a file shown in the Files view is staged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesStage {
    None,
    /// In this folder queue's items (Folder queues tab).
    Queue(String),
    /// In Items (loose staging), with the queue picked there, if any.
    Item(Option<String>),
}

/// Whether the Files view shows staged files (in a folder queue or in Items).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StagedFilter {
    #[default]
    All,
    StagedOnly,
    NotStaged,
}

/// Every filter the Files view applies. All fields are AND-ed together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesFilter {
    /// Case-insensitive substring of the path relative to the chosen folder.
    pub name: String,
    pub kind: ExplorerTypeFilter,
    /// Inclusive size bounds in bytes.
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    /// Keep files modified within this many days.
    pub modified_within_days: Option<u32>,
    pub staged: StagedFilter,
}

impl Default for FilesFilter {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: ExplorerTypeFilter::Video,
            min_size: None,
            max_size: None,
            modified_within_days: None,
            staged: StagedFilter::All,
        }
    }
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
#[allow(dead_code)]
pub struct GroupOutcome {
    pub name: String,
    pub target: PathBuf,
    pub items: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct FileQueue {
    pub name: String,
    pub target: PathBuf,
    pub items: Vec<PathBuf>,
}

/// A staged item in the movable queue awaiting destination assignment and move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovableItem {
    pub path: PathBuf,
    pub target_queue: Option<String>,
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

#[derive(Debug, Clone)]
pub struct ThumbnailRequest {
    pub path: PathBuf,
    pub is_video: bool,
    pub epoch: usize,
}

/// Development aid: `--snapshot <png> <screen>` renders one screen and exits.
#[derive(Debug, Clone)]
pub struct SnapshotPlan {
    pub path: PathBuf,
    pub frames: u32,
    pub requested: bool,
}

pub struct ThumbnailResponse {
    pub path: PathBuf,
    pub epoch: usize,
    pub image: Option<eframe::egui::ColorImage>,
}

/// A bulk action that the confirm dialog must approve before it runs (Phase 1).
#[derive(Debug, Clone)]
pub enum PendingAction {
    /// Move the planned paths to Trash. `action` names the action in the log.
    Trash {
        title: String,
        action: &'static str,
        plan: sortwise::action_plan::ActionPlan,
    },
    /// Run the global Apply action (Delete or HardLink) across all groups.
    Apply {
        mode: ActionMode,
        plan: sortwise::action_plan::ActionPlan,
    },
}

/// Inline banner after an archive was extracted (UI plan §6.2). Dismissed on navigation.
#[derive(Debug, Clone)]
pub struct ExtractBanner {
    /// The archive that was extracted.
    pub zip: PathBuf,
    /// Folder the contents were extracted into.
    pub dest: PathBuf,
    /// Folder that was open when the banner appeared. Navigating elsewhere dismisses it.
    pub dir: PathBuf,
}

/// Memoized visible-group list for the results sidebar (P-3).
#[derive(Debug, Clone, Default)]
pub struct DupListCache {
    /// `(results_epoch, filter, sort)` the cached list was built for.
    pub key: Option<(u64, String, DupGroupSortOrder, Option<sortwise::heuristics::MediaKind>)>,
    pub visible: Vec<usize>,
    /// Largest `total_wasted_bytes` among `visible`, for gradient normalization.
    pub max_wasted: u64,
}

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
        self.files_preview_active = false;
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
        let Some(dir) = self.files_view_dir.clone() else {
            return;
        };
        let same_dir = self.explorer_files_root.as_ref() == Some(&dir);
        if same_dir && !self.explorer_files_rescan {
            return;
        }
        // Refresh of the same folder keeps the checked files and the old list until the new one arrives.
        let keep_state = same_dir;
        self.explorer_files_rescan = false;
        if let Some(old) = self.explorer_files_cancel.take() {
            old.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let root = dir.clone();
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
        self.explorer_files_root = Some(dir);
        self.explorer_files_cancel = Some(cancel);
        self.explorer_files_rx = Some(rx);
        self.explorer_files_loading = true;
        if !keep_state {
            self.explorer_files.clear();
            self.explorer_files_truncated = false;
            self.explorer_files_checked.clear();
        }
        self.explorer_files_gen += 1;
    }

    /// Files tab Refresh: re-walk the current folder and pick up new, renamed or deleted files.
    pub fn explorer_files_refresh(&mut self) {
        self.explorer_files_rescan = true;
        self.explorer_files_gen += 1;
    }

    /// Drop one moved path (and everything under it, for a moved folder) from the Files list.
    /// Called after a successful move, so moved items disappear from the list.
    pub fn explorer_files_forget_path(&mut self, path: &Path, was_dir: bool) {
        let gone = |p: &Path| p == path || (was_dir && p.starts_with(path));
        let before = self.explorer_files.len();
        self.explorer_files.retain(|f| !gone(&f.path));
        self.explorer_files_checked.retain(|p| !gone(p));
        if self.explorer_files.len() != before {
            self.explorer_files_gen += 1;
        }
    }

    /// Drop every listed file that no longer exists on disk. Called after folder-queue submits,
    /// which move many files at once.
    pub fn explorer_files_prune_missing(&mut self) {
        let before = self.explorer_files.len();
        self.explorer_files.retain(|f| f.path.exists());
        let remaining: HashSet<&PathBuf> = self.explorer_files.iter().map(|f| &f.path).collect();
        self.explorer_files_checked.retain(|p| remaining.contains(p));
        if self.explorer_files.len() != before {
            self.explorer_files_gen += 1;
        }
    }

    /// Files view: stage files. `Some(name)` adds them to that folder queue's items (Folder
    /// queues tab, moved on Submit). `None` adds them to Items (loose staging, no queue).
    /// A file ends up in exactly one place, so it can never be moved twice. Moves nothing on disk.
    /// Returns how many files were newly placed.
    pub fn explorer_files_stage(&mut self, paths: &[PathBuf], queue: Option<String>) -> usize {
        let idx = match &queue {
            Some(name) => match self.explorer_queues.iter().position(|q| &q.name == name) {
                Some(i) => Some(i),
                None => {
                    self.explorer_status = Some(format!("Queue “{name}” not found."));
                    return 0;
                }
            },
            None => None,
        };
        let mut added = 0;
        for path in paths {
            match idx {
                Some(i) => {
                    if self.explorer_queues[i].items.contains(path) {
                        continue;
                    }
                    self.explorer_files_unstage(path);
                    self.explorer_queues[i].items.push(path.clone());
                    added += 1;
                }
                None => {
                    if self.movable_items.iter().any(|m| &m.path == path) {
                        continue;
                    }
                    self.explorer_files_unstage(path);
                    self.movable_items.push(MovableItem { path: path.clone(), target_queue: None });
                    added += 1;
                }
            }
        }
        self.explorer_status = Some(match &queue {
            Some(q) => format!("Added {added} file(s) to queue “{q}”."),
            None => format!("Added {added} file(s) to Items."),
        });
        added
    }

    /// Files view: remove a file from every folder queue and from Items. Never touches the file.
    pub fn explorer_files_unstage(&mut self, path: &Path) {
        for q in &mut self.explorer_queues {
            q.items.retain(|p| p != path);
        }
        self.movable_items.retain(|m| m.path != path);
    }

    /// Files view: where a file is staged. A folder queue wins over Items.
    pub fn explorer_files_stage_of(&self, path: &Path) -> FilesStage {
        if let Some(q) = self.explorer_queues.iter().find(|q| q.items.iter().any(|p| p == path)) {
            return FilesStage::Queue(q.name.clone());
        }
        match self.movable_items.iter().find(|m| m.path == path) {
            Some(m) => FilesStage::Item(m.target_queue.clone()),
            None => FilesStage::None,
        }
    }

    /// Files view: every staged path, from folder queues and Items.
    pub fn explorer_files_staged_set(&self) -> std::collections::HashSet<PathBuf> {
        self.explorer_queues
            .iter()
            .flat_map(|q| q.items.iter().cloned())
            .chain(self.movable_items.iter().map(|m| m.path.clone()))
            .collect()
    }

    /// Files view: order-independent fingerprint of the staged set, so the list is refiltered
    /// whenever staging changes anywhere (cards, preview modal, staging panel).
    pub fn explorer_files_staged_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let set = self.explorer_files_staged_set();
        let mut acc = set.len() as u64;
        for p in &set {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            p.hash(&mut h);
            acc = acc.wrapping_add(h.finish());
        }
        acc
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

    /// Open the shared preview from a Files-tab card. Arrow keys then step through the Files list.
    pub fn files_open_preview(&mut self, path: PathBuf) {
        self.explorer_open_preview(path);
        self.files_preview_active = true;
    }

    pub fn explorer_open_preview(&mut self, path: PathBuf) {
        self.files_preview_active = false;
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
        self.files_preview_active = false;
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
        let p = crate::ui::style::pal();

        // Left navigation (modes + places). Icon rail when the window is narrow or staging is open.
        let staging_open = self.movable_queue_open || self.explorer_queue_view_open;
        let rail = crate::ui::style::is_narrow(ctx)
            || (staging_open && ctx.screen_rect().width() < 1200.0);
        egui::SidePanel::left("nav")
            .resizable(false)
            .exact_width(if rail { crate::ui::shell::RAIL_W } else { crate::ui::shell::NAV_W })
            .frame(
                egui::Frame::none()
                    .fill(p.sidebar)
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .stroke(egui::Stroke::new(1.0, p.border)),
            )
            .show(ctx, |ui| crate::ui::shell::nav(self, ui, rail));

        egui::TopBottomPanel::top("top")
            .exact_height(crate::ui::style::H_TOOLBAR)
            .frame(
                egui::Frame::none()
                    .fill(p.surface)
                    .inner_margin(egui::Margin::symmetric(20.0, 0.0))
                    .stroke(egui::Stroke::new(1.0, p.border)),
            )
            .show(ctx, |ui| crate::ui::shell::toolbar(self, ui));

        // Status bar. Added before the side and central panels, as egui requires.
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(
                egui::Frame::none()
                    .fill(p.surface)
                    .inner_margin(egui::Margin::symmetric(14.0, 0.0))
                    .stroke(egui::Stroke::new(1.0, p.border)),
            )
            .show(ctx, |ui| crate::ui::status_bar::status_bar(self, ui));

        let results_ready =
            (mode == AppMode::Dedupe || mode == AppMode::CompareSourceDest)
                && matches!(self.view, AppView::Results)
                && self.last_result.is_some();

        // Duplicate groups column (results only).
        if results_ready {
            let narrow = crate::ui::style::is_narrow(ctx);
            egui::SidePanel::left("groups")
                .resizable(true)
                .default_width(if narrow { 240.0 } else { 300.0 })
                .min_width(220.0)
                .max_width(520.0)
                .frame(
                    egui::Frame::none()
                        .fill(p.canvas)
                        .inner_margin(egui::Margin::same(12.0))
                        .stroke(egui::Stroke::new(1.0, p.border)),
                )
                .show(ctx, |ui| results_dup_list_panel(self, ui, ctx));
        }

        // Staging panel: a right side panel, so it must be added before the central panel.
        crate::ui::staging_panel(self, ctx);

        let central_margin = match mode {
            AppMode::Explorer | AppMode::AllFiles => egui::Margin::symmetric(16.0, 12.0),
            _ if crate::ui::style::is_narrow(ctx) => egui::Margin::symmetric(14.0, 12.0),
            _ => egui::Margin::symmetric(24.0, 18.0),
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
                    AppMode::AllFiles => crate::ui::recursive_files_panel::recursive_files_page(self, ui),
                }
                // Render global player modal globally so it functions in all modes/views
                crate::ui::folder_contents_modal(self, ctx);
                crate::ui::player_window(self, ctx);
                crate::ui::confirm_pending_action(self, ctx);
            });
    }
}

/// Move a file as fast as possible:
/// 1. `rename(2)` — atomic, instant on same filesystem.
/// 2. `cp -c` — APFS copy-on-write clone (macOS), instant regardless of file size.
/// 3. `cp`    — regular copy for non-APFS volumes, then delete source.
fn fast_move(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    // Safety guard: NEVER overwrite an existing destination file, directory, or symlink!
    if dst.exists() || dst.symlink_metadata().is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("Destination already exists: {}", dst.display()),
        ));
    }

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

/// Move a directory as a unit: rename when possible, else recursive copy + delete.
fn fast_move_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    // Safety guard: NEVER overwrite an existing destination directory, file, or symlink!
    if dst.exists() || dst.symlink_metadata().is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("Destination directory already exists: {}", dst.display()),
        ));
    }

    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("cp")
            .args(["-R", "--"])
            .arg(src)
            .arg(dst)
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            std::fs::remove_dir_all(src)?;
            return Ok(());
        }
        // Clean partial dest if `cp` left anything behind.
        let _ = std::fs::remove_dir_all(dst);
    }

    copy_dir_recursive(src, dst)?;
    std::fs::remove_dir_all(src)?;
    Ok(())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src).follow_links(false) {
        let entry = entry?;
        let rel = match entry.path().strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest_path = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

/// Unique destination for a directory: `Name`, then `Name_2`, `Name_3`, …
/// Never overwrites an existing directory, file, or symlink.
pub fn unique_dir_dest(target_dir: &std::path::Path, folder_name: &str) -> PathBuf {
    let mut dest = target_dir.join(folder_name);
    let mut counter = 2u32;
    while dest.exists() || dest.symlink_metadata().is_ok() {
        dest = target_dir.join(format!("{folder_name}_{counter}"));
        counter += 1;
    }
    dest
}

/// Unique non-colliding destination path for a file: `Name.ext`, then `Name_1.ext`, `Name_2.ext`, …
/// Never overwrites an existing file, directory, or symlink.
pub fn unique_file_dest(target_dir: &std::path::Path, file_path: &std::path::Path) -> PathBuf {
    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let original_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    let mut dest = target_dir.join(original_name);
    let mut counter = 1u32;
    while dest.exists() || dest.symlink_metadata().is_ok() {
        let new_name = if ext.is_empty() {
            format!("{stem}_{counter}")
        } else {
            format!("{stem}_{counter}.{ext}")
        };
        dest = target_dir.join(&new_name);
        counter += 1;
    }
    dest
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
    // AppleDouble / Finder metadata (`._photo.jpg`) — always hide, even when
    // showing other hidden files.
    if name.starts_with("._") {
        return true;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_entry_exclusions() {
        assert!(is_system_entry(".git", true));
        assert!(is_system_entry("node_modules", true));
        assert!(is_system_entry("__pycache__", true));
        assert!(is_system_entry("$recycle.bin", true));
        assert!(is_system_entry(".DS_Store", false));
        assert!(is_system_entry(".ds_store", false));
        assert!(is_system_entry("._photo.jpg", false));
        assert!(is_system_entry("thumbs.db", false));

        // Normal and custom hidden folders should NOT be considered system junk
        assert!(!is_system_entry("Documents", true));
        assert!(!is_system_entry(".config", true));
        assert!(!is_system_entry(".secret_archive", true));
        assert!(!is_system_entry(".hidden_photos", true));
        assert!(!is_system_entry("vacation_2024", true));
        assert!(!is_system_entry("photo.jpg", false));
    }

    #[test]
    fn test_folder_explorer_listing_and_system_filtering() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sfm_folder_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create normal folders, hidden folders, and system junk folders
        let normal_dir1 = temp_dir.join("Albums");
        let normal_dir2 = temp_dir.join("Invoices");
        let hidden_dir = temp_dir.join(".custom_hidden_folder");
        let sys_git = temp_dir.join(".git");
        let sys_node = temp_dir.join("node_modules");

        std::fs::create_dir_all(&normal_dir1).unwrap();
        std::fs::create_dir_all(&normal_dir2).unwrap();
        std::fs::create_dir_all(&hidden_dir).unwrap();
        std::fs::create_dir_all(&sys_git).unwrap();
        std::fs::create_dir_all(&sys_node).unwrap();

        // Add dummy files
        std::fs::write(temp_dir.join(".DS_Store"), b"junk").unwrap();
        std::fs::write(temp_dir.join("root_note.txt"), b"hello").unwrap();

        let mut app = SortwiseApp::default();
        app.explorer_dir = temp_dir.clone();
        app.explorer_view_mode = ExplorerViewMode::Folders;
        app.explorer_show_hidden = true;
        app.explorer_reload();

        let dir_names: Vec<String> = app
            .explorer_entries
            .iter()
            .filter(|e| e.is_dir)
            .map(|e| e.name.clone())
            .collect();

        // Must include normal and hidden folders
        assert!(dir_names.contains(&"Albums".to_string()));
        assert!(dir_names.contains(&"Invoices".to_string()));
        assert!(dir_names.contains(&".custom_hidden_folder".to_string()));

        // Must NOT include system junk folders or system files
        assert!(!dir_names.contains(&".git".to_string()));
        assert!(!dir_names.contains(&"node_modules".to_string()));
        assert!(!dir_names.contains(&".DS_Store".to_string()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_folder_modal_and_carousel_navigation() {
        let temp_dir = std::env::temp_dir().join(format!(
            "sfm_carousel_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let f1 = temp_dir.join("Folder_A");
        let f2 = temp_dir.join("Folder_B");
        let f3 = temp_dir.join("Folder_C");
        std::fs::create_dir_all(&f1).unwrap();
        std::fs::create_dir_all(&f2).unwrap();
        std::fs::create_dir_all(&f3).unwrap();

        // Put files in Folder_A
        std::fs::write(f1.join("image1.png"), b"img").unwrap();
        std::fs::write(f1.join("doc1.pdf"), b"pdf").unwrap();

        let mut app = SortwiseApp::default();
        app.explorer_dir = temp_dir.clone();
        app.explorer_reload();

        // 1. Open Folder_A in modal
        app.folder_modal_open_dir(f1.clone());
        assert!(app.folder_modal_open);
        assert_eq!(app.folder_modal_path, Some(f1.clone()));
        assert_eq!(app.folder_modal_entries.len(), 2);

        // 2. Navigate Next -> Folder_B
        app.folder_modal_next();
        assert_eq!(app.folder_modal_path, Some(f2.clone()));

        // 3. Navigate Next -> Folder_C
        app.folder_modal_next();
        assert_eq!(app.folder_modal_path, Some(f3.clone()));

        // 4. Navigate Prev -> Folder_B
        app.folder_modal_prev();
        assert_eq!(app.folder_modal_path, Some(f2.clone()));

        // 5. Open in Grid -> updates explorer_dir and closes modal
        app.folder_modal_open_in_grid();
        assert!(!app.folder_modal_open);
        assert_eq!(app.explorer_dir, f2);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_queue_and_dequeue_features() {
        let mut app = SortwiseApp::default();
        let target_dir = std::env::temp_dir().join("sfm_queue_target");
        app.explorer_queues.push(FileQueue {
            name: "Sorted Photos".to_string(),
            target: target_dir,
            items: Vec::new(),
        });

        let test_folder = PathBuf::from("/Users/test/UnmanagedFolder");
        let test_file = PathBuf::from("/Users/test/UnmanagedFolder/photo.jpg");

        // 1. Test staging folder
        assert!(!app.explorer_is_in_queue(0, &test_folder));
        app.explorer_toggle_queue(0, test_folder.clone());
        assert!(app.explorer_is_in_queue(0, &test_folder));
        assert_eq!(app.explorer_queues[0].items.len(), 1);

        // 2. Test dequeuing folder
        app.explorer_toggle_queue(0, test_folder.clone());
        assert!(!app.explorer_is_in_queue(0, &test_folder));
        assert_eq!(app.explorer_queues[0].items.len(), 0);

        // 3. Test staging file
        assert!(!app.explorer_is_in_queue(0, &test_file));
        app.explorer_toggle_queue(0, test_file.clone());
        assert!(app.explorer_is_in_queue(0, &test_file));
        assert_eq!(app.explorer_queues[0].items.len(), 1);

        // 3b. Test finding queue for path & auto-selection in preview
        assert_eq!(app.explorer_find_queue_for_path(&test_file), Some(0));
        assert_eq!(app.explorer_find_queue_for_path(&PathBuf::from("/non/existent")), None);
        app.explorer_preview_queue_pick = None;
        app.explorer_open_preview(test_file.clone());
        assert_eq!(app.explorer_preview_queue_pick, Some(0));

        // 4. Test explicit remove
        app.explorer_remove_from_queue(0, test_file.clone());
        assert!(!app.explorer_is_in_queue(0, &test_file));
        assert_eq!(app.explorer_queues[0].items.len(), 0);
    }

    #[test]
    fn test_explorer_performance_and_epoch_invariants() {
        let mut app = SortwiseApp::default();
        let initial_epoch = app.navigation_epoch.load(std::sync::atomic::Ordering::SeqCst);
        assert!(initial_epoch >= 1);

        // Reload bumps epoch
        app.explorer_reload();
        let new_epoch = app.navigation_epoch.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(new_epoch, initial_epoch + 1);

        // Sorting is case-insensitive and zero-alloc
        app.explorer_entries = vec![
            ExplorerEntry {
                name: "b_file.txt".to_string(),
                path: PathBuf::from("/test/b_file.txt"),
                is_dir: false,
                size: 10,
                modified: None,
            },
            ExplorerEntry {
                name: "A_folder".to_string(),
                path: PathBuf::from("/test/A_folder"),
                is_dir: true,
                size: 0,
                modified: None,
            },
            ExplorerEntry {
                name: "a_file.txt".to_string(),
                path: PathBuf::from("/test/a_file.txt"),
                is_dir: false,
                size: 20,
                modified: None,
            },
        ];
        app.explorer_sort_key = ExplorerSortKey::Name;
        app.explorer_sort_asc = true;
        app.explorer_sort_entries();

        // Folders first:
        assert!(app.explorer_entries[0].is_dir);
        assert_eq!(app.explorer_entries[0].name, "A_folder");
        // Then files sorted case-insensitively:
        assert_eq!(app.explorer_entries[1].name, "a_file.txt");
        assert_eq!(app.explorer_entries[2].name, "b_file.txt");
    }

    #[test]
    fn test_unique_destinations_prevent_overwrites() {
        let temp = std::env::temp_dir().join(format!(
            "sfm_collision_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp).unwrap();

        // 1. Test unique_file_dest:
        let f1 = temp.join("report.pdf");
        std::fs::write(&f1, b"original").unwrap();

        let src_file = PathBuf::from("/anywhere/report.pdf");
        let dest1 = unique_file_dest(&temp, &src_file);
        assert_eq!(dest1, temp.join("report_1.pdf"));

        // If report_1.pdf also exists:
        std::fs::write(&dest1, b"first collision").unwrap();
        let dest2 = unique_file_dest(&temp, &src_file);
        assert_eq!(dest2, temp.join("report_2.pdf"));

        // 2. Test unique_dir_dest:
        let d1 = temp.join("Photos");
        std::fs::create_dir_all(&d1).unwrap();

        let dest_d1 = unique_dir_dest(&temp, "Photos");
        assert_eq!(dest_d1, temp.join("Photos_2"));

        // If Photos_2 also exists:
        std::fs::create_dir_all(&dest_d1).unwrap();
        let dest_d2 = unique_dir_dest(&temp, "Photos");
        assert_eq!(dest_d2, temp.join("Photos_3"));

        // 3. Test fast_move safety guard:
        let dummy_src = temp.join("dummy.txt");
        std::fs::write(&dummy_src, b"test").unwrap();
        let err = fast_move(&dummy_src, &f1);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);

        // 4. Test fast_move_dir safety guard:
        let dummy_dir = temp.join("dummy_dir");
        std::fs::create_dir_all(&dummy_dir).unwrap();
        let err_dir = fast_move_dir(&dummy_dir, &d1);
        assert!(err_dir.is_err());
        assert_eq!(err_dir.unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_movable_queue_individual_and_bulk_move_with_renaming() {
        let temp = std::env::temp_dir().join(format!(
            "sfm_movable_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let src_dir = temp.join("source");
        let dest_a = temp.join("Person_A");
        let dest_b = temp.join("Person_B");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&dest_a).unwrap();
        std::fs::create_dir_all(&dest_b).unwrap();

        // Populate source video files
        let vid1 = src_dir.join("clip1.mp4");
        let vid2 = src_dir.join("clip2.mp4");
        let vid3 = src_dir.join("clip3.mp4");
        std::fs::write(&vid1, b"video 1 content").unwrap();
        std::fs::write(&vid2, b"video 2 content").unwrap();
        std::fs::write(&vid3, b"video 3 content").unwrap();

        // Create an existing file in dest_a with the exact same name clip1.mp4
        std::fs::write(dest_a.join("clip1.mp4"), b"existing file").unwrap();

        let mut app = SortwiseApp::default();
        app.explorer_dir = src_dir.clone();

        // Register destination queues
        app.explorer_queues.push(FileQueue {
            name: "Person A".to_string(),
            target: dest_a.clone(),
            items: Vec::new(),
        });
        app.explorer_queues.push(FileQueue {
            name: "Person B".to_string(),
            target: dest_b.clone(),
            items: Vec::new(),
        });

        // 1. Stage items into movable queue
        app.toggle_movable(vid1.clone());
        app.toggle_movable(vid2.clone());
        app.toggle_movable(vid3.clone());

        assert_eq!(app.movable_items.len(), 3);
        assert!(app.is_in_movable(&vid1));
        assert!(app.is_in_movable(&vid2));
        assert!(app.is_in_movable(&vid3));

        // 2. Assign queues
        app.movable_items[0].target_queue = Some("Person A".to_string());
        app.movable_items[1].target_queue = Some("Person B".to_string());
        // item 2 left unassigned for now

        // 3. Move individual item 0 (clip1.mp4 to Person A)
        let moved = app.move_single_movable_item(0);
        assert!(moved);
        assert_eq!(app.movable_items.len(), 2);
        assert!(!vid1.exists(), "Original file should be moved");
        // Check automatic non-destructive renaming
        assert!(dest_a.join("clip1.mp4").exists(), "Existing destination file must remain intact");
        assert!(dest_a.join("clip1_1.mp4").exists(), "Colliding file should be automatically renamed to clip1_1.mp4");

        // 4. Assign queue for remaining item and move bulk
        app.movable_items[1].target_queue = Some("Person B".to_string());
        let (success, failed) = app.move_all_movable_items();
        assert_eq!(success, 2);
        assert_eq!(failed, 0);
        assert_eq!(app.movable_items.len(), 0);
        assert!(dest_b.join("clip2.mp4").exists());
        assert!(dest_b.join("clip3.mp4").exists());

        let _ = std::fs::remove_dir_all(&temp);
    }

    fn files_tab_rec(path: &str) -> sortwise::recursive_files::RecursiveFile {
        sortwise::recursive_files::RecursiveFile {
            path: PathBuf::from(path),
            rel: path.to_string(),
            size: 1,
            modified: None,
        }
    }

    #[test]
    fn files_tab_stage_goes_to_folder_queue_or_items_never_both() {
        let mut app = SortwiseApp::default();
        app.explorer_queues.push(FileQueue { name: "A".into(), target: PathBuf::from("/dest/a"), items: vec![] });
        app.explorer_queues.push(FileQueue { name: "B".into(), target: PathBuf::from("/dest/b"), items: vec![] });
        let f = PathBuf::from("/x/a.mp4");

        // Picking a queue lists the file in that folder queue (Folder queues tab).
        assert_eq!(app.explorer_files_stage(&[f.clone()], Some("A".into())), 1);
        assert_eq!(app.explorer_queues[0].items, vec![f.clone()]);
        assert!(app.movable_items.is_empty());
        assert_eq!(app.explorer_files_stage_of(&f), FilesStage::Queue("A".into()));

        // Re-picking another queue moves it there; it is never in two queues.
        app.explorer_files_stage(&[f.clone()], Some("B".into()));
        assert!(app.explorer_queues[0].items.is_empty());
        assert_eq!(app.explorer_queues[1].items, vec![f.clone()]);

        // No queue: goes to Items only.
        app.explorer_files_stage(&[f.clone()], None);
        assert!(app.explorer_queues.iter().all(|q| q.items.is_empty()));
        assert_eq!(app.explorer_files_stage_of(&f), FilesStage::Item(None));

        // Unknown queue changes nothing.
        assert_eq!(app.explorer_files_stage(&[f.clone()], Some("Nope".into())), 0);
        assert_eq!(app.explorer_files_stage_of(&f), FilesStage::Item(None));

        // Added from the preview modal (folder queue path) is seen by the Files view too.
        app.explorer_files_unstage(&f);
        assert_eq!(app.explorer_files_stage_of(&f), FilesStage::None);
        let before = app.explorer_files_staged_fingerprint();
        app.explorer_add_to_queue(0, f.clone());
        assert!(app.explorer_files_staged_set().contains(&f));
        assert_ne!(before, app.explorer_files_staged_fingerprint());
    }

    #[test]
    fn files_tab_moved_file_leaves_list_and_folder_move_removes_children() {
        let mut app = SortwiseApp::default();
        app.explorer_files = vec![
            files_tab_rec("/x/a.mp4"),
            files_tab_rec("/x/dir/b.mp4"),
            files_tab_rec("/x/dir/c.zip"),
            files_tab_rec("/x/d.zip"),
        ];
        app.explorer_files_checked.insert(PathBuf::from("/x/a.mp4"));

        app.explorer_files_forget_path(Path::new("/x/a.mp4"), false);
        assert_eq!(app.explorer_files.len(), 3);
        assert!(!app.explorer_files_checked.contains(Path::new("/x/a.mp4")));

        // A moved folder takes every file under it with it, but not similar prefixes.
        app.explorer_files_forget_path(Path::new("/x/dir"), true);
        let left: Vec<&str> = app.explorer_files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(left, vec!["/x/d.zip"]);
    }

    #[test]
    fn files_tab_prune_drops_missing_and_refresh_sets_rescan() {
        let temp = std::env::temp_dir().join(format!(
            "sfm_files_prune_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let keep = temp.join("keep.zip");
        std::fs::write(&keep, b"k").unwrap();
        let mut app = SortwiseApp::default();
        app.explorer_files = vec![
            files_tab_rec(&keep.to_string_lossy()),
            files_tab_rec(&temp.join("gone.zip").to_string_lossy()),
        ];
        app.explorer_files_checked.insert(temp.join("gone.zip"));

        app.explorer_files_prune_missing();
        assert_eq!(app.explorer_files.len(), 1);
        assert!(app.explorer_files_checked.is_empty());

        app.explorer_files_refresh();
        assert!(app.explorer_files_rescan);
        let _ = std::fs::remove_dir_all(&temp);
    }
}

