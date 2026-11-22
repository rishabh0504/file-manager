# Phase 0: Type & Constant Extraction Plan

Extract all type definitions, enums, constants, and structs from `app.rs` into organized modules.

**Goal**: Reduce app.rs from 2788 lines to ~1200 lines (pure implementation only)

---

## 📊 Current State

**app.rs content breakdown**:
- Lines 1-20: Imports & constants
- Lines 20-200: Type/enum/struct definitions (180 lines)
- Lines 200+: SortwiseApp struct + methods

**Problem**: Types mixed with implementation logic makes it hard to understand app.rs purpose and structure.

---

## 🎯 Extraction Plan

### 1. Create `src/types/mod.rs`

**Purpose**: All public type definitions and data structures

**Extract from app.rs** (lines 30-201):
```
Enums (8):
  ✓ AppView
  ✓ AppMode
  ✓ ExplorerSortKey
  ✓ ExplorerViewMode
  ✓ ExplorerFilesSort
  ✓ ExplorerTypeFilter
  ✓ ScanWorkKind
  ✓ DupGroupSortOrder
  ✓ PendingAction

Structs (7):
  ✓ ExplorerEntry
  ✓ GroupOutcome
  ✓ FileQueue
  ✓ MovableItem
  ✓ ThumbnailRequest
  ✓ SnapshotPlan
  ✓ ThumbnailResponse
  ✓ ExtractBanner
  ✓ DupListCache
```

**File structure**:
```
src/types/
├── mod.rs              (re-exports all)
├── app_view.rs         (AppView, AppMode)
├── explorer.rs         (Explorer* enums/structs)
├── scan.rs             (ScanWorkKind, DupGroupSortOrder)
├── queue.rs            (FileQueue, MovableItem)
├── thumbnails.rs       (ThumbnailRequest, ThumbnailResponse)
├── actions.rs          (PendingAction, ActionPlan)
└── cache.rs            (DupListCache, etc)
```

**File size estimates**:
- mod.rs: 50 lines (re-exports)
- app_view.rs: 30 lines
- explorer.rs: 80 lines
- scan.rs: 50 lines
- queue.rs: 30 lines
- thumbnails.rs: 40 lines
- actions.rs: 30 lines
- cache.rs: 30 lines
**Total**: ~330 lines (clean, organized)

---

### 2. Create `src/constants.rs`

**Purpose**: All compile-time constants

**Extract from app.rs**:
```
✓ SEARCH_DEBOUNCE: Duration
✓ MEDIA_EXTS: &[&str]
(+ any other consts we find)
```

**File structure**: Single flat file (50 lines max)

---

### 3. Update `src/main.rs`

**Add**:
```rust
mod types;
mod constants;
```

**Update imports**:
```rust
use crate::types::{AppView, AppMode, ...};
use crate::constants::{SEARCH_DEBOUNCE, MEDIA_EXTS};
```

---

### 4. Update `src/app.rs`

**After extraction**:
- Remove lines 1-201 (types + consts)
- Add imports: `use crate::types::*; use crate::constants::*;`
- Keep lines 202+ (SortwiseApp struct + impl blocks)

**New app.rs structure**:
```
1. Imports (crate::types, crate::constants, external)
2. SortwiseApp struct definition (450+ lines of fields)
3. impl SortwiseApp (methods) (1500+ lines)
4. Any standalone functions
```

---

## 📈 Expected Impact

| Metric | Before | After | Reduction |
|--------|--------|-------|-----------|
| app.rs lines | 2788 | ~1400 | 50% |
| Type definitions | Mixed in app.rs | Organized modules | Clear separation |
| Compile focus | Types + Logic | Logic only | Better readability |
| Binary errors | 45 (types undefined) | 0 (types organized) | Cleaner imports |
| Phase 3 readiness | Blocked | Clear | Ready for state struct extraction |

---

## 🔧 Implementation Steps

### Step 1: Create type modules
Create `src/types/mod.rs` and submodules with all extracted types

### Step 2: Create constants module
Create `src/constants.rs` with all consts

### Step 3: Update main.rs
Add module declarations and re-exports

### Step 4: Update app.rs
Remove type definitions, add imports

### Step 5: Verify
- Library builds: `cargo build --lib`
- Binary builds: `cargo build`
- Tests pass: `cargo test`

---

## ⏱️ Effort Estimate

**Time**: 2-3 hours total
- Create modules: 30 min
- Extract types: 45 min
- Fix imports: 30 min
- Verify & test: 30 min

**Risk**: Low (pure refactor, no logic changes)

**Blockers resolved**:
- ✅ Cleaner imports for operations modules
- ✅ Binary can find types if organized properly
- ✅ Clearer code organization
- ✅ Foundation for Phase 3

---

## 📋 Before/After Examples

### Before
```rust
// app.rs line 30
pub enum AppView { Scan, Scanning, Results }

// ... 170 lines of other types ...

// app.rs line 202
pub struct SortwiseApp { ... }

impl SortwiseApp { ... }
```

### After
```rust
// types/mod.rs
pub mod app_view;
pub mod explorer;
pub mod scan;
pub mod queue;
// ...
pub use app_view::{AppView, AppMode};
pub use explorer::{ExplorerSortKey, ...};
// ... all re-exports

// app.rs line 1
use crate::types::*;
use crate::constants::*;

pub struct SortwiseApp { ... }

impl SortwiseApp { ... }
```

---

## 🎯 Ready to Proceed?

This plan:
1. ✅ Organizes code by concern
2. ✅ Reduces app.rs to pure implementation
3. ✅ Fixes binary build import issues
4. ✅ Prepares foundation for Phase 3
5. ✅ Zero breaking changes (all types remain public)

**Recommended**: Execute Phase 0 before Phase 3 to stabilize codebase.
