pub mod confirm_dialog;
pub mod icons;
pub mod modal;
pub mod explorer_panel;
#[cfg(feature = "mpv")]
pub mod mpv_player;
mod preview;
mod progress_panel;
mod results_panel;
pub(crate) mod scan_panel;
pub mod style;
pub mod video_inline;
pub mod folder_intelligence;
pub mod empty_folders_panel;
pub mod archive_files_panel;
pub mod folder_explorer_panel;
pub mod recursive_files_panel;
pub mod status_bar;
pub mod progress_indicator;
pub mod context_menu;
pub mod keyboard;
pub mod brand;
pub mod result_list;
pub mod explorer_interactions;

pub use explorer_panel::{explorer_main_panel, explorer_sidebar, player_window, staging_panel};
pub use explorer_interactions::{handle_explorer_keys, show_enhanced_file_context_menu, apply_key_action, apply_context_action};
pub use folder_explorer_panel::{folder_contents_modal, render_folder_grid};
pub use confirm_dialog::confirm_pending_action;
pub use progress_panel::progress_panel;
pub use results_panel::{results_dup_list_panel, results_main_panel};
pub use scan_panel::scan_panel;
pub use empty_folders_panel::{empty_folders_scan_panel, empty_folders_progress_panel, empty_folders_results_panel};
pub use archive_files_panel::{archive_files_scan_panel, archive_files_progress_panel, archive_files_results_panel};

