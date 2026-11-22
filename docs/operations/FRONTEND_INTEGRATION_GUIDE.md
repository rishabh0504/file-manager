# Frontend Integration Guide - File Operations

## 1. FRONTEND ARCHITECTURE OVERVIEW

### File Operation Dispatch
**Location**: `src/ui/explorer_panel.rs:3087`

```rust
enum Act {
    RemoveFromQueue(usize, PathBuf),
    DeleteQueue(usize),
    MoveSingleMovableItem(usize),
    // ... other actions
}

match action {
    Act::RemoveFromQueue(i, p) => app.explorer_remove_from_queue(i, p),
    Act::DeleteQueue(i) => app.explorer_delete_queue(i),
    Act::MoveSingleMovableItem(idx) => {
        app.move_single_movable_item(idx);
    }
    // ...
}
```

This pattern decouples UI events from app logic. Each `Act` variant maps 1:1 to a backend function.

---

## 2. CURRENT UI COMPONENTS

### 2.1 Queue Detail Modal
**Location**: `src/ui/explorer_panel.rs`

**Features**:
- Shows `explorer_queues[active_queue].items`
- Lists all staged files
- Each row has: filename, size, "Remove" button

**Current Actions**:
- Click "Remove" → `Act::RemoveFromQueue(queue_idx, file_path)`
- Click "Delete Queue" (header) → `Act::DeleteQueue(queue_idx)`
- Click "Submit" → `submit_all_queues()` (direct call, no Act dispatch)

**Missing Features**:
- No drag-to-reorder
- No search/filter within queue
- No preview of target destination
- No progress indicator during submit

---

### 2.2 Movable Queue Sidebar Panel
**Location**: `src/ui/recursive_files_panel.rs` (or separate panel)

**Features**:
- Shows `movable_items` list
- Each row shows: filename, size, target queue dropdown
- Action buttons: "Move", "Remove", "Move All"

**Current Actions**:
- Select queue in dropdown → update `MovableItem.target_queue`
- Click "Move" → `Act::MoveSingleMovableItem(idx)`
- Click "Remove" → remove from `movable_items`
- Click "Move All" → `move_all_movable_items()`

**Current State Tracking**:
- `movable_last_chosen_queue`: remembers last selected queue (auto-fill)
- `movable_playing_path`: for inline video preview

---

### 2.3 Results Panel (Duplicates)
**Location**: `src/ui/results_panel.rs`

**Features**:
- Shows duplicate groups
- Each group has: keep file (marked with star/badge), duplicate files
- Per-file and per-group selection via checkboxes

**Current Actions**:
- Select files → updates `results_selected_files`
- Click "Trash Selected" → creates `PendingAction::Trash{...}`
- Click "Apply to All" → creates `PendingAction::Apply{...}`
- Confirm in dialog → executes action

**Action Logging**: Each deletion logged to `action_log` with timestamp, files, success count

---

### 2.4 Confirm Dialog
**Location**: `src/ui/confirm_dialog.rs`

**Current Implementation**:
```rust
match &app.pending_action {
    Some(PendingAction::Trash { title, plan, .. }) => {
        // Show: "Move X files to Trash?"
        // Buttons: Yes / No
        if yes_clicked {
            // Execute plan
            app.pending_action = None;
        }
    }
    Some(PendingAction::Apply { mode, plan }) => {
        // Show: "Apply Delete to N groups? (M total files, X GB reclaimable)"
        // Buttons: Yes / No
        if yes_clicked {
            // Execute plan
            app.pending_action = None;
        }
    }
    _ => { }
}
```

**Improvements Needed**:
- Show file list in dialog (scrollable, truncated to 10?)
- Show estimated time for large operations
- Allow preview of which files will be deleted
- Cancel button during execution (if operation running)

---

## 3. STATE FLOW: FROM UI TO BACKEND

### Example: Submit All Queues

