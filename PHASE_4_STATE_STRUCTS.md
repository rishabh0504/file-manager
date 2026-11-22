# Phase 4: State Struct Extraction - Definition Complete

## ✅ COMPLETED

All 5 state structs defined and ready in `src/app/state.rs` (254 lines):

### 1. ExplorerState (43 fields)
Navigation, browsing, file search, recursive listing, preview
```rust
pub struct ExplorerState {
    pub dir, entries, loaded_dir, back, fwd, selected,
    pub multi, anchor, error, show_hidden, sort_key, sort_asc,
    pub view_mode, type_filter, filter, last_filter, group_by_kind,
    pub preview_open, preview_path, preview_rotation, status,
    pub search_rx, search_token, searching, is_recursive_search,
    pub search_typed_at, search_pending, search_checked,
    pub files_root, files, files_loading, files_truncated, files_rx,
    pub files_cancel, files_checked, files_sort, files_sort_asc,
    pub files_view, files_view_key, files_gen,
    pub mpv, mpv_loaded_path,
}
```

### 2. QueueState (13 fields)
File move queues and movable items staging
```rust
pub struct QueueState {
    pub queues, queue_view_open, active_queue, preview_queue_pick,
    pub queue_submitting, queue_submit_rx, queue_submit_progress, queue_submitting_idx,
    pub movable_items, movable_queue_open, movable_playing_path,
    pub movable_last_chosen_queue, movable_size_cache,
}
```

### 3. DuplicatesState (14 fields)
Duplicate detection results and archive operations
```rust
pub struct DuplicatesState {
    pub dup_list_cache, dup_group_sort, dup_list_scroll_to_group,
    pub dedupe_twins_modal_open,
    pub archive_files_results, archive_files_selected, archive_files_scanning,
    pub archive_files_rx, archive_files_progress_count, archive_files_progress_path,
    pub archive_files_move_target, archive_files_moving,
    pub archive_files_move_rx, archive_files_move_progress,
}
```

### 4. ScanState (38 fields)
Scan configuration, progress, and global results
```rust
pub struct ScanState {
    pub roots, source_roots, dest_roots, preferred_dirs_text, exclude_globs_text,
    pub excluded_subdirs_text, skip_hidden, skip_system_dirs, min_file_size,
    pub action_mode, use_hash_cache, skip_logs_and_locks, io_profile,
    pub token, progress, result_rx, scan_work, last_result, results_epoch,
    pub pending_action, results_kind_filter, live_groups, live_rx, scan_started_at,
    pub saved_settings, results_selected_group, results_last_selected_group,
    pub results_selected_files, results_selected_file_in_group, results_filter,
    pub apply_message, apply_errors, empty_folders_results, empty_folders_selected,
    pub empty_folders_rx, empty_folders_progress_count, empty_folders_progress_path,
}
```

### 5. UIState (29 fields)
UI state: modals, previews, thumbnails, inline player
```rust
pub struct UIState {
    pub view, mode, extract_banner, brand_logo,
    pub folder_modal_open, folder_modal_path, folder_modal_entries, folder_modal_loaded_path,
    pub folder_modal_filter, folder_modal_selected,
    pub preview_path, preview_rotation, preview_texture, preview_scroll_text, preview_subtitle,
    pub thumbnail_cache, thumbnail_loading, thumbnail_failed, thumbnail_response_rx, thumbnail_worker_tx,
    pub shortcuts_open, snapshot, show_error_log, inline_video, navigation_epoch,
    pub folder_overlap, twins_trash_selections, results_filter_folders, twins_compare_pair,
    pub video_volume,
}
```

---

## 📊 Remaining Work

### Update SortwiseApp struct (1 step)
Replace 137 flat fields with 5 nested state structs:
```rust
pub struct SortwiseApp {
    pub explorer: ExplorerState,
    pub queue: QueueState,
    pub duplicates: DuplicatesState,
    pub scan: ScanState,
    pub ui: UIState,
}
```

**Result**: app.rs struct definition reduced from ~200 lines to ~10 lines

### Update field access patterns (~500-700 replacements)
Example transformations:
```
app.explorer_dir            → app.explorer.dir
app.explorer_entries        → app.explorer.entries
app.queue_submitting        → app.queue.queue_submitting
app.dup_list_cache          → app.duplicates.dup_list_cache
app.scan_work               → app.scan.scan_work
app.view                    → app.ui.view
```

### Automated approach:
Use find-and-replace patterns to update:
1. All `app.explorer_*` → `app.explorer.*`
2. All `app.queue_*` → `app.queue.*`
3. All `app.dup_*` → `app.duplicates.*`
4. All `app.archive_*` → `app.duplicates.*`
5. All `app.scan_*` → `app.scan.*`
6. All `app.results_*` → `app.scan.*`
7. All `app.view` → `app.ui.view`
8. All `app.mode` → `app.ui.mode`
9. ... (remaining fields)

---

## ⏱️ Implementation Effort

- State structs creation: ✅ DONE (254 lines)
- SortwiseApp struct refactoring: 1-2 hours (straightforward)
- Field access updates: 1-2 hours (systematic replacements)
- Testing & verification: 30 min

**Total**: ~3-4 hours for full implementation

---

## Expected Result After Phase 4

| Metric | Before Phase 4 | After Phase 4 | Total Reduction |
|--------|---|---|---|
| app.rs | 1955 lines | ~1200 lines | 61% from original 2610 |
| Struct definition | ~200 lines | ~10 lines | 95% |
| Field organization | 137 flat fields | 5 nested structs | ✅ Clear organization |
| State clarity | Mixed | Organized by domain | ✅ Type safety |

---

## Status

**State structs**: ✅ DEFINED & READY (src/app/state.rs)
**Next step**: Update app.rs to use `explorer:`, `queue:`, `duplicates:`, `scan:`, `ui:` prefixes

Can proceed immediately with systematic field refactoring.

