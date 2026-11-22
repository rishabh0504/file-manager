//! Organized state structs for SortwiseApp.

use crate::types::*;
use sortwise::types::{ActionMode, ScanProgress, ScanResult, IoProfile, DuplicateGroup};
use sortwise::recursive_files::RecursiveFile;
use sortwise::settings::Settings;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;
use std::collections::{HashSet, HashMap};
use std::path::PathBuf;
use std::time::Instant;
use eframe::egui;

/// Explorer browsing state: navigation, selection, filtering, preview.
#[derive(Default)]
pub struct ExplorerState {
    pub dir: PathBuf,
    pub entries: Vec<ExplorerEntry>,
    pub loaded_dir: Option<PathBuf>,
    pub back: Vec<PathBuf>,
    pub fwd: Vec<PathBuf>,
    pub selected: Option<usize>,
    pub multi: HashSet<PathBuf>,
    pub anchor: Option<usize>,
    pub error: Option<String>,
    pub show_hidden: bool,
    pub sort_key: ExplorerSortKey,
    pub sort_asc: bool,
    pub view_mode: ExplorerViewMode,
    pub type_filter: ExplorerTypeFilter,
    pub filter: String,
    pub last_filter: String,
    pub group_by_kind: bool,
    pub preview_open: bool,
    pub preview_path: Option<PathBuf>,
    pub preview_rotation: i32,
    pub status: Option<String>,
    pub search_rx: Option<Receiver<Vec<ExplorerEntry>>>,
    pub search_token: Option<sortwise::cancel::CancellationToken>,
    pub searching: bool,
    pub is_recursive_search: bool,
    pub search_typed_at: Option<Instant>,
    pub search_pending: bool,
    pub search_checked: HashSet<PathBuf>,
    pub files_root: Option<PathBuf>,
    pub files: Vec<RecursiveFile>,
    pub files_loading: bool,
    pub files_truncated: bool,
    pub files_rx: Option<Receiver<sortwise::recursive_files::Listing>>,
    pub files_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub files_checked: HashSet<PathBuf>,
    pub files_sort: ExplorerFilesSort,
    pub files_sort_asc: bool,
    pub files_view: Vec<usize>,
    pub files_view_key: Option<(String, ExplorerTypeFilter, ExplorerFilesSort, bool, u64)>,
    pub files_gen: u64,
    #[cfg(feature = "mpv")]
    pub mpv: Option<crate::ui::mpv_player::MpvPlayer>,
    pub mpv_loaded_path: Option<PathBuf>,
}

/// Queue/movable operations state.
#[derive(Default)]
pub struct QueueState {
    pub queues: Vec<FileQueue>,
    pub queue_view_open: bool,
    pub active_queue: Option<usize>,
    pub preview_queue_pick: Option<usize>,
    pub queue_submitting: bool,
    pub queue_submit_rx: Option<std::sync::mpsc::Receiver<(usize, usize)>>,
    pub queue_submit_progress: Arc<Mutex<(usize, usize, String)>>,
    pub queue_submitting_idx: Option<usize>,
    pub movable_items: Vec<MovableItem>,
    pub movable_queue_open: bool,
    pub movable_playing_path: Option<PathBuf>,
    pub movable_last_chosen_queue: Option<String>,
    pub movable_size_cache: HashMap<PathBuf, u64>,
}

/// Duplicate finding and results state.
#[derive(Default)]
pub struct DuplicatesState {
    pub dup_list_cache: DupListCache,
    pub dup_group_sort: DupGroupSortOrder,
    pub dup_list_scroll_to_group: Option<usize>,
    pub dedupe_twins_modal_open: bool,
    pub archive_files_results: Vec<PathBuf>,
    pub archive_files_selected: HashSet<PathBuf>,
    pub archive_files_scanning: bool,
    pub archive_files_rx: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    pub archive_files_progress_count: Arc<Mutex<u64>>,
    pub archive_files_progress_path: Arc<Mutex<PathBuf>>,
    pub archive_files_move_target: Option<PathBuf>,
    pub archive_files_moving: bool,
    pub archive_files_move_rx: Option<std::sync::mpsc::Receiver<(usize, usize)>>,
    pub archive_files_move_progress: Arc<Mutex<(usize, usize, String)>>,
}

/// Scan configuration and progress.
pub struct ScanState {
    pub roots: Vec<PathBuf>,
    pub source_roots: Vec<PathBuf>,
    pub dest_roots: Vec<PathBuf>,
    pub preferred_dirs_text: String,
    pub exclude_globs_text: String,
    pub excluded_subdirs_text: String,
    pub skip_hidden: bool,
    pub skip_system_dirs: bool,
    pub min_file_size: u64,
    pub action_mode: ActionMode,
    pub use_hash_cache: bool,
    pub skip_logs_and_locks: bool,
    pub io_profile: IoProfile,
    pub token: sortwise::cancel::CancellationToken,
    pub progress: Arc<Mutex<ScanProgress>>,
    pub result_rx: Option<Receiver<ScanResult>>,
    pub scan_work: ScanWorkKind,
    pub last_result: Option<Arc<ScanResult>>,
    pub results_epoch: u64,
    pub pending_action: Option<PendingAction>,
    pub results_kind_filter: Option<sortwise::heuristics::MediaKind>,
    pub live_groups: Vec<DuplicateGroup>,
    pub live_rx: Option<Receiver<Vec<DuplicateGroup>>>,
    pub scan_started_at: Option<Instant>,
    pub saved_settings: Settings,
    pub results_selected_group: Option<usize>,
    pub results_last_selected_group: Option<usize>,
    pub results_selected_files: HashSet<PathBuf>,
    pub results_selected_file_in_group: usize,
    pub results_filter: String,
    pub apply_message: Option<String>,
    pub apply_errors: Option<String>,
    pub empty_folders_results: Vec<PathBuf>,
    pub empty_folders_selected: HashSet<PathBuf>,
    pub empty_folders_rx: Option<std::sync::mpsc::Receiver<Vec<PathBuf>>>,
    pub empty_folders_progress_count: Arc<Mutex<u64>>,
    pub empty_folders_progress_path: Arc<Mutex<PathBuf>>,
}

