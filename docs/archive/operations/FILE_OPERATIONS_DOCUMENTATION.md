# File Operations Business Logic Documentation

## Overview
Sortwise handles four core file operations: **Copy**, **Move**, **Delete**, and **Remove** (from staging queues). These operations work across multiple workflows and are designed to be safe, non-destructive by default, and reversible where possible.

---

## 1. MOVE OPERATIONS

### Business Logic
Move is the primary operation used throughout the app. It atomically relocates files/folders from source to destination, never overwriting existing files.

#### Implementation: `fast_move()` - Single File Move
**Location**: `src/app.rs:2739`

```rust
fn fast_move(src: &Path, dst: &Path) -> io::Result<()>
```

**Strategy** (in priority order):
1. **Safety Guard**: Checks if destination exists. Never overwrites. Fails if `dst.exists() || dst.symlink_metadata().is_ok()`
2. **Atomic Rename**: Attempts `fs::rename()` (same filesystem, instant)
3. **macOS APFS Clone**: Uses `cp -c` for copy-on-write (near-instant, macOS only)
4. **Generic Fallback**: Stream copy (4 MB buffers) + source deletion (cross-volume support)

**Guarantees**:
- No overwrite of existing files/symlinks
- Atomic or all-or-nothing behavior
- Portable across platforms

#### Implementation: `fast_move_dir()` - Directory Move
**Location**: `src/app.rs:2786`

Identical safety guarantees as `fast_move()`, but for directories:
1. Atomic rename attempt
2. macOS `cp -R` with cleanup on failure
3. Recursive copy + delete fallback

---

### 1.1 Queue Submit Operations
**Use Case**: User stages files into named queues, then submits all to target folders.

#### Operation: `submit_all_queues()`
**Called From**: UI action (e.g., "Submit All Queues" button)
**Location**: `src/app.rs:1402`

**Flow**:
1. User creates named queues (each bound to a target folder)
2. User adds files/folders to queues via explorer
3. User clicks "Submit" → shows confirmation dialog with file count & queue count
4. Backend spawns background thread to move files
5. Progress updates to UI: `explorer_queue_submit_progress`
6. On completion: clears queues, refreshes explorer, shows status

**Caller Chain**:
- UI → `submit_all_queues()` → spawns thread → `fast_move()` / `fast_move_dir()` per file
- Uses `unique_file_dest()` / `unique_dir_dest()` to rename on collision
- Thread sends `(success_count, fail_count)` via channel

**Before/After**:
- Before: Files in queue staging lists (`explorer_queues[i].items`)
- After: Files moved to target folders; queues cleared; explorer reloaded

---

### 1.2 Movable Queue Submit
**Use Case**: Individual triage workflow. User picks files one-by-one, assigns target queue, moves singly or bulk.

#### Operation: `move_single_movable_item(idx)`
**Called From**: UI action on movable queue row
**Location**: `src/app.rs:1300`

**Flow**:
1. User selects file → adds to `movable_items` with optional target queue
2. User picks destination queue from dropdown
3. User clicks move → calls `move_single_movable_item()`
4. Checks if target queue exists; creates target dir if missing
5. Calls `fast_move()` or `fast_move_dir()` with collision handling
6. On success: removes from movable queue, forgets path from explorer, updates status
7. On failure: shows error; item remains in queue

**Cleanup on Move Success**:
```rust
self.explorer_entries.retain(|e| e.path != p);  // Clear listing
self.folder_modal_entries.retain(|e| e.path != p);  // Clear modal if open
self.thumbnail_cache.remove(&p);  // Clear cached thumbnails
if self.explorer_preview_path == Some(p) { 
    self.close_explorer_preview();  // Close preview if viewing moved file
}
```

**Before/After**:
- Before: `MovableItem { path, target_queue: Some(queue_name) }`
- After: File moved; item removed from `movable_items`; UI state cleaned

---

#### Operation: `move_all_movable_items()`
**Called From**: "Move All" button on movable queue modal
**Location**: `src/app.rs:1363`

