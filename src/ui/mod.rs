mod explorer_panel;
#[cfg(feature = "mpv")]
pub mod mpv_player;
mod preview;
mod progress_panel;
mod results_panel;
mod scan_panel;
pub mod style;
pub mod video_inline;
pub mod folder_intelligence;
pub mod empty_folders_panel;
pub mod archive_files_panel;

pub use explorer_panel::{explorer_main_panel, explorer_sidebar, player_window, queue_window};
pub use progress_panel::progress_panel;
pub use results_panel::{results_dup_list_panel, results_main_panel};
pub use scan_panel::scan_panel;
pub use empty_folders_panel::{empty_folders_scan_panel, empty_folders_progress_panel, empty_folders_results_panel};
pub use archive_files_panel::{archive_files_scan_panel, archive_files_progress_panel, archive_files_results_panel};
