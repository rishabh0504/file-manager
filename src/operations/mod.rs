//! File operation modules: move, delete, queue, archive.

pub mod move_ops;
pub mod queue_ops;
pub mod movable_ops;
pub mod archive_ops;

pub use move_ops::{fast_move, fast_move_dir, unique_file_dest, unique_dir_dest};
pub use queue_ops::{
    explorer_new_queue, explorer_add_to_queue, explorer_remove_from_queue,
    explorer_delete_queue, explorer_toggle_queue, explorer_is_in_queue,
    explorer_find_queue_for_path, explorer_change_queue_target, submit_all_queues,
    poll_queue_submit_result,
};
pub use movable_ops::{
    is_in_movable, stage_paths, toggle_movable, remove_from_movable,
    clear_movable_items, move_single_movable_item, move_all_movable_items,
};
pub use archive_ops::{execute_move_archive_files, poll_archive_files_move_result};
