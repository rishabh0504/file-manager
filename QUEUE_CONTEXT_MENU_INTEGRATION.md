# Adding Queue Context Menu to Explorer Files

## Goal
Add a "📋 Add to Queue" submenu to the right-click context menu for files, allowing users to pick a target queue without opening a separate dialog.

## Current State
- ✅ File context menu exists (line ~1130 in explorer_panel.rs)
- ✅ Queue operations work (explorer_add_to_queue, etc)
- ⏳ Queue menu not yet shown in context menu

## Integration Steps

### Step 1: Modify item_interactions() signature
**File:** src/ui/explorer_panel.rs, line ~1108

```rust
// OLD:
fn item_interactions(resp: &egui::Response, idx: usize, e: &ExplorerEntry, staged: bool, act: &mut Option<Act>) {

// NEW:
fn item_interactions(resp: &egui::Response, idx: usize, e: &ExplorerEntry, staged: bool, app: &SortwiseApp, act: &mut Option<Act>) {
```

### Step 2: Update call sites (2 locations)
**File:** src/ui/explorer_panel.rs

Lines ~869 and ~1101:
```rust
// OLD:
item_interactions(&resp, idx, &e, staged, act);

// NEW:
item_interactions(&resp, idx, &e, staged, app, act);
```

### Step 3: Add queue menu to context_menu closure
**File:** src/ui/explorer_panel.rs, in item_interactions() after line ~1150

Add after "Copy path" button:
```rust
        // Queue operations
        if !app.explorer_queues.is_empty() {
            ui.menu_button("📋 Add to Queue", |ui| {
                for (idx, q) in app.explorer_queues.iter().enumerate() {
                    if ui.button(&q.name).clicked() {
                        use crate::operations::explorer_add_to_queue;
                        explorer_add_to_queue(app, idx, e.path.clone());
                        ui.close_menu();
                    }
                }
            });
        }
        
        // Movable queue option
        if ui.button("🔗 Add to Movable Queue").clicked() {
            *act = Some(Act::Stage(e.path.clone()));
            ui.close_menu();
        }
```

## Expected Result
When right-clicking a file:
```
├─ Open in player
├─ Open in default app
├─ 📋 Add to Queue ▶
│  ├─ Documents
│  ├─ Downloads  
│  └─ Archive
├─ 🔗 Add to Movable Queue
├─ Copy path
├─ Reveal in Finder
└─ 🗑 Move to Trash
```

## Notes
- The menu_button() with nested for loop is egui's way of handling submenus
- explorer_add_to_queue() will show status message when file is added
- If no queues exist, the menu is skipped (< 1 LOC overhead)
- Movable queue option always appears (non-queue alternative)

## Verification
1. Add queue in UI
2. Right-click a file
3. Hover "📋 Add to Queue" to see submenu
4. Click queue name
5. Verify file appears in that queue in the queue panel
6. Check status bar shows "Added X to queue Y"