```
┌─ UI Layer ──────────────────────────────────────────┐
│                                                      │
│  User clicks "Submit All Queues" button              │
│           ↓                                          │
│  explorer_panel.rs calls:                            │
│    if ui.button("Submit All").clicked() {            │
│        app.submit_all_queues()  ← DIRECT CALL        │
│    }                                                 │
│           ↓                                          │
└──────────────────────────────────────────────────────┘

┌─ App Layer ─────────────────────────────────────────┐
│                                                      │
│  submit_all_queues()  [app.rs:1402]                  │
│    1. Collect total file count                       │
│    2. Show confirmation dialog (rfd::MessageDialog)  │
│    3. If confirmed:                                  │
│       • Set explorer_queue_submitting = true         │
│       • Snapshot queues                              │
│       • Spawn background thread                      │
│       • Thread moves files via fast_move()           │
│       • Returns (success, fail) count via channel    │
│           ↓                                          │
│  Main loop → poll_queue_submit_result()              │
│    1. Check if thread finished                       │
│    2. If yes: clear queues, reload explorer          │
│    3. Update status message                          │
│           ↓                                          │
└──────────────────────────────────────────────────────┘

┌─ Next Frame ────────────────────────────────────────┐
│                                                      │
│  UI updates:                                         │
│  • Queue modal shows empty items                     │
│  • Explorer refreshed with new files                 │
│  • Status bar shows result message                   │
│                                                      │
└──────────────────────────────────────────────────────┘
```

---

## 4. HOW TO ADD NEW OPERATIONS

### Pattern: Add "Copy to Queue" Feature

**Step 1: Add Backend Function** (`src/app.rs`)
```rust
pub fn explorer_copy_to_queue(&mut self, idx: usize, path: PathBuf) {
    // Copy file to queue's target dir (but keep original)
    // Similar to submit, but doesn't remove source
}
```

**Step 2: Add UI Action** (`src/ui/explorer_panel.rs`)
```rust
enum Act {
    // ...
    CopyToQueue(usize, PathBuf),  // ← NEW
}

match action {
    // ...
    Act::CopyToQueue(i, p) => app.explorer_copy_to_queue(i, p),
}
```

**Step 3: Add UI Button** (`explorer_panel.rs`)
```rust
if ui.small_button("📋 Copy to Queue").clicked() {
    pending_action = Some(Act::CopyToQueue(queue_idx, file_path));
}
```

**Step 4: Handle Action** (dispatch in main loop or immediately)

---

## 5. KEY DATA STRUCTURES FOR FRONTEND

### Queue Representation
```rust
pub struct FileQueue {
    pub name: String,           // "Documents", "Downloads", …
    pub target: PathBuf,        // /Users/me/Documents
    pub items: Vec<PathBuf>,    // [/file1, /file2, …]
}

// Staged in app.rs:
pub explorer_queues: Vec<FileQueue>
```

**Frontend Usage**:
```rust
for (q_idx, q) in app.explorer_queues.iter().enumerate() {
    ui.label(&q.name);
    ui.label(format!("{} files", q.items.len()));
}
```

---

### Movable Item Representation
```rust
pub struct MovableItem {
    pub path: PathBuf,
    pub target_queue: Option<String>,  // None until user picks queue
}

// Staged in app.rs:
pub movable_items: Vec<MovableItem>
```

**Frontend Usage**:
```rust
for (idx, item) in app.movable_items.iter().enumerate() {
    ui.label(item.path.file_name().unwrap_or_default().to_string_lossy());
    
    ComboBox::new(id, "")
        .selected_text(item.target_queue.as_deref().unwrap_or("Select queue"))
        .show_ui(ui, |ui| {
            for q in &app.explorer_queues {
                ui.selectable_value(&mut item.target_queue, Some(q.name.clone()), &q.name);
            }
        });
    
    if ui.button("Move").clicked() {
        app.move_single_movable_item(idx);
    }
}
```

---

