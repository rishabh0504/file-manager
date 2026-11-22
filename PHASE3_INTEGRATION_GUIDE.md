# Phase 3 Integration Guide

## Quick Start
Phase 3 modules are **completely standalone** — they don't modify existing code. To activate them:

## Step 1: Add fields to SortwiseApp struct
**File:** src/app.rs, in `pub struct SortwiseApp`

```rust
// Add after existing fields (around line 180):
pub operation_history: OperationHistory,
pub settings: AppSettings,
pub notifications: NotificationManager,
```

Then add imports at top:
```rust
use crate::types::{OperationHistory, AppSettings};
use crate::ui::NotificationManager;
```

## Step 2: Initialize in impl Default
**File:** src/app.rs, in `impl Default for SortwiseApp`

```rust
// Add in the Default impl:
operation_history: OperationHistory::new(500),
settings: AppSettings::load().unwrap_or_default(),
notifications: NotificationManager::new(),
```

## Step 3: Save settings on drop
**File:** src/app.rs, add Drop implementation

```rust
impl Drop for SortwiseApp {
    fn drop(&mut self) {
        let _ = self.settings.save();
    }
}
```

## Step 4: Track operations in move_ops.rs
**File:** src/operations/move_ops.rs

After `fast_move()` returns Ok:
```rust
// In caller (e.g., submit_all_queues in queue_ops.rs):
if moved {
    success_count += 1;
    
    // Track operation
    let mut record = OperationRecord::new(OperationType::Move, p.clone());
    record = record.with_destination(dest.clone()).with_success(true);
    app.operation_history.add(record);
    
    app.notifications.success(format!("Moved {} ✓", fname));
} else {
    fail_count += 1;
    
    let mut record = OperationRecord::new(OperationType::Move, p.clone());
    record = record.with_error("Failed to move file".to_string());
    app.operation_history.add(record);
    
    app.notifications.error(format!("Failed to move {} ✕", fname));
}
```

## Step 5: Render notifications in frame
**File:** src/app.rs, in `impl eframe::App`

In the `update()` method, after `apply_action()`:
```rust
// Render notifications at end of frame
self.notifications.render(ctx);
```

## Step 6: (Optional) Add settings UI
**File:** src/ui/settings_panel.rs (new file)

```rust
pub fn settings_panel(app: &mut SortwiseApp, ui: &mut egui::Ui) {
    ui.heading("⚙ Settings");
    
    if ui.checkbox(&mut app.settings.hide_system_files, "Hide system files").changed() {
        // Trigger reload if needed
    }
    if ui.checkbox(&mut app.settings.show_hidden_files, "Show hidden files").changed() {
        // Trigger reload if needed
    }
    if ui.checkbox(&mut app.settings.confirm_before_delete, "Confirm before delete").changed() {
    }
    
    ui.separator();
    
    if ui.button("💾 Save Settings").clicked() {
        let _ = app.settings.save();
    }
}
```

## Testing Checklist

After integration, verify:
- [ ] App starts without errors
- [ ] Settings file created at ~/.config/sortwise/settings.json
- [ ] Moving files shows notifications (top-right)
- [ ] Operation history records appear
- [ ] Settings persist after restart
- [ ] No compilation warnings
- [ ] Existing features still work (queues, file operations, etc)

## Minimal Integration (3 steps only)
If you want just notifications without full integration:

1. Add `notifications: NotificationManager::new()` to SortwiseApp
2. Call `self.notifications.render(ctx)` in frame
3. Use `app.notifications.success("message")` where needed

This requires **zero changes** to existing business logic.

## Rollback
If something breaks, Phase 3 is fully removable:
1. Revert types/mod.rs, ui/mod.rs to remove exports
2. Remove the 4 Phase 3 files
3. No changes to existing code means no rollback needed

---

**Files created:** 4 new files
**Files modified:** 2 (types/mod.rs, ui/mod.rs — only add exports)
**Breaking changes:** None
**Risk level:** Very Low
