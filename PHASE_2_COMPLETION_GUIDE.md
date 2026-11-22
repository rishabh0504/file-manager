# Phase 2 Implementation Guide - Complete Operations Module

## Status
✅ Phase 1 Complete (committed)  
⏳ Phase 2 Ready for Implementation  

## What Phase 2 Does
Removes 666 lines from app.rs (19% reduction) by converting method implementations into forwarding calls to the operations module.

## Implementation Steps

### 1. Populate Operations Module Files

Create the 4 missing operations/*.rs files:

```bash
# Create move_ops.rs (170 lines)
# - fast_move()
# - fast_move_dir()
# - copy_dir_recursive()
# - unique_file_dest()
# - unique_dir_dest()
```

[See RESTRUCTURING_PHASE_1.md for detailed content of each file]

### 2. Replace Method Implementations in app.rs

Replace these 17 method implementations with forwarding calls:

**Queue ops**:
- `explorer_new_queue()` → `crate::operations::explorer_new_queue(self)`
- `explorer_add_to_queue()` → `crate::operations::explorer_add_to_queue(self, idx, path)`
- `explorer_remove_from_queue()` → `crate::operations::explorer_remove_from_queue(self, idx, path)`
- `explorer_delete_queue()` → `crate::operations::explorer_delete_queue(self, idx)`
- `explorer_toggle_queue()` → `crate::operations::explorer_toggle_queue(self, idx, path)`
- `explorer_change_queue_target()` → `crate::operations::explorer_change_queue_target(self, idx)`
- `submit_all_queues()` → `crate::operations::submit_all_queues(self)`
- `poll_queue_submit_result()` → `crate::operations::poll_queue_submit_result(self)`

**Movable ops**:
- `is_in_movable()` → `crate::operations::is_in_movable(self, path)`
- `stage_paths()` → `crate::operations::stage_paths(self, paths)`
- `toggle_movable()` → `crate::operations::toggle_movable(self, path)`
- `remove_from_movable()` → `crate::operations::remove_from_movable(self, idx)`
- `clear_movable_items()` → `crate::operations::clear_movable_items(self)`
- `move_single_movable_item()` → `crate::operations::move_single_movable_item(self, idx)`
- `move_all_movable_items()` → `crate::operations::move_all_movable_items(self)`

**Archive ops**:
- `execute_move_archive_files()` → `crate::operations::execute_move_archive_files(self)`
- `poll_archive_files_move_result()` → `crate::operations::poll_archive_files_move_result(self)`

### 3. Test & Verify

```bash
# Check compilation
cargo build

# Run tests
cargo test

# Verify line reduction
wc -l src/app.rs
# Expected: ~2770 lines (down from 3438)
```

### 4. Commit

```bash
git add src/operations/ src/app.rs
git commit -m "refactor: complete Phase 2 - replace duplicated methods with forwarding calls

- Removed 666 lines from app.rs (19% reduction)
- All methods now forward to operations module
- Zero API changes (public interface unchanged)
- Safe migration pattern (old and new coexist during transition)

Metrics:
- app.rs: 3438 → 2772 lines
- Module organization: operations module fully populated
- Compilation: ✓ All tests pass
- Context savings: Continued from Phase 1 (85% for move ops)

This completes Phase 2. Phase 3 (extract state) can proceed next.

Co-Authored-By: Claude Haiku 4.5 <noreply@anthropic.com>"
```

## File Locations

Operations module files needed:
- `src/operations/move_ops.rs` (170 lines)
- `src/operations/queue_ops.rs` (220 lines)
- `src/operations/movable_ops.rs` (190 lines)
- `src/operations/archive_ops.rs` (90 lines)

[Files are documented in RESTRUCTURING_PHASE_1.md - copy the content from there]

## Expected Outcome

After Phase 2:
- ✓ app.rs reduced by 666 lines (19%)
- ✓ All methods still callable (same public API)
- ✓ Implementation in operations module, not app.rs
- ✓ Builds successfully
- ✓ All tests pass
- ✓ Ready for Phase 3 (optional state extraction)

## Why This Approach is Safe

1. **No API Changes**: Methods still callable the same way
2. **Forwarding Pattern**: Just delegates to module functions
3. **Incremental**: One method at a time, test after each
4. **Reversible**: Can revert if issues found
5. **Coexists**: Old and new logic both present during transition

## Next: Phase 3 (Optional)

After Phase 2 is complete and tested:
- Extract state management
- Split SortwiseApp into focused structs
- Further reduce app.rs to ~1500 lines

See PHASE_3_PLAN.md for details.
