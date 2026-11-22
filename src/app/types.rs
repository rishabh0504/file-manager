//! App-level types, enums and constants.
use super::*;

/// Idle time after the last keystroke before recursive search fires.
pub(crate) const SEARCH_DEBOUNCE: Duration = Duration::from_secs(2);

/// Media / archive extensions used by recursive search and folder→queue expand.
pub(crate) const MEDIA_EXTS: &[&str] = &[
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

/// A running zip extraction: worker result channel, the archive, and its new destination folder.
pub type ExtractJob = (std::sync::mpsc::Receiver<Result<(), String>>, PathBuf, PathBuf);
