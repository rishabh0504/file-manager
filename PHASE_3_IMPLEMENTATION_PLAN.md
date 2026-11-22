# Phase 3 Implementation Plan - Extract State Management

## Status: Planning Phase

This document outlines the approach for Phase 3 of backend restructuring: extracting state management from the monolithic SortwiseApp struct into focused, logical state modules.

---

## Current State

**SortwiseApp struct**: ~468 lines, ~120 fields

Grouped logically by comments but all fields in one struct:
- Explorer navigation & browsing (~40 fields)
- Queue management (~12 fields)
- Movable queue (~8 fields)
- Files recursive view (~15 fields)
- Folder modal (~5 fields)
- Dedup workflow & scanning (~35 fields)
- Results/duplicates (~15 fields)
- Settings & caching (~15 fields)

---

## Phase 3 Strategy

### Step 1: Create State Structs

Create 5-6 new focused structs to hold related state:

```rust
// 1. Explorer State (~50 fields)
pub struct ExplorerState {
    pub dir: PathBuf,
    pub entries: Vec<ExplorerEntry>,
    pub back: Vec<PathBuf>,
    pub fwd: Vec<PathBuf>,
    pub selected: Option<usize>,
    pub multi: HashSet<PathBuf>,
    pub show_hidden: bool,
    pub sort_key: ExplorerSortKey,
    // ... ~40 more fields
}

// 2. Queue State (~12 fields)
pub struct QueueState {
    pub queues: Vec<FileQueue>,
    pub active_queue: Option<usize>,
    pub submitting: bool,
    pub submit_rx: Option<Receiver<(usize, usize)>>,
    pub submit_progress: Arc<Mutex<(usize, usize, String)>>,
    // ...
}

// 3. Movable Queue State (~8 fields)
pub struct MovableQueueState {
    pub items: Vec<MovableItem>,
    pub queue_open: bool,
    pub playing_path: Option<PathBuf>,
    pub last_chosen_queue: Option<String>,
    pub size_cache: HashMap<PathBuf, u64>,
    // ...
}

// 4. Duplicates/Results State (~15 fields)
pub struct DuplicatesState {
    pub last_result: Option<Arc<ScanResult>>,
    pub results_epoch: u64,
    pub selected_group: Option<usize>,
    pub selected_files: HashSet<PathBuf>,
    pub results_filter: String,
    pub pending_action: Option<PendingAction>,
    // ...
}

// 5. Scan/Configuration State (~35 fields)
pub struct ScanState {
    pub roots: Vec<PathBuf>,
    pub source_roots: Vec<PathBuf>,
    pub dest_roots: Vec<PathBuf>,
    pub preferred_dirs: String,
    pub skip_hidden: bool,
    pub skip_system_dirs: bool,
    pub min_file_size: u64,
    pub action_mode: ActionMode,
    // ...
}

// 6. Files View State (~15 fields)
pub struct FilesViewState {
    pub root: Option<PathBuf>,
    pub files: Vec<RecursiveFile>,
    pub loading: bool,
    pub rx: Option<Receiver<Listing>>,
    pub checked: HashSet<PathBuf>,
    pub sort: ExplorerFilesSort,
    // ...
}
```

### Step 2: Update SortwiseApp

Replace flat fields with nested structs:

```rust
pub struct SortwiseApp {
    pub mode: AppMode,
    pub view: AppView,
    
    pub explorer: ExplorerState,
    pub queues: QueueState,
    pub movable: MovableQueueState,
    pub duplicates: DuplicatesState,
    pub scan: ScanState,
    pub files: FilesViewState,
    
    // Remaining shared state
    pub token: CancellationToken,
    pub status: Option<String>,
    // ...
}
```

### Step 3: Update All Accessors

Change field access patterns:
- `app.explorer_dir` → `app.explorer.dir`
- `app.explorer_entries` → `app.explorer.entries`
- `app.explorer_queues` → `app.queues.queues`
- etc.

**Impact**: ~400+ accessor changes across app.rs

### Step 4: Create State Initialization

Update `impl Default for SortwiseApp` to initialize nested structs.

### Step 5: Update Operations Module

