//! Type definitions: library types + app-specific types organized by concern.

pub mod library;
pub mod app_view;
pub mod explorer;
pub mod queue;
pub mod scan;
pub mod thumbnails;
pub mod actions;
pub mod cache;

// Re-export library types
pub use library::{
    ActionMode, KeepPolicy, FileEntry, HardLinkGroup, DuplicateGroup, FileEntrySnapshot,
    FolderConfig, ScanConfig, IoProfile, ScanProgress, FolderStatus, FolderProgress,
    ScanResult, ScanStats, ProgressPhase, ActionManifest, ManifestAction,
    CrossFolderConflict, log_error, FolderSummary,
};

// Re-export app-specific types
pub use app_view::{AppView, AppMode};
pub use explorer::{ExplorerSortKey, ExplorerViewMode, ExplorerFilesSort, ExplorerTypeFilter, ExplorerEntry};
pub use queue::{FileQueue, MovableItem, GroupOutcome};
pub use scan::{ScanWorkKind, DupGroupSortOrder};
pub use thumbnails::{ThumbnailRequest, SnapshotPlan, ThumbnailResponse};
pub use actions::PendingAction;
pub use cache::{ExtractBanner, DupListCache};
