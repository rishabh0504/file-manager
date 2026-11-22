# File Operations Documentation - Complete Guide

This documentation set covers the **business logic, frontend integration, and operations flow** for file move, copy, delete, and remove operations in Sortwise.

## 📚 Documents Included

### 1. **FILE_OPERATIONS_DOCUMENTATION.md**
   - **What**: Core business logic for all file operations
   - **Who**: Backend engineers, architects
   - **Coverage**:
     - Move operations (fast_move, fast_move_dir)
     - Delete/Trash operations (apply_delete_group)
     - Remove operations (from queues/staging)
     - Copy operations (copy_dir_recursive)
     - Collision handling (unique_file_dest, unique_dir_dest)
     - State management & cleanup
     - Error handling
     - Quick reference table

   **Read this if**: You're modifying core move/delete logic or need to understand why operations work this way.

---

### 2. **OPERATION_FLOWS.md**
   - **What**: Visual flowcharts for how operations execute
   - **Who**: Frontend developers, QA, architects
   - **Coverage**:
     - Queue submit flow (most common move path)
     - Movable queue single item flow
     - Bulk delete via results panel
     - Global apply flow
     - Remove from queue flow
     - Internal move strategy (attempt order: rename → clone → copy)
     - Collision handling example
     - State lifecycle diagrams
     - Quick operation matrix

   **Read this if**: You need to understand the step-by-step flow of an operation, or you're debugging why something isn't working.

---

### 3. **FRONTEND_INTEGRATION_GUIDE.md**
   - **What**: UI patterns, components, and integration points
   - **Who**: Frontend developers, UI/UX designers
   - **Coverage**:
     - Frontend architecture (Act enum pattern)
     - Current UI components (Queue modal, Movable panel, Results panel)
     - State flow from UI to backend
     - How to add new operations
     - Key data structures
     - Progress tracking
     - Error handling patterns
     - 7 recommended UI improvements
     - Testing patterns
     - Suggested keyboard shortcuts

   **Read this if**: You're building UI for file operations or integrating a new operation into the frontend.

---

## 🎯 Quick Navigation

### By Task

**"I need to fix a bug in the move operation"**
1. Read: OPERATION_FLOWS.md → Operation you're fixing
2. Read: FILE_OPERATIONS_DOCUMENTATION.md → Business Logic section
3. Find: Source code at the locations listed

**"I want to add a new file operation (e.g., Copy)"**
1. Read: FRONTEND_INTEGRATION_GUIDE.md → "How to Add New Operations"
2. Read: FILE_OPERATIONS_DOCUMENTATION.md → Understand core functions
3. Implement backend function in `src/app.rs`
4. Add Act enum variant in `src/ui/explorer_panel.rs`
5. Add UI button that dispatches the action

**"I need to understand why a move failed"**
1. Read: OPERATION_FLOWS.md → Internal Move Strategy
2. Check: Error handling in FILE_OPERATIONS_DOCUMENTATION.md
3. Look at: Status bar or error log in UI
4. Trace: Execution path in source code

**"I'm implementing a UI feature for queues"**
1. Read: FRONTEND_INTEGRATION_GUIDE.md → Queue Representation section
2. Read: OPERATION_FLOWS.md → Queue Submit Flow
3. Look at: Current UI components in results_panel.rs / explorer_panel.rs
4. Use: Recommended improvements as inspiration

**"I need to understand the full lifecycle of a file operation"**
1. Read: OPERATION_FLOWS.md → Full flow diagram (top to bottom)
2. Read: FRONTEND_INTEGRATION_GUIDE.md → State Flow section
3. Understand: Progress tracking and error handling patterns

---

## 🔑 Key Concepts

### Move Operation (Primary)
- **Strategy**: Try atomic rename → try APFS clone → stream copy + delete
- **Safety**: Never overwrites; fails cleanly if destination exists
- **Called from**: Queue submit, movable queue, archive files
- **Performance**: Atomic rename is instant; fallback works across volumes

### Delete Operation
- **What**: Moves duplicates to Trash (recoverable, not permanent)
- **Who gets kept**: One file per group, chosen by scoring algorithm
- **Called from**: Results panel (manual selection), Apply action (global)
- **Safety**: Logged to action_log; trashed files in system Trash