### Pending Action Representation
```rust
pub enum PendingAction {
    Trash {
        title: String,
        action: &'static str,  // "manual_delete", "auto_delete", …
        plan: ActionPlan,
    },
    Apply {
        mode: ActionMode,  // Delete or HardLink
        plan: ActionPlan,
    },
}

// In confirm_dialog.rs:
match &app.pending_action {
    Some(PendingAction::Trash { title, .. }) => {
        // Show dialog with title
    }
    Some(PendingAction::Apply { mode, plan }) => {
        // Show summary from plan
    }
    _ => {}
}
```

---

## 6. PROGRESS TRACKING IN UI

### Queue Submit Progress
```rust
// In main loop:
if app.explorer_queue_submitting {
    if let Ok(progress) = app.explorer_queue_submit_progress.lock() {
        let (done, total, current_file) = *progress;
        // Show progress bar: done / total
        // Show current file: current_file
    }
}
```

**Display Example**:
```
Submitting: 15 / 50 files
Current: photo_123.jpg
[████████░░░░░░░░░░░░] 30%
```

---

### Archive Files Move Progress
```rust
if app.archive_files_moving {
    if let Ok(progress) = app.archive_files_move_progress.lock() {
        let (done, total, current_file) = *progress;
        // Show progress bar
    }
}
```

---

## 7. ERROR HANDLING IN UI

### Current Pattern: Status Bar
```rust
// After operation:
app.explorer_status = Some(format!("Moved 5 files → Documents"));

// In status bar UI:
if let Some(status) = &app.explorer_status {
    ui.label(status);
}
```

**Improvements**:
- Use different colors for success/error/warning
- Auto-hide after 5 seconds
- Show error details on hover
- Offer "Undo" link for recoverable operations

---

### Error Log
```rust
app.show_error_log = true;  // Toggle

// Display log entries:
if app.show_error_log {
    ScrollArea::vertical().show(ui, |ui| {
        for entry in &app.apply_errors {
            ui.label(format!("❌ {}", entry));
        }
    });
}
```

---

## 8. RECOMMENDED UI IMPROVEMENTS

### 1. Destination Preview
**Current**: User submits files blindly to queue target
**Improvement**: Show destination path before submission

```rust
// Before submit button:
if let Some(q) = app.explorer_queues.get(active_queue_idx) {
    ui.label(format!("📁 Target: {}", q.target.display()));
}
```

---

### 2. Collision Warning
**Current**: Auto-renames on collision (e.g., `photo.jpg` → `photo_1.jpg`)
**Improvement**: Warn user and ask action before moving

```rust
// In confirm dialog:
"File 'photo.jpg' already exists in destination.
 ○ Skip this file
 ○ Rename to 'photo_1.jpg'
 ○ Overwrite (dangerous!)"
```

---

### 3. Drag-and-Drop
**Current**: Only context menu to add to queue
**Improvement**: Allow drag from explorer to queue panel

```rust
// In queue panel:
let response = ui.drop_target(false);
if response.hovered {
    ui.painter().rect_filled(ui.available_rect_before_wrap(), 4.0, Color32::LIGHT_BLUE);
}
if let Some(DroppedFile::Path(path)) = response.dropped_file {
    app.explorer_add_to_queue(active_queue_idx, path);
}
```

---

### 4. Undo Capability
**Current**: No undo (but files in Trash are recoverable)
**Improvement**: Track recent moves; offer "Move to..." undo

```rust
// Store recent operations:
struct RecentOperation {
    src: PathBuf,
    dst: PathBuf,
    timestamp: Instant,
    action: &'static str,
}
pub recent_ops: VecDeque<RecentOperation>  // Keep last 20

// UI: Show undo link
if let Some(op) = app.recent_ops.front() {
    if Instant::now().duration_since(op.timestamp) < Duration::from_secs(300) {
        if ui.link(format!("↶ Undo: moved '{}'", op.src.file_name().unwrap_or_default().to_string_lossy())) {
            app.undo_operation(&op);
        }
    }
}
```

---