**Flow**:
1. Iterates `movable_items`
2. Skips items without target queue assigned
3. Calls `move_single_movable_item(idx)` for each
4. Tracks success/fail counts
5. Calls `explorer_reload()` once after all moves
6. Returns `(success_count, fail_count)`

---

### 1.3 Archive Files Move
**Use Case**: User finds archive files, extracts them, then moves originals to trash/archive folder.

#### Operation: `execute_move_archive_files()`
**Called From**: UI action after archive selection
**Location**: `src/app.rs:1033`

**Flow**:
1. User selects archive files from scan results
2. User picks destination folder
3. User clicks move → shows confirmation (file count + target path)
4. If confirmed: spawns background thread
5. For each selected archive:
   - Updates progress: (index, total, filename)
   - Calls `fast_move()` with collision handling
   - Increments success/fail count
6. Thread sends `(success_count, fail_count)` via channel
7. `poll_archive_files_move_result()` shows completion dialog

**Before/After**:
- Before: Archive files in `archive_files_selected` set
- After: Files moved to target; set cleared; view reset to Scan

---

### 1.4 Queue-to-Queue Transfer (via Explorer)
**Use Case**: User adds file to named queue while browsing explorer.

#### Operation: `explorer_add_to_queue(idx, path)`
**Called From**: "Add to Queue" button in explorer context menu
**Location**: `src/app.rs:1140`

**Flow**:
1. User right-clicks file/folder in explorer
2. User selects queue → calls `explorer_add_to_queue(queue_idx, file_path)`
3. Dedupes: checks if path already in queue; shows status if duplicate
4. Otherwise: appends to `explorer_queues[idx].items`
5. Shows status: "Added 'file' to 'queue_name'"

**Note**: This is **staging only** — no file movement yet. Files stay in source location until queue is submitted.

---

### 1.5 Movable Queue Toggle (Add/Remove)
**Use Case**: Quick triage without named queues.

#### Operation: `toggle_movable(path)`
**Called From**: "Move to Movable Queue" button in explorer
**Location**: `src/app.rs:1273`

**Flow**:
1. If path already in movable queue: remove it
2. Otherwise: add with default queue (or None if not set)
3. Shows status: "Added/Removed from movable queue"

**Caller Chain**:
- UI → `toggle_movable()` → modifies `movable_items`

---

## 2. DELETE OPERATIONS

### Business Logic
Deletion moves files to Trash (recoverable), never permanent deletion by default. Hardlink mode is alternative for deduplication.

#### Implementation: `apply_delete_group()`
**Location**: `src/resolver.rs:264`

```rust
pub fn apply_delete_group(group: &DuplicateGroup, mode: ActionMode) 
  -> Vec<FileResult<()>>
```

**Flow**:
1. Checks if mode is `ActionMode::Delete` (skips if other mode)
2. Collects all files EXCEPT the one at `group.keep_index`
3. Calls `trash_batch(&paths)` to move all duplicates to Trash
4. Returns result per file

**Key Feature**: Only duplicates are removed; the "keep" file (determined by scoring logic) is preserved.

---

### 2.1 Bulk Delete via Results Panel
**Use Case**: After dedup scan, user marks duplicate copies for deletion and applies action.

#### Operation: `Trash { title, action, plan }`
**PendingAction Type**
**Location**: `src/app.rs:224` (enum), called from results panel

**Flow**:
1. User checks files in duplicate group
2. User clicks "Trash Selected" or "Trash Group"
3. UI creates `PendingAction::Trash { title, action, plan }`
4. Confirm dialog shows: "Move X files to Trash?"
5. User confirms → executes plan via `apply_delete_group()`
6. `action` parameter is logged for audit trail

**Caller Chain**:
- UI (results_panel.rs) → `app.pending_action = Some(PendingAction::Trash{...})`
- Confirm dialog → user clicks Yes
- Main loop executes action → logs to `action_log`

**Before/After**:
- Before: Files marked for deletion in `DuplicateGroup.files`
- After: Files in Trash; `DuplicateGroup` updated; keep file marked as "kept"

---

### 2.2 Apply Action (Global)
**Use Case**: User chooses global action (Delete or HardLink) across all groups.

#### Operation: `Apply { mode: ActionMode, plan }`
**PendingAction Type**
**Location**: `src/app.rs:230`