Operations functions already take `&mut SortwiseApp`, so they'll work after accessor update.

---

## Implementation Complexity

### High Impact Areas
1. **Main app.rs**: ~200+ field accesses to update
2. **UI methods**: Methods that access multiple state groups
3. **Operations module**: May need accessor helpers
4. **Tests**: Any tests accessing app fields

### Estimated Changes
- Lines modified: ~500-700
- Files modified: app.rs (mostly)
- Risk: Medium (many accessor changes, easy to miss one)
- Verification: Compiler will catch missed accessors

---

## Expected Results

| Metric | Before | After | Benefit |
|--------|--------|-------|---------|
| SortwiseApp fields | 120 flat fields | 6 state struct fields | Clear organization |
| Largest struct | SortwiseApp (468 lines) | ExplorerState (~100 lines) | Easier to understand |
| Field grouping | Implicit (via comments) | Explicit (structs) | Type safety |
| Access pattern | `app.explorer_dir` | `app.explorer.dir` | Semantic clarity |
| Total app.rs lines | ~2788 (after Phase 2) | ~2800-2850 | Slight growth (new state structs) |

---

## Backward Compatibility

### Challenge
Changing field access patterns requires updating all call sites.

### Solution Options

**Option A: Direct Migration** (Recommended)
- Update all accessors at once
- One large refactoring commit
- Compiler catches errors
- Risk: Higher, but controlled by compiler

**Option B: Gradual Migration**
- Create helper accessors: `fn explorer_dir(&self) -> &PathBuf { &self.explorer.dir }`
- Gradually update call sites
- Risk: Lower, but leaves tech debt

**Option C: Delegation Pattern**
- Create forwarding methods on SortwiseApp
- Keep external interface the same
- Slower accessor performance
- Risk: Lowest, but adds complexity

**Recommended**: Option A (direct migration with compiler verification)

---

## Implementation Order

1. Create new state structs (types.rs or separate module)
2. Update SortwiseApp struct to nest states
3. Update Default impl
4. Run compiler, fix all accessor errors (~200+ fixes)
5. Test compilation and unit tests
6. Verify all integration tests pass
7. Commit as one large refactoring PR

---

## Success Criteria

- ✅ Code compiles without errors
- ✅ All unit tests pass (8/8)
- ✅ No semantic changes (only structural)
- ✅ Clear state organization
- ✅ Type safety improved

---

## Effort Estimate

- **Analysis**: 30 minutes (done)
- **Implementation**: 2-3 hours
- **Testing/Verification**: 30-45 minutes
- **Total**: 3-4 hours

---

## Why This Approach

1. **Clear Ownership**: Each state struct owns its domain
2. **Easier Navigation**: Logical grouping matches code comments
3. **Type Safety**: Compiler catches missed accessors
4. **Future Scalability**: Easier to extend state management
5. **Testability**: Can mock/test individual state structs

---

## Risks

1. **Large Refactoring**: Many changes in one commit
   - Mitigation: Compiler verification, unit tests

2. **Missed Accessors**: Easy to miss a field access
   - Mitigation: Compiler catches these

3. **Performance**: Slightly deeper nesting
   - Impact: Negligible (all inlined)

---

## Alternative: Don't Do Phase 3

Phase 3 is **optional**. Benefits are organizational, not performance:
- Phase 1-2 achieved: 650 lines reduction (18%)
- Context savings: 85% for move operations
- Operations module: Clean and focused

Phase 3 would add: Better state organization, clearer code structure.

---

## Next: When to Start Phase 3

Consider starting Phase 3 when:
1. ✅ Current WIP changes (FilesFilter, AllFiles) are merged
2. ✅ Binary builds successfully
3. ✅ All UI features are working

Do NOT start if:
- ❌ Ongoing WIP changes to app.rs
- ❌ Binary build failing due to missing types
- ❌ Time-sensitive feature work

---

## Recommended Decision

**Current Status**: Phase 2 complete, Phase 3 ready for planning

**Recommendation**: 
- Fix binary build errors (merge/cleanup WIP changes first)
- Then proceed with Phase 3 when codebase is stable

This ensures Phase 3 refactoring doesn't collide with active feature work.