### 5. Batch Queue Operations
**Current**: Only "Delete Queue" per queue
**Improvement**: Delete/rename/merge multiple queues

```rust
// Add to queue modal header:
if ui.button("🗑 Delete All Queues").clicked() {
    for _ in 0..app.explorer_queues.len() {
        app.explorer_queues.pop();
    }
}

if ui.button("📋 Merge Queues").clicked() {
    // Merge selected queues into one
}
```

---

### 6. Queue Reordering
**Current**: Fixed order (creation order)
**Improvement**: Reorder queues by dragging or move up/down buttons

```rust
// Add buttons:
if ui.button("▲").clicked() {
    if queue_idx > 0 {
        app.explorer_queues.swap(queue_idx, queue_idx - 1);
    }
}
if ui.button("▼").clicked() {
    if queue_idx < app.explorer_queues.len() - 1 {
        app.explorer_queues.swap(queue_idx, queue_idx + 1);
    }
}
```

---

### 7. Batch File Selection
**Current**: Single file "Remove from Queue"
**Improvement**: Multi-select and batch remove

```rust
// In queue detail modal:
let mut to_remove = Vec::new();
for (idx, path) in app.explorer_queues[q_idx].items.iter().enumerate() {
    let checked = app.queue_checked.contains(path);
    if ui.checkbox(&mut checked, path.file_name().unwrap_or_default().to_string_lossy()).clicked() {
        if checked {
            app.queue_checked.insert(path.clone());
        } else {
            app.queue_checked.remove(path);
        }
    }
}

if ui.button("Remove Selected").clicked() {
    for path in &app.queue_checked {
        app.explorer_remove_from_queue(q_idx, path.clone());
    }
    app.queue_checked.clear();
}
```

---

## 9. TESTING FRONTEND OPERATIONS

### Unit Test: Queue Add/Remove
```rust
#[test]
fn queue_add_remove() {
    let mut app = SortwiseApp::default();
    
    // Create queue
    app.explorer_queues.push(FileQueue {
        name: "Test".into(),
        target: PathBuf::from("/tmp/test"),
        items: vec![],
    });
    
    // Add file
    let path = PathBuf::from("/src/file.txt");
    app.explorer_add_to_queue(0, path.clone());
    assert_eq!(app.explorer_queues[0].items.len(), 1);
    
    // Remove file
    app.explorer_remove_from_queue(0, path);
    assert_eq!(app.explorer_queues[0].items.len(), 0);
}
```

---

### Integration Test: Move Operation
```rust
#[test]
fn move_single_movable_item() {
    let mut app = SortwiseApp::default();
    
    // Create temp files
    let src = "/tmp/source.txt";
    std::fs::write(src, "test").unwrap();
    
    // Create target queue
    let target_dir = "/tmp/target_dir";
    std::fs::create_dir_all(target_dir).unwrap();
    app.explorer_queues.push(FileQueue {
        name: "Target".into(),
        target: PathBuf::from(target_dir),
        items: vec![],
    });
    
    // Stage item
    app.movable_items.push(MovableItem {
        path: PathBuf::from(src),
        target_queue: Some("Target".into()),
    });
    
    // Move
    let success = app.move_single_movable_item(0);
    assert!(success);
    assert!(!PathBuf::from(src).exists());
    assert!(PathBuf::from("/tmp/target_dir/source.txt").exists());
}
```

---

## 10. HOTKEYS / KEYBOARD SHORTCUTS

**Suggested Additions**:
- `Ctrl+Shift+Q`: Create new queue
- `Ctrl+Shift+M`: Add selected to movable queue
- `Ctrl+Shift+T`: Submit all queues
- `Ctrl+M`: Move single movable item (if focused)
- `Del`: Remove from queue (if queue modal focused)

**Implementation**:
```rust
// In main UI loop:
if ctx.input(|i| i.key_pressed(egui::Key::D)) && app.explorer_queue_view_open {
    // Delete focused queue
}
```

