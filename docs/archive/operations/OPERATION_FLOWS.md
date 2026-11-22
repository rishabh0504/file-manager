# File Operations Flows - Visual Guide

## 1. MOVE OPERATION FLOWS

### 1.1 Queue Submit Flow (Most Common Move Path)

```
┌─────────────────────────────────────────────────────────────────┐
│                      USER ACTIONS                               │
└─────────────────────────────────────────────────────────────────┘

   User Creates Queues (1+ destination folders)
        ↓
   User Browses Explorer, Adds Files to Queues
   • explorer_add_to_queue(queue_idx, file_path)
   • File path appended to explorer_queues[idx].items
   • NO file movement yet
        ↓
   User Clicks "Submit All Queues"
        ↓

┌─────────────────────────────────────────────────────────────────┐
│          FRONTEND CONFIRMATION DIALOG                            │
│          Shows: "Move N files across M queues?"                  │
│          User clicks: Yes / No                                   │
└─────────────────────────────────────────────────────────────────┘

   If No → abort, nothing moves
   If Yes ↓

┌─────────────────────────────────────────────────────────────────┐
│          BACKEND: submit_all_queues()                            │
│          Location: src/app.rs:1402                               │
└─────────────────────────────────────────────────────────────────┘

   Sets explorer_queue_submitting = true
   Snapshots all queues: Vec<(target_dir, Vec<file_paths>)>
   Spawns background thread
        ↓

┌─────────────────────────────────────────────────────────────────┐
│          BACKGROUND THREAD                                       │
│          For each (target_dir, file_paths):                      │
│            For each file:                                        │
│              • update_progress(current_idx, total, filename)     │
│              • if is_dir: fast_move_dir(src, unique_dir_dest())  │
│              • if is_file: fast_move(src, unique_file_dest())    │
│              • increment success/fail count                      │
│            end                                                   │
│          end                                                     │
│          send (success_count, fail_count) via channel            │
└─────────────────────────────────────────────────────────────────┘

   ↓ Back to Main Thread (next frame)

┌─────────────────────────────────────────────────────────────────┐
│          MAIN LOOP: poll_queue_submit_result()                   │
│          Location: src/app.rs:1486                               │
└─────────────────────────────────────────────────────────────────┘

   Checks if thread sent result
   If yes:
     1. Set explorer_queue_submitting = false
     2. Clear moved items from queues: q.items.clear()
     3. Update status: "Moved N files across M queues"
     4. Call explorer_files_prune_missing()
     5. Call explorer_reload()
     6. Show completion message
   If no:
     • Wait for next frame (keep polling)

┌─────────────────────────────────────────────────────────────────┐
│          UI STATE AFTER                                          │
│          • Files moved to target folders                         │
│          • Queues emptied                                        │
│          • Explorer refreshed, shows new location                │
│          • Thumbnails cleared                                    │
│          • Modals updated                                        │
└─────────────────────────────────────────────────────────────────┘
```

---

### 1.2 Movable Queue Single Item Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                      USER ACTIONS                               │
└─────────────────────────────────────────────────────────────────┘

   User Clicks File → "Add to Movable Queue"
   • toggle_movable(path)
   • Appends to movable_items: MovableItem { path, target_queue: None }
   • Status: "Added to movable queue"
        ↓
   User Sees Movable Queue Modal (Sidebar Panel)
   • Shows files staged for move
   • User clicks on file → picks destination queue from dropdown
   • target_queue set to selected queue name
        ↓
   User Clicks "Move" (per row)
   • OR User Clicks "Move All" (bulk)

