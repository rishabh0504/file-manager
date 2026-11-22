# Sortwise - Project Analysis & Interactivity Improvements

## 🎯 Project Purpose

**Sortwise** is a Rust-based desktop file manager (macOS & Windows) that helps users:
1. Browse and organize files
2. Find and deduplicate files
3. Compare source and destination folders
4. Find and remove empty folders
5. Extract and manage archive files

Built with egui (immediate-mode GUI framework).

---

## 🎮 Current Features (5 Modes)

```
SortwiseApp {
    mode: AppMode {
        ✅ Explorer           → File browser with queues & staging
        ✅ Dedupe             → Find duplicate files
        ✅ CompareSourceDest  → Compare two folder trees
        ✅ EmptyFolders       → Scan and remove empty dirs
        ✅ ArchiveFiles       → Extract and organize archives
    }
}
```

### Explorer Mode (Most Interactive)
- File browsing
- Multi-selection
- Drag/drop support
- Preview window
- Queue staging (add files to move queues)
- Movable queue (temporary staging area)
- Real-time search with debounce

### Dedupe Mode (Less Interactive)
- Scan for duplicates
- Group results
- Select files to keep/remove
- Bulk delete with confirmation

### Other Modes (Minimal)
- Compare: Display diff results
- EmptyFolders: Scan and delete
- ArchiveFiles: Extract files

---

## ⚠️ Non-Interactive Operations (Problem Areas)

### 1. **Background Scanning** (No Progress/Feedback)
```rust
// In scan_panel.rs - runs in background thread
pub fn run_dedup_streaming(...) // No live progress
```
**Issue**: User clicks "Scan" and waits with no feedback
**Solution**: Add real-time progress updates (files scanned, speed, ETA)

### 2. **Result Processing** (Silent Computation)
```rust
// Duplicate detection completes "magically"
pub fn refresh_duplicates_from_result(...)
```
**Issue**: No visibility into what's happening
**Solution**: Show live group formation, sorting, filtering progress

### 3. **Bulk Operations** (Confirm Then Wait)
```rust
// Archive file move - no cancel or pause
pub fn execute_move_archive_files(...) // Blocking thread
pub fn submit_all_queues(...)           // Blocking thread
```
**Issue**: No way to pause/cancel once started
**Solution**: Add pause/resume/cancel buttons during execution

### 4. **Queue Management** (Drag-and-Drop Only)
```rust
// Explorer queues - all drag/drop based
pub explorer_queues: Vec<FileQueue>
```
**Issue**: No right-click context menus for queue operations
**Solution**: Add context menus, keyboard shortcuts

### 5. **File Selection** (Keyboard-Only for Some)
```rust
// No context menu - hard to discover features
pub explorer_selected: Option<usize>
```
**Issue**: Requires knowing keyboard shortcuts
**Solution**: Add right-click context menus everywhere

### 6. **Search** (Auto-triggered, Not Controllable)
```rust
// Search fires automatically with debounce
pub explorer_search_typed_at: Option<Instant>
pub explorer_search_pending: bool
```
**Issue**: Can't manually trigger, pause, or refine mid-search
**Solution**: Add search controls (Start, Stop, Pause buttons)

---

## 🎯 Recommended Interactivity Improvements

### High Priority

| Issue | Solution | Effort | Impact |
|-------|----------|--------|--------|
| **No scan progress** | Real-time progress bar + file count | 2h | CRITICAL |
| **No operation cancel** | Add Cancel button during scans/moves | 2h | CRITICAL |
| **No queue shortcuts** | Context menus + keyboard shortcuts | 3h | HIGH |
| **Silent operations** | Add progress indicators everywhere | 2h | HIGH |

### Medium Priority

| Issue | Solution | Effort | Impact |
|-------|----------|--------|--------|
| **Search not controllable** | Add Start/Stop/Clear buttons | 1.5h | MEDIUM |
| **No operation history** | Add log panel showing what happened | 2h | MEDIUM |
| **No drag confirmation** | Confirm before major drag operations | 1h | MEDIUM |
| **No keyboard navigation** | Tab between panels/controls | 2h | MEDIUM |

### Low Priority