impl Default for ScanState {
    fn default() -> Self {
        Self {
            roots: Default::default(),
            source_roots: Default::default(),
            dest_roots: Default::default(),
            preferred_dirs_text: Default::default(),
            exclude_globs_text: Default::default(),
            excluded_subdirs_text: Default::default(),
            skip_hidden: Default::default(),
            skip_system_dirs: Default::default(),
            min_file_size: Default::default(),
            action_mode: Default::default(),
            use_hash_cache: Default::default(),
            skip_logs_and_locks: Default::default(),
            io_profile: Default::default(),
            token: sortwise::cancel::CancellationToken::new(),
            progress: Arc::new(Mutex::new(Default::default())),
            result_rx: None,
            scan_work: Default::default(),
            last_result: None,
            results_epoch: 0,
            pending_action: None,
            results_kind_filter: None,
            live_groups: Default::default(),
            live_rx: None,
            scan_started_at: None,
            saved_settings: Default::default(),
            results_selected_group: None,
            results_last_selected_group: None,
            results_selected_files: Default::default(),
            results_selected_file_in_group: 0,
            results_filter: Default::default(),
            apply_message: None,
            apply_errors: None,
            empty_folders_results: Default::default(),
            empty_folders_selected: Default::default(),
            empty_folders_rx: None,
            empty_folders_progress_count: Arc::new(Mutex::new(0)),
            empty_folders_progress_path: Arc::new(Mutex::new(PathBuf::new())),
        }
    }
}

/// UI state: modals, previews, thumbnails, etc.
pub struct UIState {
    pub view: AppView,
    pub mode: AppMode,
    pub extract_banner: Option<ExtractBanner>,
    pub brand_logo: Option<egui::TextureHandle>,
    pub folder_modal_open: bool,
    pub folder_modal_path: Option<PathBuf>,
    pub folder_modal_entries: Vec<ExplorerEntry>,
    pub folder_modal_loaded_path: Option<PathBuf>,
    pub folder_modal_filter: String,
    pub folder_modal_selected: Option<usize>,
    pub preview_path: Option<PathBuf>,
    pub preview_rotation: i32,
    pub preview_texture: Option<egui::TextureHandle>,
    pub preview_scroll_text: Option<String>,
    pub preview_subtitle: Option<String>,
    pub thumbnail_cache: HashMap<PathBuf, egui::ColorImage>,
    pub thumbnail_loading: HashSet<PathBuf>,
    pub thumbnail_failed: HashSet<PathBuf>,
    pub thumbnail_response_rx: crossbeam_channel::Receiver<ThumbnailResponse>,
    pub thumbnail_worker_tx: crossbeam_channel::Sender<ThumbnailRequest>,
    pub shortcuts_open: bool,
    pub snapshot: Option<SnapshotPlan>,
    pub show_error_log: bool,
    pub inline_video: Option<crate::ui::video_inline::InlineVideoDecoder>,
    pub navigation_epoch: Arc<std::sync::atomic::AtomicUsize>,
    pub folder_overlap: HashMap<(PathBuf, PathBuf), usize>,
    pub twins_trash_selections: HashMap<(PathBuf, PathBuf), bool>,
    pub results_filter_folders: Option<(PathBuf, PathBuf)>,
    pub twins_compare_pair: Option<(PathBuf, PathBuf)>,
    pub video_volume: f64,
}

impl Default for UIState {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            view: Default::default(),
            mode: Default::default(),
            extract_banner: None,
            brand_logo: None,
            folder_modal_open: false,
            folder_modal_path: None,
            folder_modal_entries: Default::default(),
            folder_modal_loaded_path: None,
            folder_modal_filter: Default::default(),
            folder_modal_selected: None,
            preview_path: None,
            preview_rotation: 0,
            preview_texture: None,
            preview_scroll_text: None,
            preview_subtitle: None,
            thumbnail_cache: Default::default(),
            thumbnail_loading: Default::default(),
            thumbnail_failed: Default::default(),
            thumbnail_response_rx: rx,
            thumbnail_worker_tx: tx,
            shortcuts_open: false,
            snapshot: None,
            show_error_log: false,
            inline_video: None,
            navigation_epoch: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            folder_overlap: Default::default(),
            twins_trash_selections: Default::default(),
            results_filter_folders: None,
            twins_compare_pair: None,
            video_volume: 1.0,
        }
    }
}