### Remove Operation
- **What**: Removes file from staging queue/list
- **Important**: Does NOT delete the actual file
- **Called from**: Queue detail modal, movable queue
- **State**: Only modifies in-memory lists; no disk changes

### Collision Handling
- **Strategy**: Append counter to filename if destination exists
- **Example**: `photo.jpg` → `photo_1.jpg` → `photo_2.jpg`
- **For directories**: `Folder` → `Folder_2` → `Folder_3`

---

## 🏗️ Architecture Overview

```
┌────────────────────────────────────────┐
│          UI Layer                      │
│  (explorer_panel.rs,                   │
│   results_panel.rs,                    │
│   confirm_dialog.rs)                   │
└────────────┬─────────────────────────┘
             │ Act enum dispatch
             ↓
┌────────────────────────────────────────┐
│          App Layer                     │
│  (SortwiseApp, app.rs)                 │
│  • submit_all_queues()                 │
│  • move_single_movable_item()          │
│  • execute_move_archive_files()        │
│  • pending_action management           │
└────────────┬─────────────────────────┘
             │ Backend functions
             ↓
┌────────────────────────────────────────┐
│          Operations Layer              │
│  (resolver.rs, app.rs)                 │
│  • fast_move()                         │
│  • fast_move_dir()                     │
│  • apply_delete_group()                │
│  • unique_file_dest()                  │
└────────────┬─────────────────────────┘
             │ Filesystem I/O
             ↓
┌────────────────────────────────────────┐
│          Filesystem                    │
│  (fs::rename, fs::copy, trash, etc.)   │
└────────────────────────────────────────┘
```

---

## 📊 Operation Matrix

| Operation | Backend Function | UI Entry | Confirmation | Threading | Called Locations |
|-----------|------------------|----------|--------------|-----------|------------------|
| **Move Single File** | `fast_move()` | Direct | Implicit | Background thread | Queue submit, Movable item move, Archive move |
| **Move Single Directory** | `fast_move_dir()` | Direct | Implicit | Background thread | Queue submit, Movable dir move |
| **Submit All Queues** | `submit_all_queues()` | UI button | Dialog (file count) | Background thread | Queue modal |
| **Move Movable Single** | `move_single_movable_item()` | Movable row button | Implicit (target must be set) | Main thread | Movable queue modal |
| **Move Movable All** | `move_all_movable_items()` | Movable modal button | Implicit | Main thread (per-item) | Movable queue modal |
| **Move Archives** | `execute_move_archive_files()` | Archive modal | Dialog (file count + path) | Background thread | Archive scan results |
| **Delete Duplicates** | `apply_delete_group()` | Results panel | Dialog (file count) | Sync (batch) | Action plan execution |
| **Apply Global Delete** | `apply_delete_group()` (per group) | Results panel | Dialog (file count + bytes) | Sync (batch) | Action plan execution |
| **Remove from Queue** | `explorer_remove_from_queue()` | Queue detail | None | Main thread | Queue detail modal |
| **Delete Queue** | `explorer_delete_queue()` | Queue header | None | Main thread | Queue list |
| **Remove Movable Item** | `remove_from_movable()` | Movable row button | None | Main thread | Movable queue modal |

---

## 🔄 State Machines

### File Operation Lifecycle
```
START
  ↓
User initiates (clicks button, drags, etc.)
  ↓
Collect affected files
  ↓
Show confirmation dialog (if required)
  ↓
User confirms/cancels
  ↓
If confirmed:
  ├→ Spawn background thread (if async)
  │   ↓
  │   Execute operation (move/delete/etc.)
  │   ↓
  │   Send result via channel
  │   ↓
  │   Poll result in main loop
  │   ↓
  │   Update UI state
  │   ↓
  │   Show completion message
  │
If cancelled or failed:
  └→ Revert staging state
      ↓
      Show error message
      ↓
END
```

---

## 🛠️ Common Debugging Scenarios

### "Move failed silently"
1. Check `app.explorer_status` for error message
2. Check `app.show_error_log` for detailed error
3. Verify destination is writable: `std::fs::metadata(&target_dir)`
4. Check if collision handling triggered: destination already exists?