┌─────────────────────────────────────────────────────────────────┐
│          IMMEDIATE: move_single_movable_item(idx)                │
│          Location: src/app.rs:1300                               │
│          (Runs on main thread, blocks briefly)                   │
└─────────────────────────────────────────────────────────────────┘

   Checks if idx in bounds
   Extracts: path, target_queue_name
   Finds queue in explorer_queues by name
   
   If queue not found:
     • Status: "Queue 'X' not found"
     • Return false (item stays in movable queue)
   
   If target queue found:
     • Create target dir if missing
     • if path is dir: fast_move_dir(src, unique_dir_dest(target, name))
     • if path is file: fast_move(src, unique_file_dest(target, path))
     
     If move succeeds:
       1. Remove from movable_items
       2. Clear from explorer listing: explorer_entries.retain(...)
       3. Clear from folder modal if open: folder_modal_entries.retain(...)
       4. Clear thumbnail: thumbnail_cache.remove(&path)
       5. Clear thumbnail loading: thumbnail_loading.remove(&path)
       6. Close preview if viewing: close_explorer_preview()
       7. Status: "Moved 'file' → 'queue'"
       8. Return true
     
     If move fails:
       • Status: "Failed to move 'file'"
       • Item remains in movable_items
       • Return false

┌─────────────────────────────────────────────────────────────────┐
│          move_all_movable_items()                                │
│          Location: src/app.rs:1363                               │
└─────────────────────────────────────────────────────────────────┘

   Iterates movable_items (careful: list changes during iteration)
   For each item:
     If target_queue is None → skip (increment counter)
     Else → call move_single_movable_item(idx)
       If success → don't increment idx (remove shifts list)
       If fail → increment idx
   Return (success_count, fail_count)
   Call explorer_reload() once
```

---

## 2. DELETE/TRASH OPERATION FLOW

### 2.1 Bulk Delete via Results Panel

```
┌─────────────────────────────────────────────────────────────────┐
│                      USER WORKFLOW                              │
└─────────────────────────────────────────────────────────────────┘

   After dedup scan completes:
   User sees duplicate groups in results panel
   
   For each group:
     • One file is marked "KEEP" (auto-selected by scoring)
     • Other files are marked as duplicates
        ↓
   User manually checks boxes to select files for deletion:
   • Per-file checkboxes: unchecking "keep" file
   • Or: select entire group
        ↓
   User Clicks "Trash Selected" or "Trash Group"
        ↓

┌─────────────────────────────────────────────────────────────────┐
│          FRONTEND: Create PendingAction                          │
│          Location: src/ui/results_panel.rs                       │
└─────────────────────────────────────────────────────────────────┘

   app.pending_action = Some(PendingAction::Trash {
       title: "Trash X Duplicates",
       action: "manual_delete",
       plan: ActionPlan::build(...)
   })
        ↓

┌─────────────────────────────────────────────────────────────────┐
│          FRONTEND CONFIRM DIALOG                                 │
│          Location: src/ui/confirm_dialog.rs                      │
│          Shows: "Move X files to Trash?"                         │
└─────────────────────────────────────────────────────────────────┘

   If user cancels → app.pending_action = None, abort
   If user confirms ↓

┌─────────────────────────────────────────────────────────────────┐
│          BACKEND: Execute ActionPlan                             │
│          Plan contains all groups to process                     │
└─────────────────────────────────────────────────────────────────┘

   For each group in plan:
     Call apply_delete_group(group, ActionMode::Delete)
        ↓

┌─────────────────────────────────────────────────────────────────┐
│          apply_delete_group()                                    │
│          Location: src/resolver.rs:264                           │
└─────────────────────────────────────────────────────────────────┘

   Filter: get all files in group EXCEPT keep_index
   Call trash_batch(&paths) → moves all to Trash
   Return Vec<FileResult<()>> (per-file status)
        ↓

