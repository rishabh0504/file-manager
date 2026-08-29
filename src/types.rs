use crate::errors::FileError;
use crate::platform::FileId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionMode {
    DryRun,
    Delete,
    HardLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeepPolicy {
    Oldest,
    ShortestPath,
    SmartScore,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: SystemTime,
    /// Modification time as seconds since UNIX epoch, pre-computed at scan time.
    /// Used by the pipeline and hash cache — avoids repeated stat() syscalls.
    pub mtime_secs: u64,
    pub file_id: FileId,
    /// Unix link count when known; 1 on Windows if unknown.
    pub nlink: u32,
    pub root_index: usize,
    /// Best-effort access time (seconds since UNIX epoch); 0 if unavailable.
    pub atime_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardLinkGroup {
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub hash: String,
    pub files: Vec<FileEntrySnapshot>,
    pub total_wasted_bytes: u64,
    pub keep_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntrySnapshot {
    pub path: PathBuf,
    pub size: u64,
    pub score: i64,
    pub is_keep: bool,
    pub root_index: usize,
}

#[derive(Debug, Clone)]
pub struct FolderConfig {
    pub root: PathBuf,
    pub excluded_subdirs: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub roots: Vec<PathBuf>,
    pub folder_configs: Vec<FolderConfig>,
    /// When true, skips files/dirs whose name starts with `.`.
    /// Defaults to FALSE — hidden directories like `.photos/`, `.local/` are
    /// traversed so media files inside them are found. System dirs (.git,
    /// node_modules, etc.) are always excluded separately via `skip_system_dirs`.
    pub skip_hidden: bool,
    pub skip_system_dirs: bool,
    pub min_file_size: u64,
    pub action_mode: ActionMode,
    pub preferred_dirs: Vec<PathBuf>,
    /// Root order for scoring (first = highest preference).
    pub root_order: Vec<PathBuf>,
    pub exclude_globs: Vec<String>,
    pub use_hash_cache: bool,
    pub skip_logs_and_locks: bool,
    pub io_profile: IoProfile,
    /// When Some(split_idx), roots 0..split_idx are Source (A) and roots split_idx..roots.len() are Destination (B).
    /// Asymmetric mode: only find duplicates in B that exist in A. Internal duplicates in A or B are ignored.
    pub asymmetric_split_index: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IoProfile {
    #[default]
    Auto,
    HddSafe,
    SsdFast,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            folder_configs: Vec::new(),
            skip_hidden: false,
            skip_system_dirs: true,
            min_file_size: 1,
            action_mode: ActionMode::DryRun,
            preferred_dirs: Vec::new(),
            root_order: Vec::new(),
            exclude_globs: Vec::new(),
            use_hash_cache: true,
            skip_logs_and_locks: false,
            io_profile: IoProfile::default(),
            asymmetric_split_index: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ProgressPhase {
    Enumerating {
        files_found: u64,
        current_path: PathBuf,
    },
    Processing {
        stage_label: String,
        stage: u8,
        stages_total: u8,
    },
    Finishing,
}

#[derive(Debug, Clone)]
pub struct FolderProgress {
    pub folder_path: PathBuf,
    pub files_total: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub bytes_done: u64,
    pub status: FolderStatus,
}

#[derive(Debug, Clone)]
pub enum FolderStatus {
    Queued,
    Scanning,
    Hashing,
    Done { groups_found: u32 },
    Cancelled,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub phase: ProgressPhase,
    pub bytes_processed: u64,
    pub bytes_total: u64,
    pub files_hashed: u64,
    pub files_to_hash_total: u64,
    pub groups_found: u64,
    pub current_path: PathBuf,
    pub folders: Vec<FolderProgress>,
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self {
            phase: ProgressPhase::Enumerating {
                files_found: 0,
                current_path: PathBuf::new(),
            },
            bytes_processed: 0,
            bytes_total: 0,
            files_hashed: 0,
            files_to_hash_total: 0,
            groups_found: 0,
            current_path: PathBuf::new(),
            folders: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStats {
    pub files_scanned: u64,
    pub files_skipped_error: u64,
    pub duplicate_groups: u64,
    pub total_wasted_bytes: u64,
    pub partial: bool,
    pub hard_link_groups: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderSummary {
    pub root: PathBuf,
    pub files_scanned: u64,
    pub duplicate_groups: u32,
    pub total_wasted_bytes: u64,
    pub error_count: u32,
    pub external_hardlinks: u32,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub duplicate_groups: Vec<DuplicateGroup>,
    pub hard_link_groups: Vec<HardLinkGroup>,
    pub stats: ScanStats,
    pub error_log: Vec<(PathBuf, String)>,
    pub folder_summaries: Vec<FolderSummary>,
    pub cross_folder_conflicts: Vec<CrossFolderConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossFolderConflict {
    pub hash_hex: String,
    pub paths: Vec<PathBuf>,
    pub keep_index: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionManifest {
    pub actions: Vec<ManifestAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestAction {
    pub op: String,
    pub path: PathBuf,
    pub keep_path: Option<PathBuf>,
}

impl ScanResult {
    pub fn empty_partial() -> Self {
        Self {
            duplicate_groups: Vec::new(),
            hard_link_groups: Vec::new(),
            stats: ScanStats {
                files_scanned: 0,
                files_skipped_error: 0,
                duplicate_groups: 0,
                total_wasted_bytes: 0,
                partial: true,
                hard_link_groups: 0,
            },
            error_log: Vec::new(),
            folder_summaries: Vec::new(),
            cross_folder_conflicts: Vec::new(),
        }
    }
}

pub fn log_error(errors: &mut Vec<(PathBuf, String)>, path: PathBuf, err: FileError) {
    const MAX: usize = 1000;
    if errors.len() >= MAX {
        return;
    }
    errors.push((path, err.to_string()));
}