**Flow**:
1. User selects "Delete" or "HardLink" mode in settings
2. User clicks "Apply to All" in results panel
3. UI creates `PendingAction::Apply { mode, plan }`
4. Confirm dialog shows total files & reclaimable bytes
5. User confirms → plan executes for every group
6. If `mode == Delete`: calls `apply_delete_group()` per group
7. If `mode == HardLink`: calls link logic (not covered here)

**Plan Execution**: `sortwise::action_plan::ActionPlan` orchestrates the batch.

---

## 3. REMOVE OPERATIONS

### Business Logic
Remove deletes a file from a staging queue (doesn't delete the actual file), or removes a file from movable queue.

#### Operation: `explorer_remove_from_queue(idx, path)`
**Called From**: "Remove from Queue" button on queue detail modal
**Location**: `src/app.rs:1186`

**Flow**:
1. User clicks "Remove" on a file in queue detail view
2. Calls `explorer_remove_from_queue(queue_idx, file_path)`
3. Dequeues: `q.items.retain(|p| p != &path)`
4. **Does NOT touch the actual file** — only removes staging reference

**Before/After**:
- Before: File path in `explorer_queues[idx].items`
- After: Path removed from queue; file untouched on disk

---

#### Operation: `explorer_delete_queue(idx)`
**Called From**: "Delete Queue" button on queue list
**Location**: `src/app.rs:1193`

**Flow**:
1. User clicks "Delete Queue" on queue header
2. Removes the entire queue: `explorer_queues.remove(idx)`
3. Adjusts active queue index if deleted queue was active
4. Clears dropdown selection
5. Shows status: "Deleted queue 'name'"
6. **Does NOT delete any files** — only removes the queue staging list

---

#### Operation: `remove_from_movable(idx)`
**Called From**: "Remove" button on movable queue row
**Location**: `src/app.rs:1287`

**Flow**:
1. User clicks "Remove" on movable queue row
2. Removes item at index: `movable_items.remove(idx)`
3. **Does NOT delete the actual file**

---

## 4. COPY OPERATIONS

### Business Logic
Explicit copy is rare in the UI but available for internal operations (recursive directory copying during cross-volume moves).

#### Implementation: `copy_dir_recursive()`
**Location**: `src/app.rs:2819`

```rust
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()>
```

**Flow**:
1. Creates destination directory
2. Walks source tree with `walkdir`
3. For each directory: creates it at destination
4. For each file: copies via `fs::copy()`
5. Preserves directory structure

**Called By**: `fast_move_dir()` as fallback for cross-volume moves

---

### 4.1 Collision Handling
Both move operations use collision-aware destination functions to avoid overwrites.

#### `unique_file_dest(target_dir, file_path)`
**Location**: `src/app.rs:2857`

**Strategy**:
1. First try: `Name.ext`
2. If exists: `Name_1.ext`, `Name_2.ext`, …
3. Increments counter until finding available name

**Example**:
```
Input: target=/dest, file=/src/photo.jpg
Output: /dest/photo.jpg         (if free)
        /dest/photo_1.jpg       (if photo.jpg exists)
        /dest/photo_2.jpg       (if photo_1.jpg exists)
```

---

#### `unique_dir_dest(target_dir, folder_name)`
**Location**: `src/app.rs:2845`

**Strategy**:
1. First try: `Name`
2. If exists: `Name_2`, `Name_3`, …
3. Never uses `_1` suffix (starts at `_2`)

**Example**:
```
Input: target=/dest, folder=Documents
Output: /dest/Documents         (if free)
        /dest/Documents_2       (if Documents exists)
        /dest/Documents_3       (if Documents_2 exists)
```

---

## 5. FRONTEND INTEGRATION POINTS

### Explorer Panel Actions
**Location**: `src/ui/explorer_panel.rs:3087`

```rust
match action {
    Act::RemoveFromQueue(i, p) => app.explorer_remove_from_queue(i, p),
    Act::DeleteQueue(i) => app.explorer_delete_queue(i),
    Act::MoveSingleMovableItem(idx) => app.move_single_movable_item(idx),
    // ...
}
```

### Results Panel Actions
**Location**: `src/ui/results_panel.rs`

Initiates:
- `PendingAction::Trash { ... }` for manual duplicate selection
- `PendingAction::Apply { ... }` for global actions

### Confirm Dialog
**Location**: `src/ui/confirm_dialog.rs`

Shows before executing:
- Trash operations (shows file count & target)
- Apply operations (shows reclaimable bytes & action type)

Receives user confirmation → executes action via backend

---

## 6. STATE MANAGEMENT & CLEANUP

### After Move Success
1. Remove from source listing: `explorer_entries.retain(|e| e.path != p)`
2. Remove from modal if open: `folder_modal_entries.retain(...)`
3. Clear thumbnail cache: `thumbnail_cache.remove(&p)`
4. Clear thumbnail loading set: `thumbnail_loading.remove(&p)`
5. Close preview if viewing: `close_explorer_preview()`
6. Remove from movable queue: `movable_items.remove(idx)`
7. Reload explorer: `explorer_reload()`

### After Queue Submit
1. Clear submitted queue: `q.items.clear()`
2. Reload explorer: `explorer_reload()`
3. Prune missing files: `explorer_files_prune_missing()`

### Progress Tracking
- **Movable queue**: `Arc<Mutex<(usize, usize, String)>>` = `(files_done, total, current_filename)`
- **Queue submit**: `explorer_queue_submit_progress`
- **Archive move**: `archive_files_move_progress`

---

## 7. ERROR HANDLING

### Move Failures
- File locked, no permissions, source doesn't exist → move fails; item remains in queue
- UI shows error status: "Failed to move 'filename'"

### Delete/Trash Failures
- File locked → trash operation fails; logged in error log
- Partial successes tracked: `(success_count, fail_count)`

### Cross-Volume Fallback
- Rename fails → attempts APFS clone (macOS) → falls back to stream copy
- Ensures operation always completes or fails cleanly

---

## 8. RECOMMENDED UI IMPROVEMENTS

### For Better UX:
1. **Undo**: Store trash operations in log; offer undo for recent moves/deletes
2. **Partial Failure Handling**: Show which files failed with reasons
3. **Collision Dialog**: Warn user before auto-renaming (ask: skip/rename/overwrite?)
4. **Move Preview**: Show destination before moving (especially for queue submit)
5. **Drag-to-Queue**: Allow drag from explorer directly to queue cards
6. **Batch Rename**: Rename on collision instead of incrementing `_1`, `_2`, …

### For Performance:
1. **Cache Queue Targets**: Pre-resolve target dirs instead of fs::metadata on submit
2. **Parallel Moves**: Split large submits across thread pool instead of sequential
3. **Streaming UI**: Show file count incrementally, not bulk at end

---

## 9. QUICK REFERENCE

| Operation | Function | Location | Caller | Safety |
|-----------|----------|----------|--------|--------|
| Move Single File | `fast_move()` | app.rs:2739 | Queue submit, Movable item move | Never overwrites |
| Move Directory | `fast_move_dir()` | app.rs:2786 | Queue submit, Movable dir move | Never overwrites |
| Move All Queues | `submit_all_queues()` | app.rs:1402 | UI button | Confirmed dialog |
| Move Single Movable | `move_single_movable_item()` | app.rs:1300 | Movable row button | Confirmed per item |
| Move Archive Files | `execute_move_archive_files()` | app.rs:1033 | Archive scan modal | Confirmed dialog |
| Delete Duplicates | `apply_delete_group()` | resolver.rs:264 | Action plan | To Trash only |
| Remove from Queue | `explorer_remove_from_queue()` | app.rs:1186 | Queue detail modal | No file touched |
| Delete Queue | `explorer_delete_queue()` | app.rs:1193 | Queue list UI | No files touched |
| Remove Movable | `remove_from_movable()` | app.rs:1287 | Movable row button | No file touched |
| Handle Collision | `unique_file_dest()` | app.rs:2857 | Move operations | Increments name |
| Handle Collision Dir | `unique_dir_dest()` | app.rs:2845 | Move operations | Increments name |