┌─────────────────────────────────────────────────────────────────┐
│          UI AFTER                                                │
│          • Group updated: duplicates marked as trashed           │
│          • Keep file marked as "kept"                            │
│          • Action logged in action log                           │
│          • Status: "Moved N files to Trash"                      │
└─────────────────────────────────────────────────────────────────┘
```

---

### 2.2 Global Apply (Delete or HardLink)

```
┌─────────────────────────────────────────────────────────────────┐
│                      USER ACTIONS                               │
└─────────────────────────────────────────────────────────────────┘

   After scan, user selects mode:
   • Action Mode Dropdown: "Delete" or "HardLink"
        ↓
   User Clicks "Apply to All"
        ↓

┌─────────────────────────────────────────────────────────────────┐
│          FRONTEND: Create Global ActionPlan                      │
└─────────────────────────────────────────────────────────────────┘

   app.pending_action = Some(PendingAction::Apply {
       mode: ActionMode::Delete,  // or HardLink
       plan: ActionPlan::build(all_groups, ...)
   })
        ↓

┌─────────────────────────────────────────────────────────────────┐
│          CONFIRM DIALOG                                          │
│          Shows: "Apply Delete to ALL groups?"                    │
│          Summary: total files, total reclaimable bytes           │
└─────────────────────────────────────────────────────────────────┘

   User confirms ↓

┌─────────────────────────────────────────────────────────────────┐
│          BACKEND: Execute Per-Group                              │
│          For every group in ActionPlan:                          │
│            apply_delete_group(group, mode)                       │
└─────────────────────────────────────────────────────────────────┘

   Each group:
     All duplicates → Trash (or hardlink if mode==HardLink)
     Keep file → preserved
        ↓

┌─────────────────────────────────────────────────────────────────┐
│          UI AFTER                                                │
│          • All results updated                                   │
│          • Action logged (bulk delete)                           │
│          • Status: "Applied Delete to N groups, trashed M files" │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. REMOVE OPERATION FLOWS

### 3.1 Remove from Queue (Staging Only)

```
   User Views Queue Detail Modal
   Shows files staged in queue[idx]
        ↓
   User Clicks "Remove" on file row
        ↓
   explorer_remove_from_queue(idx, path)
        ↓
   q.items.retain(|p| p != &path)
   (Removes path from staging list)
        ↓
   Status: "Removed 'file' from 'queue'"
   
   NOTE: Actual file untouched! Only queue staging cleared.
```

---

### 3.2 Delete Entire Queue

```
   User Views Queue List
        ↓
   User Clicks "Delete Queue" on queue header
        ↓
   explorer_delete_queue(idx)
        ↓
   explorer_queues.remove(idx)
   (Removes entire queue; all staged files remain on disk)
        ↓
   Adjust active_queue index if needed
   Status: "Deleted queue 'name'"
   
   NOTE: No files deleted from disk!
```

---

## 4. INTERNAL MOVE STRATEGY (fast_move)

```
┌─────────────────────────────────────────────────────────────────┐
│          fast_move(src, dst)                                     │
│          Location: src/app.rs:2739                               │
└─────────────────────────────────────────────────────────────────┘

   SAFETY CHECK:
   if dst.exists() || dst.symlink_metadata().is_ok() {
       return Err("Destination already exists")
   }
   (Never overwrites!)
        ↓
   ATTEMPT 1: Atomic Rename (fastest, same filesystem)
   match fs::rename(src, dst) {
       Ok(()) → return Ok(())  ✓ Success!
       Err(_) → continue to attempt 2
   }
        ↓
   ATTEMPT 2: macOS APFS Clone (copy-on-write, near-instant)
   #[cfg(target_os = "macos")] {
       if cp -c src dst succeeds {
           fs::remove_file(src)  // Delete source after clone
           return Ok(())  ✓ Success!
       }
   }
        ↓
   ATTEMPT 3: Generic Stream Copy + Delete (cross-platform, cross-volume)
   Open src for reading (4 MB buffered)
   Open dst for writing (4 MB buffered)
   Loop:
       Read chunk from src → Write to dst
   Close files
   fs::remove_file(src)
   return Ok(())  ✓ Success!
        ↓
   If stream copy fails at any point: return Err (atomic failure)
```