### "File appears in explorer but isn't there"
1. Call `app.explorer_reload()` to refresh listing
2. Check if file was moved but explorer not updated
3. Verify cache wasn't cleared: `app.thumbnail_cache`, `app.explorer_entries`

### "Queue never submitted"
1. Check `app.explorer_queue_submitting` flag
2. Check if background thread crashed: look at `result_rx`
3. Verify queues have items: `app.explorer_queues[i].items.len()`
4. Check permissions on target directories

### "Delete didn't send files to Trash"
1. Verify `action_mode == ActionMode::Delete` (not DryRun or HardLink)
2. Check if `apply_delete_group()` was actually called
3. Verify files didn't have permission errors
4. Check system Trash location

---

## 📋 Checklists

### Adding a New File Operation
- [ ] Implement backend function in `src/app.rs` or `src/resolver.rs`
- [ ] Add `Act::NewOperation` variant to dispatcher
- [ ] Add match arm in `explorer_panel.rs:3087`
- [ ] Add UI button/trigger in relevant panel
- [ ] Test backend function in isolation
- [ ] Test UI integration (button → action → result)
- [ ] Handle errors and show user feedback
- [ ] Update state appropriately (clear caches, reload if needed)
- [ ] Document in this guide

### Code Review Checklist for Move Operations
- [ ] Destination collision checked (never overwrites)
- [ ] Atomic rename attempted first (performance)
- [ ] Cross-platform fallback implemented (stream copy)
- [ ] Errors handled gracefully (doesn't crash UI)
- [ ] Progress tracked (for background ops)
- [ ] UI state cleaned up after move (thumbnails, preview, listings)
- [ ] User gets feedback (status message or error)
- [ ] Logging added (action_log entry if applicable)

---

## 🚀 Performance Tips

### For Move Operations
- **Atomic rename** (if same filesystem): <1ms
- **APFS clone** (macOS): <1ms  
- **Stream copy** (cross-volume): ~100MB/s (depends on disk)

**Optimization**: For large batches, consider:
- Parallelizing moves across thread pool (currently sequential)
- Showing real-time progress per file
- Allowing cancel mid-operation

### For UI Responsiveness
- Always run moves in background thread (don't block UI)
- Poll results each frame (1-5ms per check)
- Batch UI updates (don't redraw every file)

---

## 🔐 Safety Guarantees

✅ **What's Safe**:
- Files moved to Trash are recoverable
- Never overwrites existing files
- Atomic rename on same filesystem (all-or-nothing)
- Symlinks detected and not followed

❌ **What's Not**:
- Hardlink mode (permanent dedup, not reversible)
- Files on slow/flaky networks (may time out)
- Very large directories (may hang during copy)

---

## 🔗 Related Code Locations

**Core Operations**:
- `src/app.rs:1033` - `execute_move_archive_files()`
- `src/app.rs:1186` - `explorer_remove_from_queue()`
- `src/app.rs:1193` - `explorer_delete_queue()`
- `src/app.rs:1287` - `remove_from_movable()`
- `src/app.rs:1300` - `move_single_movable_item()`
- `src/app.rs:1363` - `move_all_movable_items()`
- `src/app.rs:1402` - `submit_all_queues()`
- `src/app.rs:2739` - `fast_move()`
- `src/app.rs:2786` - `fast_move_dir()`
- `src/app.rs:2857` - `unique_file_dest()`

**Deletion**:
- `src/resolver.rs:264` - `apply_delete_group()`

**UI**:
- `src/ui/explorer_panel.rs:3087` - Action dispatch
- `src/ui/results_panel.rs` - Results panel actions
- `src/ui/confirm_dialog.rs` - Confirmation dialog

**Type Definitions**:
- `src/app.rs:85-166` - FileQueue, MovableItem, FilesStage, etc.
- `src/app.rs:220-234` - PendingAction enum

---

## 📞 Questions?

For specific implementation details, refer to:
- **Business logic questions**: FILE_OPERATIONS_DOCUMENTATION.md
- **Flow/sequence questions**: OPERATION_FLOWS.md
- **UI integration questions**: FRONTEND_INTEGRATION_GUIDE.md
- **Source code**: Locations listed above

---

**Last Updated**: 2026-11-22  
**Documentation Version**: 1.0  
**Applicable to**: sortwise file-manager project  