| Issue | Solution | Effort | Impact |
|-------|----------|--------|--------|
| **Sparse tooltips** | Add tooltips to all buttons | 1h | LOW |
| **No settings UI** | Add preferences dialog | 3h | LOW |
| **No undo/redo** | Implement operation history | 4h | LOW |

---

## 📋 Implementation Priority

### Phase 1: Critical (Make Operations Visible)
1. **Progress Indicators**
   - Add progress bar to scan_panel.rs
   - Show live file count + speed
   - Display ETA

2. **Operation Control**
   - Add Cancel button to scanning operations
   - Pause/Resume for file moves
   - Stop button for searches

3. **User Feedback**
   - Toast notifications for completed ops
   - Status bar with current operation
   - Log panel with operation history

### Phase 2: Usability (Keyboard & Context Menus)
1. **Context Menus**
   - Right-click files → (Add to queue, Move, Copy, Delete)
   - Right-click queue → (Rename, Delete, Change target)
   - Right-click folder → (Enter, Open in Finder)

2. **Keyboard Shortcuts**
   - Delete → Send to trash
   - Cmd+C → Copy selected
   - Cmd+X → Cut to movable queue
   - Cmd+V → Paste/move

3. **Tab Navigation**
   - Tab moves between sections
   - Arrow keys navigate file list
   - Enter to select/open

### Phase 3: Polish (UX Refinements)
1. **Search UX**
   - Search box always visible
   - Start/Stop/Clear buttons
   - Save search history

2. **Operation History**
   - Log panel shows all operations
   - Undo last 10 operations
   - Export log

3. **Settings**
   - Preference dialog
   - Custom keyboard shortcuts
   - Theme selection

---

## 🔧 Specific Code Changes Needed

### 1. Add Progress Panel
```rust
// src/ui/progress_indicator.rs (NEW)
pub struct OperationProgress {
    pub operation: String,
    pub current: u64,
    pub total: u64,
    pub speed: f64,  // files/sec
    pub eta: Duration,
    pub can_cancel: bool,
}

pub fn render_progress(...) -> bool {  // Returns true if cancelled
    // Show progress bar + stats
    // Add Cancel button
}
```

### 2. Add Context Menu Handler
```rust
// src/ui/context_menu.rs (NEW)
pub fn handle_context_menu_click(
    path: &Path,
    menu_item: ContextMenuItem,
    app: &mut SortwiseApp,
) {
    match menu_item {
        ContextMenuItem::AddToQueue(queue_idx) => { /* add to queue */ }
        ContextMenuItem::MoveToTrash => { /* delete */ }
        ContextMenuItem::CopyPath => { /* copy to clipboard */ }
        // ...
    }
}
```

### 3. Add Keyboard Handler
```rust
// src/ui/keyboard.rs (NEW)
pub fn handle_key_press(
    key: egui::Key,
    modifiers: egui::Modifiers,
    app: &mut SortwiseApp,
) {
    match (key, modifiers) {
        (egui::Key::Delete, _) => { /* trash */ }
        (egui::Key::C, Modifiers::MAC_CMD) => { /* copy */ }
        (egui::Key::X, Modifiers::MAC_CMD) => { /* cut */ }
        // ...
    }
}
```

---

## 🎯 Why These Changes Matter

**Current State**: Sortwise feels "batch-oriented"
- Scan everything → see results → bulk action
- No feedback while processing
- No way to interact during operations
- Steep learning curve (discover via UI exploration)

**After Changes**: Sortwise feels "interactive"
- Real-time feedback on all operations
- Pause/cancel at any time
- Right-click context menus everywhere
- Keyboard shortcuts + tab navigation
- Clear operation log

---

## 📊 Estimated Effort

| Phase | Hours | Priority |
|-------|-------|----------|
| Phase 1 (Progress + Control) | 6-8h | CRITICAL |
| Phase 2 (Context + Keyboard) | 6-8h | HIGH |
| Phase 3 (Polish) | 4-6h | LOW |
| **TOTAL** | **16-22h** | - |

---

## ✅ Benefits

✅ **Better UX** - Users know what's happening  
✅ **More Discoverable** - Right-click menus show options  
✅ **More Efficient** - Keyboard shortcuts speed up workflow  
✅ **More Powerful** - Cancel/pause operations mid-flight  
✅ **Professional Feel** - Like real file managers (Finder, Explorer)