---

## 5. COLLISION HANDLING

```
┌─────────────────────────────────────────────────────────────────┐
│          Scenario: Moving "photo.jpg" to /dest/                  │
│          where /dest/photo.jpg already exists                    │
└─────────────────────────────────────────────────────────────────┘

   unique_file_dest(/dest, /src/photo.jpg)
        ↓
   Stem = "photo"
   Ext = "jpg"
   Original = "photo.jpg"
        ↓
   Try #1: /dest/photo.jpg
   if exists or symlink_metadata().is_ok() → continue
        ↓
   Try #2: /dest/photo_1.jpg
   if exists or symlink_metadata().is_ok() → continue
        ↓
   Try #3: /dest/photo_2.jpg
   if free! → return /dest/photo_2.jpg
        ↓
   fast_move(/src/photo.jpg, /dest/photo_2.jpg)
   
   Result: Original stays, duplicate renamed with counter.
```

---

## 6. STATE DIAGRAM: Movable Queue Item Lifecycle

```
┌─────────────────────────────────┐
│   Item Created                  │
│   • User clicks "Add to Movable" │
│   • MovableItem {               │
│       path,                      │
│       target_queue: None         │
│     }                            │
└──────────────┬──────────────────┘
               │
               ↓
┌─────────────────────────────────┐
│   Item Staged                   │
│   • Item in movable_items       │
│   • Shown in sidebar panel      │
│   • User can remove (keep file) │
└──────────────┬──────────────────┘
               │ User picks queue
               ↓
┌─────────────────────────────────┐
│   Queue Assigned                │
│   • target_queue = "Documents"  │
│   • Ready to move               │
└──────────────┬──────────────────┘
               │
        ┌──────┴─────────┐
        │                │
        ↓                ↓
  Move Succeeds     Move Fails
  │                 │
  │                 └→ Item stays in queue
  │                    User can retry
  │
  ↓
┌──────────────────────────┐
│   Item Moved             │
│   • File moved to target  │
│   • Item removed from     │
│     movable_items        │
│   • Thumbnails cleared   │
│   • Explorer updated      │
└──────────────────────────┘
```

---

## 7. QUICK OPERATION MATRIX

```
┌──────────────────┬──────────────────┬──────────────────┬──────────────────┐
│   OPERATION      │   TRIGGERS       │   CONFIRMATION  │   ACTUAL FILE    │
├──────────────────┼──────────────────┼──────────────────┼──────────────────┤
│ Add to Queue     │ Context menu     │ None             │ No change        │
│ Remove from Q.   │ Queue detail     │ None             │ No change        │
│ Delete Queue     │ Queue header     │ None             │ No change        │
│                  │                  │                  │                  │
│ Move Single      │ Movable row btn  │ Implicit (target │ MOVED to target  │
│ Movable          │                  │ must be set)     │                  │
│ Move All         │ Movable modal    │ Implicit         │ All MOVED        │
│ Movable          │ "Move All" btn   │                  │                  │
│                  │                  │                  │                  │
│ Submit All       │ Queue modal      │ Dialog: file cnt │ All MOVED to     │
│ Queues           │ "Submit" btn     │                  │ targets          │
│                  │                  │                  │                  │
│ Trash Selected   │ Results panel    │ Dialog: file cnt │ MOVED to Trash   │
│ (duplicates)     │ "Trash" btn      │                  │                  │
│                  │                  │                  │                  │
│ Apply Delete     │ Results panel    │ Dialog: file cnt │ MOVED to Trash   │
│ (all groups)     │ "Apply" btn      │ + bytes          │                  │
│                  │                  │                  │                  │
│ Move Archive     │ Archive modal    │ Dialog: file cnt │ MOVED to target  │
│ Files            │ "Move" btn       │                  │ folder           │
└──────────────────┴──────────────────┴──────────────────┴──────────────────┘
```

