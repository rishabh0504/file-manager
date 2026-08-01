# Smart File Manager — Implementation Plan

> **Goal:** A deterministic, batch-based, multi-stage duplicate file detection engine with a Rust-native egui UI. Scan → Index → Deduplicate with maximum throughput, minimum I/O, and full cancellability.
>
> **Status:** v4 — Production-grade. All 15 critical improvements + 5 senior-polish enhancements + Folder-Selection Mode integrated.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Crate & Dependency Plan](#2-crate--dependency-plan)
3. [Module Layout](#3-module-layout)
4. [Pipeline Stages — Detailed Design](#4-pipeline-stages--detailed-design)
   - Stage 0: I/O Scheduler Layer *(NEW)*
   - Stage 1: File Enumerator
   - Stage 2: Size Bucketing + Large-Group Chunking *(UPGRADED)*
   - Stage 3: Hard-Link / Inode Pre-Filter *(NEW)*
   - Stage 4: Partial Hashing *(FIXED — consistent 3-region sampling)*
   - Stage 5: Full Hashing
   - Stage 6: Byte-Level Collision Verification *(NEW)*
   - Stage 7: Duplicate Resolution *(UPGRADED — smart scoring)*
5. [Concurrency Model](#5-concurrency-model)
6. [Cancellation System](#6-cancellation-system)
7. [Memory Strategy](#7-memory-strategy)
8. [Error Handling Strategy](#8-error-handling-strategy) *(NEW)*
9. [Action Mode System](#9-action-mode-system) *(NEW)*
10. [UI Layer Design](#10-ui-layer-design)
11. [File-by-File Implementation Checklist](#11-file-by-file-implementation-checklist)
12. [Complexity & Performance Targets](#12-complexity--performance-targets)
13. [Critical Improvements Index](#13-critical-improvements-index)
14. [Advanced Upgrade: Similarity Detection via LSH](#14-advanced-upgrade-similarity-detection-via-lsh)
15. [Key Design Decisions](#15-key-design-decisions)
16. [Final 5% — Critical Correctness Fixes](#16-final-5--critical-correctness-fixes)
    - Fix A: Cross-Platform File Identity (`FileId`)
    - Fix B: mmap Safety Guard
    - Fix C: Atomic HardLink Mode
    - Fix D: O(N) Byte Verification
    - Fix E: Parallel BFS Directory Scanner
17. [Senior-Level Polish Enhancements](#17-senior-level-polish-enhancements)
    - Enhancement 1: Hash Cache (Incremental Scans)
    - Enhancement 2: File-Type Heuristics
    - Enhancement 3: Adaptive Chunk Size
    - Enhancement 4: Byte-Based Progress Accuracy
    - Enhancement 5: NUMA-Aware Thread Pinning
18. [Folder-Selection Mode Adjustments](#18-folder-selection-mode-adjustments) *(NEW)*
    - 1. I/O Scheduling — Per-Folder Thread Assignment
    - 2. Partial / Full Hash — Scope Unchanged
    - 3. Hard-Link / Inode Filtering — Cross-Selection Awareness
    - 4. Memory Pressure — Smaller Scope, Still Chunked
    - 5. Cancellation & Progress — Per-Folder Granularity
    - 6. Smart Keep Policy — Folder-Context Scoring
    - 7. Action Mode — Per-Folder Preview
    - 8. Hash Collision Safety — No Change
    - 9. Cross-Platform FileId — Network/Symlink Edge Cases
    - 10. BFS Depth — Per-Folder Parallel Scan
    - Folder-Specific Enhancements (4 new)

---

## 1. Architecture Overview

```
User selects folders (egui UI)
           │
           ▼
┌─────────────────────────────┐
│   File Enumerator           │  BFS queue, multi-threaded, skips system/hidden
│   (src/scanner.rs)          │  Emits: Vec<FileEntry { path, size, inode, dev, mtime }>
└────────┬────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│   Inode / Hard-Link Filter  │  NEW: group by (dev, inode) FIRST
│   (src/pipeline.rs)         │  Hard-linked files → already same → skip hashing
└────────┬────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│   Size Bucketer             │  HashMap<u64, Vec<FileEntry>>
│   (src/pipeline.rs)         │  Large size-groups → chunked (MAX_GROUP=10_000)
└────────┬────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│   I/O Scheduler             │  NEW: sort by parent dir, emit IOJob queue
│   (src/io_scheduler.rs)     │  2–4 reader threads → in-memory buffer → CPU hashers
└────────┬────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│   Partial Hasher            │  FIXED: consistent 3-region sampling
│   (src/hasher.rs)           │  chunk_size = min(8192, size) for all 3 probes
└────────┬────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│   Full Hasher               │  Only collision survivors (~1-5% of files)
│   (src/hasher.rs)           │  BLAKE3 full-file streaming, cancellation-aware
└────────┬────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│   Byte-Level Verifier       │  NEW: byte-by-byte comparison for confirmed dupes
│   (src/verifier.rs)         │  Guards against BLAKE3 collision (negligible but real)
└────────┬────────────────────┘
         │
         ▼
┌─────────────────────────────┐
│  Duplicate Resolver         │  UPGRADED: smart scoring (depth + atime + preferred dirs)
│  (src/resolver.rs)          │  ActionMode: DryRun | Delete | HardLink
└────────┬────────────────────┘
         │
         ▼
  egui Results Panel  →  Groups, stats, per-file actions, partial-result banner
```

**Key property:** The scan is a one-shot batch operation — no filesystem watchers, no daemon.

---

## 2. Crate & Dependency Plan

### Updated `Cargo.toml`

```toml
[package]
name = "smart-file-manager"
version = "0.1.0"
edition = "2024"

[dependencies]
# UI
eframe    = "0.28.1"

# Hashing (hardware-accelerated BLAKE3)
blake3    = "1.5"

# Parallel CPU work
rayon     = "1.10"

# Backpressure channels between pipeline stages
crossbeam-channel = "0.5"

# Lock-free cancellation flag
atomic    = { version = "0.6", features = ["std"] }

# Directory walking
walkdir   = "2.5"

# Error handling
anyhow    = "1.0"

# CPU count detection for pool sizing
num_cpus  = "1.16"

# Memory-mapped I/O for large files (>64MB)
memmap2   = "0.9"

# Native file dialog (folder picker)
rfd       = "0.14"

# JSON report export
serde     = { version = "1", features = ["derive"] }
serde_json = "1"

# Locality-Sensitive Hashing (Advanced Similarity Mode)
# lsh-rs = "0.9"   ← enable for Advanced Mode

# Content-Defined Chunking / Rabin fingerprinting (Advanced Mode)
# cdchunking = "1.0"   ← enable for Advanced Mode

[profile.release]
opt-level     = 3
lto           = "thin"
codegen-units = 1
```

---

## 3. Module Layout

```
src/
├── main.rs              # Entry point: eframe window setup, dark mode, fonts
├── app.rs               # App state struct, eframe::App impl, panel routing
│
├── types.rs             # All shared types (FileEntry, ScanResult, DuplicateGroup, …)
├── cancel.rs            # CancellationToken: Arc<AtomicBool> wrapper
├── errors.rs            # FileError enum, per-file Result type        [NEW]
│
├── scanner.rs           # BFS file enumerator, inode capture
├── io_scheduler.rs      # I/O job queue, reader thread pool            [NEW]
├── hasher.rs            # partial_hash(), full_hash() — BLAKE3 + cancellation
├── verifier.rs          # Byte-by-byte final verification              [NEW]
├── pipeline.rs          # Stage orchestration, chunked group processing
├── resolver.rs          # Duplicate grouping, smart scoring, ActionMode
│
└── ui/
    ├── mod.rs           # Re-exports all panels
    ├── scan_panel.rs    # Folder selector, config, ActionMode selector, "Start Scan"
    ├── progress_panel.rs # 2-phase progress (streaming + processing), Cancel button
    └── results_panel.rs  # Groups, per-file actions, partial-result banner, export
```

---

## 4. Pipeline Stages — Detailed Design

---

### Stage 0 (NEW): I/O Scheduler Layer (`src/io_scheduler.rs`)

> **Fix for: Critical Improvement #1 — Disk Thrashing from Rayon par_iter**

**Problem:** `rayon::par_iter()` has no awareness of disk layout. On HDDs it triggers random-seek storms — up to 10× slower than sequential reads.

**Solution:** Decouple I/O from CPU work using a dedicated reader thread pool + bounded buffer channel.

```rust
pub struct IOJob {
    pub path: PathBuf,
    pub size: u64,
    pub offset: u64,       // for partial reads
    pub length: usize,     // bytes to read
}

pub struct IOResult {
    pub path: PathBuf,
    pub data: Vec<u8>,
    pub error: Option<FileError>,
}
```

**Scheduling algorithm:**

```rust
// Step 1: Sort by parent directory (improves locality)
jobs.sort_by_key(|j| j.path.parent().map(|p| p.to_path_buf()));

// Step 2: Route jobs to bounded channel (backpressure built-in)
let (job_tx, job_rx) = crossbeam_channel::bounded(512);  // backpressure cap
let (buf_tx, buf_rx) = crossbeam_channel::bounded(256);  // buffer to CPU workers

// Step 3: Spawn 2–4 reader threads (not Rayon — explicit I/O pool)
for _ in 0..NUM_IO_THREADS {
    let rx = job_rx.clone();
    let tx = buf_tx.clone();
    std::thread::spawn(move || {
        for job in rx {
            let result = read_job(&job);
            tx.send(result).ok();
        }
    });
}

// Step 4: CPU hash workers consume from buf_rx via Rayon
rayon::scope(|s| {
    for result in &buf_rx {
        s.spawn(|_| hash_buffer(result));
    }
});
```

**Constants:**
```rust
const NUM_IO_THREADS: usize = 3;    // sweet spot for both SSD and HDD
const IO_CHANNEL_CAP: usize = 512;  // backpressure: prevents RAM bloat
const BUF_CHANNEL_CAP: usize = 256;
```

**Expected gain:** 2–5× on HDD, 1.2–1.5× on SSD.

---

### Stage 1: File Enumerator (`src/scanner.rs`)

**Algorithm:** Queue-based BFS (never recursive DFS) to avoid stack overflow on deep trees.

```rust
pub struct FileEntry {
    pub path:  PathBuf,
    pub size:  u64,
    pub mtime: SystemTime,
    pub inode: u64,          // for hard-link detection
    pub dev:   u64,          // device ID — inode only unique within a device
}

pub struct ScanConfig {
    pub roots:            Vec<PathBuf>,
    pub skip_hidden:      bool,
    pub skip_system_dirs: bool,
    pub min_file_size:    u64,     // default: 1 byte
    pub action_mode:      ActionMode,
}
```

**Implementation notes:**
- Use `std::collections::VecDeque` as BFS queue.
- Batch `read_dir` calls — `DirEntry::metadata()` is cheap (single syscall on Linux via `d_type`).
- Skip: `.git`, `node_modules`, `/proc`, `/sys`, `/dev`, hidden files if configured.
- Emit `FileEntry` via `crossbeam_channel::Sender<FileEntry>` (unbounded during scan phase).
- Check `CancellationToken` at the start of each directory batch.
- **Capture `metadata.ino()` and `metadata.dev()`** — required for Stage 3 hard-link filter.

**Two-Phase Progress:**
- **Phase 1 (Enumerating):** `files_found` counter increments. Total unknown → UI shows spinner + count.
- **Phase 2 (Processing):** Total known → UI shows `n / total` percentage bar.

---

### Stage 2 (UPGRADED): Size Bucketing + Large-Group Chunking (`src/pipeline.rs`)

> **Fix for: Critical Improvement #4 — Memory Spike on Pathological Input**

**Input:** Stream of `FileEntry`
**Output:** Iterator of `Vec<FileEntry>` chunks (each ≤ `MAX_GROUP_SIZE`)

```rust
const MAX_GROUP_SIZE: usize = 10_000;

fn bucket_by_size(entries: Vec<FileEntry>) -> impl Iterator<Item = Vec<FileEntry>> {
    let mut map: HashMap<u64, Vec<FileEntry>> = HashMap::new();

    for entry in entries {
        map.entry(entry.size).or_default().push(entry);
    }

    map.into_values()
        .filter(|g| g.len() > 1)            // drop unique-size singletons
        .flat_map(|group| {
            // Chunk large groups to prevent memory spikes
            group.chunks(MAX_GROUP_SIZE)
                 .map(|c| c.to_vec())
                 .collect::<Vec<_>>()
        })
}
```

**Why `MAX_GROUP_SIZE = 10_000`?**
- Worst case: 1M files all same size → single group of 1M → OOM.
- Chunking bounds active memory per batch to ~`10_000 × sizeof(FileEntry)` ≈ 5 MB max.
- Also improves CPU cache locality during hashing.

**Expected reduction from singletons:** ~95% of files eliminated.

---

### Stage 3 (NEW): Hard-Link / Inode Pre-Filter (`src/pipeline.rs`)

> **Fix for: Critical Improvement #3 — Wasted Hashing of Hard-Linked Files**

```rust
fn filter_hard_links(entries: Vec<FileEntry>)
    -> (Vec<FileEntry>, Vec<HardLinkGroup>)
{
    let mut inode_map: HashMap<(u64, u64), Vec<FileEntry>> = HashMap::new();

    for entry in entries {
        inode_map.entry((entry.dev, entry.inode)).or_default().push(entry);
    }

    let mut unique = Vec::new();
    let mut hard_links = Vec::new();

    for (_, group) in inode_map {
        if group.len() == 1 {
            unique.push(group.into_iter().next().unwrap());
        } else {
            // Already the same file at OS level — no hashing needed
            hard_links.push(HardLinkGroup(group));
        }
    }

    (unique, hard_links)
}
```

- Hard-link groups are reported in results separately (as "already de-duplicated by OS").
- Only `unique` entries proceed to I/O scheduler and hashing.
- On macOS/Linux: `metadata.ino()` and `metadata.dev()` are available via `std::os::unix::fs::MetadataExt`.

---

### Stage 4 (FIXED): Partial Hashing (`src/hasher.rs`)

> **Fix for: Critical Improvement #2 — Hash Inconsistency for Small Files**

**Problem with original design:**
```
// BROKEN: for a 10KB file:
if size > 8192  → read middle  ✅
if size > 16384 → read end     ❌ SKIPPED — so no "last" probe for 8-16KB files
```

**Fixed consistent 3-region sampling:**

```rust
fn partial_hash(path: &Path, size: u64, token: &CancellationToken) -> Result<[u8; 32], FileError> {
    if token.is_cancelled() { return Err(FileError::Cancelled); }

    let chunk_size = (8192_u64).min(size) as usize;  // always bounded by file size
    let mut hasher = blake3::Hasher::new();

    // Region 1: Start
    read_chunk(path, 0, chunk_size, &mut hasher)?;

    // Region 2: Middle (offset so it doesn't overlap Region 1 for tiny files)
    let mid = (size / 2).saturating_sub(chunk_size as u64 / 2);
    read_chunk(path, mid, chunk_size, &mut hasher)?;

    // Region 3: End
    let end = size.saturating_sub(chunk_size as u64);
    read_chunk(path, end, chunk_size, &mut hasher)?;

    Ok(hasher.finalize().into())
}
```

**Properties:**
- For a 100-byte file: `chunk_size = 100`. All 3 reads are the entire file. That's fine — still consistent.
- For a 10KB file: `chunk_size = 8192`. 3 reads at 0, 1KB, 2KB offsets.
- For a 10GB file: `chunk_size = 8192`. Max 24KB I/O regardless of size.
- **Every file gets exactly 3 probes.** No conditional branching on file size.

---

### Stage 5: Full Hashing (`src/hasher.rs`)

**Triggered only for partial-hash collision survivors (~1–5% of files).**

```rust
fn full_hash(path: &Path, token: &CancellationToken) -> Result<[u8; 32], FileError> {
    let file = File::open(path).map_err(FileError::Io)?;

    // Use mmap for files > 64MB (avoids repeated syscall overhead)
    if file.metadata()?.len() > 64 * 1024 * 1024 {
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        let hash = blake3::hash(&mmap);
        return Ok(hash.into());
    }

    let mut hasher = blake3::Hasher::new();
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut buf = [0u8; 256 * 1024];
    loop {
        if token.is_cancelled() { return Err(FileError::Cancelled); }   // inner-loop check
        let n = reader.read(&mut buf).map_err(FileError::Io)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().into())
}
```

**Key differences from v1:**
- Cancellation checked **inside the read loop** — not just at stage boundaries.
- `memmap2` for large files (> 64MB) — OS manages page cache, avoids double-copy.
- I/O already sorted by parent dir via Stage 0 scheduler.

---

### Stage 6 (NEW): Byte-Level Collision Verification (`src/verifier.rs`)

> **Fix for: Critical Improvement #9 — Hash Collision Safety**

Applied only to confirmed duplicate groups (after full hashing). The set is tiny (~1–5% of already-tiny collision group).

```rust
pub fn verify_identical(a: &Path, b: &Path) -> Result<bool, FileError> {
    let fa = File::open(a).map_err(FileError::Io)?;
    let fb = File::open(b).map_err(FileError::Io)?;

    let mut ra = BufReader::with_capacity(64 * 1024, fa);
    let mut rb = BufReader::with_capacity(64 * 1024, fb);

    let mut buf_a = [0u8; 64 * 1024];
    let mut buf_b = [0u8; 64 * 1024];

    loop {
        let na = ra.read(&mut buf_a)?;
        let nb = rb.read(&mut buf_b)?;
        if na != nb || buf_a[..na] != buf_b[..nb] {
            return Ok(false);  // Not actually identical — discard from group
        }
        if na == 0 { break; }
    }
    Ok(true)
}
```

- Pairs within each `full_hash` group are verified before being reported to the user.
- Cost is negligible — only runs on confirmed hash collisions, which is an astronomically small set.
- Prevents false-positive deletions caused by BLAKE3 collision (probability ≈ 2⁻²⁵⁶, but non-zero).

---

### Stage 7 (UPGRADED): Duplicate Resolution (`src/resolver.rs`)

> **Fix for: Critical Improvement #7 — Smarter Keep Policy**

```rust
pub struct DuplicateGroup {
    pub hash:              [u8; 32],
    pub files:             Vec<FileEntry>,
    pub total_wasted_bytes: u64,
    pub keep_index:        usize,     // index of file to keep
}

pub enum KeepPolicy {
    Oldest,              // Keep by earliest mtime
    ShortestPath,        // Keep shortest path string
    SmartScore,          // NEW: composite scoring (default)
    Manual,              // User picks per-group in UI
}

pub enum ActionMode {
    DryRun,              // Report only — no disk changes       [NEW]
    Delete,              // Delete duplicates                   [NEW]
    HardLink,            // Replace duplicates with hard links  [NEW]
}
```

**Smart Scoring Algorithm (`SmartScore`):**

```rust
fn score(entry: &FileEntry, preferred_dirs: &[PathBuf]) -> i64 {
    let mut score: i64 = 0;

    // Prefer shallower paths (fewer "/" separations)
    let depth = entry.path.components().count() as i64;
    score -= depth * 10;

    // Prefer recently accessed files (atime via metadata)
    // Higher atime = more recent = more likely "the real one"
    if let Ok(atime) = entry.atime_secs() {
        score += atime / 1_000_000;  // normalised
    }

    // Boost paths under preferred directories
    for preferred in preferred_dirs {
        if entry.path.starts_with(preferred) {
            score += 1000;
        }
    }

    // Penalise paths under Downloads, Trash, Temp
    for junk in &["/Downloads", "/Trash", "/.Trash", "/tmp", "/Temp"] {
        if entry.path.to_string_lossy().contains(junk) {
            score -= 500;
        }
    }

    score
}
```

- File with **highest score** is marked as `keep_index`.
- Preferred directories configurable from UI scan panel.
- All scoring is deterministic and re-computable — no side effects.

---

## 5. Concurrency Model

```
┌──────────────────────────────────────────────────────────────────┐
│  UI Thread (egui)                                                │
│  - Reads Arc<Mutex<ScanProgress>> every frame                   │
│  - Calls token.cancel() on button click                         │
│  - Reads Arc<Mutex<Option<ScanResult>>> once pipeline finishes  │
└──────────────┬────────────────────────────────────────────────────┘
               │  std::thread::spawn
               ▼
┌──────────────────────────────────────────────────────────────────┐
│  Pipeline Coordinator Thread                                     │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  Scanner (BFS, 1 thread, async dir reads)               │    │
│  └────────────────────────┬────────────────────────────────┘    │
│                            │ crossbeam::bounded(unbounded scan)  │
│  ┌─────────────────────────▼──────────────────────────────┐     │
│  │  I/O Scheduler (sorts by dir, bounded channel)         │     │
│  │  Reader Threads ×3 (plain OS threads, NOT Rayon)       │     │
│  └────────────────────────┬────────────────────────────────┘    │
│                            │ crossbeam::bounded(256) ← backpressure│
│  ┌─────────────────────────▼──────────────────────────────┐     │
│  │  Rayon Hash Pool (CPU-bound, num_cpus threads)         │     │
│  └────────────────────────┬────────────────────────────────┘    │
│                            │                                     │
│  ┌─────────────────────────▼──────────────────────────────┐     │
│  │  Resolver (single thread, fast)                        │     │
│  └──────────────────────────────────────────────────────── ┘    │
└──────────────────────────────────────────────────────────────────┘
```

**Why separate I/O threads from Rayon?**
Rayon is a work-stealing CPU pool — it has no concept of disk layout. Isolating I/O to 2–4 dedicated reader threads allows sequential, locality-aware reads. Rayon handles only in-memory CPU hashing.

**Backpressure discipline:**
- `crossbeam::bounded(512)` on the job queue → reader threads stall if CPU can't keep up.
- `crossbeam::bounded(256)` on buffer channel → readers stall if hash workers are saturated.
- This prevents unbounded RAM growth during hashing large file sets.

---

## 6. Cancellation System (`src/cancel.rs`)

> **Upgraded fix for: Critical Improvement #5 — Weak Rayon Cancellation**

```rust
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self { Self(Arc::new(AtomicBool::new(false))) }
    pub fn cancel(&self) { self.0.store(true, Ordering::Relaxed); }
    pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::Relaxed) }
    pub fn clone_token(&self) -> Self { Self(Arc::clone(&self.0)) }
}
```

**Cancellation check points (exhaustive):**

| Location | Check Granularity |
|---|---|
| `scanner.rs` — BFS loop | Per directory batch |
| `pipeline.rs` — Stage 2 bucketing | Per size-group chunk |
| `io_scheduler.rs` — reader loop | Per IOJob |
| `hasher.rs` — `partial_hash()` | Before each file |
| `hasher.rs` — `full_hash()` read loop | Every 256KB chunk |
| `verifier.rs` — byte comparison | Per file pair |
| `resolver.rs` — scoring loop | Per group |

**Explicit check inside Rayon (critical):**
```rust
// ✅ CORRECT — must check inside the closure, not just at stage entry
groups.par_iter().for_each(|file| {
    if token.is_cancelled() {
        return;   // Rayon sees early return; other threads finish their current item
    }
    process(file);
});
```

> Rayon doesn't support `break`-style early exit across workers. Each worker must check the token and `return` independently. The pipeline coordinator collects whatever was successfully completed before cancellation.

**On cancellation — guaranteed behaviour:**
1. Pipeline stops accepting new work within one check interval.
2. Work already in-flight completes its current unit (one file / one chunk).
3. All duplicate groups found so far are flushed to `ScanResult`.
4. `ScanResult::partial = true` is set.
5. UI transitions from `Scanning` → `Results` with a `"⚠️ Partial results — scan was cancelled"` banner.

---

## 7. Memory Strategy

**Principle:** Process per size-group chunk, discard, move on. Never accumulate files not needed downstream.

```
Active memory at any time (worst case):
  ┌──────────────────────────────────────────────────┐
  │  Current chunk being processed:                  │
  │    MAX_GROUP_SIZE × FileEntry ≈ 5 MB             │
  │                                                  │
  │  I/O buffers in transit:                         │
  │    IO_CHANNEL_CAP × 24KB ≈ 12 MB (partial phase)│
  │    BUF_CHANNEL_CAP × 256KB ≈ 64 MB (full phase) │
  │                                                  │
  │  Final results:                                  │
  │    Vec<DuplicateGroup> — small (actual dupes)    │
  └──────────────────────────────────────────────────┘

NOT in memory (guaranteed):
  - Unique-size files (dropped after bucketing)
  - Non-colliding partial-hash groups (dropped after Stage 4)
  - Raw file contents (streaming only, no accumulation)
  - All FileEntries for non-duplicate size groups
```

**Memory ceiling target:** < 150 MB for 1M file scan (down from 512 MB in v1, bounded by channel caps).

**HashMap key type:** `[u8; 32]` — no `String`, no heap alloc per key.

---

## 8. Error Handling Strategy (`src/errors.rs`)

> **Fix for: Critical Improvement #6 — Missing Error Strategy**

**Rule: The pipeline never panics. Every file error is logged, skipped, and the scan continues.**

```rust
#[derive(Debug)]
pub enum FileError {
    Io(std::io::Error),         // Read failure, permission denied
    NotFound(PathBuf),          // File deleted mid-scan
    TooLarge(u64),              // File exceeds configured limit
    Cancelled,                  // Scan was cancelled
    VerificationFailed,         // Byte-level check failed after hash match (log + skip)
}

// Per-file result used throughout the pipeline
pub type FileResult<T> = Result<T, FileError>;
```

**Pipeline behaviour on errors:**

| Error Type | Action | User Visible? |
|---|---|---|
| `Io(Permission denied)` | Skip file, log | Yes — error count in status bar |
| `Io(File not found)` | Skip file, log | Yes |
| `NotFound` (deleted mid-scan) | Skip file, log | Yes |
| `TooLarge` | Skip file | Optional (configurable) |
| `Cancelled` | Flush results, stop | Yes — partial results banner |
| `VerificationFailed` | Remove from group, log | Debug log only |

**Error accumulation:**
```rust
pub struct ScanStats {
    pub files_scanned:       u64,
    pub files_skipped_error: u64,
    pub error_log:           Vec<(PathBuf, FileError)>,  // capped at 1000 entries
    pub duplicate_groups:    u64,
    pub total_wasted_bytes:  u64,
    pub partial:             bool,    // true if scan was cancelled
}
```

---

## 9. Action Mode System (`src/resolver.rs`)

> **Fix for: Critical Improvement #8 — Missing Dry Run vs Action Mode**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ActionMode {
    DryRun,    // Default. Report findings. No disk changes.
    Delete,    // Permanently delete duplicate files (to trash on macOS).
    HardLink,  // Replace duplicates with hard links to the kept file.
               //   → Saves space while preserving all paths
               //   → All paths remain accessible
}
```

**UI placement:** Drop-down in `scan_panel.rs` before starting scan. Default: `DryRun`.

**Execution logic (post-resolution):**

```rust
fn apply_action(group: &DuplicateGroup, mode: &ActionMode) -> Vec<FileResult<()>> {
    let keep_path = &group.files[group.keep_index].path;

    group.files.iter().enumerate()
        .filter(|(i, _)| *i != group.keep_index)
        .map(|(_, entry)| match mode {
            ActionMode::DryRun  => Ok(()),                               // no-op
            ActionMode::Delete  => trash::delete(&entry.path)           // macOS Trash API
                                     .map_err(|e| FileError::Io(e.into())),
            ActionMode::HardLink => {
                std::fs::remove_file(&entry.path)?;                     // remove duplicate
                std::fs::hard_link(keep_path, &entry.path)?;            // relink it
                Ok(())
            }
        })
        .collect()
}
```

**HardLink mode notes:**
- Only works within the same filesystem (`dev` ID must match).
- On macOS, `std::fs::hard_link` is available via the standard library.
- Cross-device duplicates fall back to `DryRun` + warning.

---

## 10. UI Layer Design

### Panel Routing (`app.rs`)

```rust
pub enum AppView {
    Welcome,     // Launch screen
    Scan,        // Folder picker, config (skip hidden, min size, ActionMode, preferred dirs)
    Scanning,    // 2-phase progress display + Cancel button
    Results,     // Duplicate groups, stats, export, partial-result banner
}
```

### `ui/scan_panel.rs` — Configuration Panel

- Multi-folder selection via `rfd::FileDialog::pick_folders()`.
- Toggles: skip hidden files, skip system dirs, min file size slider.
- **ActionMode selector:** DryRun | Delete | HardLink (with warning label for non-DryRun).
- Preferred directories input (for SmartScore keep policy).
- "Start Scan →" button → spawns background pipeline thread.

### `ui/progress_panel.rs` — Two-Phase Progress Display

> **Fix for: Critical Improvement #10 — Progress Accuracy During Streaming Enumeration**

```
// Phase 1: Enumeration (total unknown)
Stage: Scanning directories...
⠋ 48,291 files found so far
Current path: /Users/rishi/Documents/Projects/...

// Phase 2: Processing (total known)
Stage: Computing partial hashes (Stage 2/4)
[==============>        ] 67% (32,152 / 48,291 files)
⚡ 1,247 potential duplicate groups

                         [ Cancel ]
```

**Progress state machine:**
```rust
pub enum ProgressPhase {
    Enumerating { files_found: u64, current_path: PathBuf },
    Processing  { stage: u8, files_done: u64, files_total: u64 },
    Finishing,
}
```

- `ctx.request_repaint_after(Duration::from_millis(100))` polled from pipeline via shared Arc.
- Phase 1 → Phase 2 transition happens when scanner finishes and total is known.

### `ui/results_panel.rs` — Results Display

```
⚠️ Partial results — scan was cancelled after 32,152/48,291 files.

┌─ Duplicate Group #1 ─────────────────────────────────────────────────┐
│  Hash: 3a7f...  │  3 files  │  Wasted: 148 MB  │  Score: 94.2       │
│                                                                       │
│  ✅ KEEP    /Users/rishi/Photos/vacation.jpg   [oldest, 2.3 MB]      │
│  🗑 DELETE  /Users/rishi/Downloads/vacation.jpg [score: 41.0]        │
│  🗑 DELETE  /Volumes/Backup/vacation.jpg        [junk dir penalty]   │
└───────────────────────────────────────────────────────────────────────┘

Summary: 12 groups | 31 duplicates | 2.1 GB reclaimable | 3 errors (view log)

[ Apply Actions ]   [ Export JSON Report ]   [ View Error Log ]
```

- Partial-result banner shown when `ScanResult::partial == true`.
- Per-file toggle: change keep/delete decision interactively.
- "Apply Actions" button: disabled in DryRun mode.
- "Export JSON Report" → `serde_json::to_writer_pretty`.
- "View Error Log" → modal with scrollable error list.

---

## 11. File-by-File Implementation Checklist

### Phase 1 — Foundation & Types
- [ ] `src/types.rs` — `FileEntry` (with `inode`, `dev`), `ScanConfig`, `ScanProgress` (2-phase), `ScanResult`, `DuplicateGroup`, `ScanStats`, `HardLinkGroup`.
- [ ] `src/errors.rs` — `FileError` enum, `FileResult<T>` alias.
- [ ] `src/cancel.rs` — `CancellationToken` (Arc<AtomicBool>).
- [ ] Update `Cargo.toml` — all dependencies from §2.

### Phase 2 — Pipeline Core
- [ ] `src/scanner.rs` — BFS enumerator, inode/dev capture, cancellation, stream to channel.
- [ ] `src/io_scheduler.rs` — `IOJob`, sorted job queue, reader thread pool, bounded backpressure channels.
- [ ] `src/hasher.rs` — Fixed consistent `partial_hash()`, cancellation-aware `full_hash()` with mmap.
- [ ] `src/verifier.rs` — `verify_identical()` byte-by-byte comparator.
- [ ] `src/pipeline.rs` — Stage orchestration: inode filter → size bucketing (chunked) → I/O scheduler → partial → full → verify → resolve.
- [ ] `src/resolver.rs` — Smart scoring, `DuplicateGroup` builder, `ActionMode` executor, `apply_action()`.

### Phase 3 — UI Panels
- [ ] `src/ui/scan_panel.rs` — Folder picker (`rfd`), config, `ActionMode` selector, preferred dirs input.
- [ ] `src/ui/progress_panel.rs` — `ProgressPhase::Enumerating` + `Processing` display, Cancel button.
- [ ] `src/ui/results_panel.rs` — Group cards, partial-result banner, per-file toggle, Apply/Export/ErrorLog buttons.
- [ ] `src/ui/mod.rs` — Re-export panels.

### Phase 4 — Integration
- [ ] `src/app.rs` — `AppView` enum, state wiring, `Arc<Mutex<...>>` plumbing, panel routing.
- [ ] `src/main.rs` — Window config, dark mode, custom fonts.

### Phase 5 — Polish & Advanced
- [ ] macOS "Show in Finder" integration (`open -R <path>`).
- [ ] JSON report export with full `ScanStats`.
- [ ] Error log modal in UI.
- [ ] Cross-device HardLink fallback warning.
- [ ] Advanced Mode: CDC / Rabin fingerprinting (`cdchunking`) — feature-flagged.
- [ ] Advanced Mode: LSH similarity detection (see §14) — feature-flagged.

---

## 12. Complexity & Performance Targets

| Stage | Complexity | Target (1M files) | Notes |
|---|---|---|---|
| Scan / Enumerate | O(n) | < 5 seconds | BFS, metadata pre-fetch |
| Inode Filter | O(n) | < 50ms | HashMap on (dev, ino) |
| Size Bucketing | O(n) | < 100ms | HashMap, chunked output |
| Partial Hash | O(k), k≪n | < 15 seconds | 24KB max I/O, locality-sorted |
| Full Hash | O(m), m≪k | < 45 seconds | mmap for large, locality-sorted |
| Byte Verify | O(dupes) | < 5 seconds | Tiny set — only actual dupes |
| Resolution | O(m log m) | < 1 second | Scoring + sort |
| **Total** | **≈ O(n)** | **< 75 seconds** | vs 2 min in v1 |

**Memory ceiling:** < 150 MB for 1M file scan (bounded by channel capacities).
**Cancellation latency:** < 200ms from click to pipeline full-stop.
**HDD vs SSD:** I/O scheduler provides 2–5× speedup on rotational drives.

---

## 13. Critical Improvements Index

A full cross-reference of all improvements integrated into this plan:

| # | Problem | Fix Applied | Location in Plan |
|---|---|---|---|
| 1 | Disk thrashing from `rayon::par_iter()` on I/O | I/O Scheduler Layer with dedicated reader threads + bounded backpressure channels | §4 Stage 0, §5 |
| 2 | Partial hash inconsistency for small files | Fixed 3-region sampling: `chunk_size = min(8192, size)` for all probes | §4 Stage 4 |
| 3 | Wasted hashing of hard-linked files | Inode/device pre-filter before any hashing | §4 Stage 3 |
| 4 | OOM on pathological input (1M same-size files) | `MAX_GROUP_SIZE = 10_000` chunking of large size-groups | §4 Stage 2 |
| 5 | Weak cancellation inside Rayon | Explicit `if token.is_cancelled() { return; }` inside every `par_iter` closure + inner read loop | §6 |
| 6 | No error handling strategy | `FileError` enum, `FileResult<T>`, skip-and-log for every failure mode | §8 |
| 7 | Too-simple keep policy | Smart scoring: path depth + atime + preferred dirs + junk dir penalty | §4 Stage 7 |
| 8 | No dry-run vs action modes | `ActionMode` enum: `DryRun` \| `Delete` \| `HardLink` | §9 |
| 9 | No hash collision protection | Byte-level `verify_identical()` post-hashing, only on confirmed duplicates | §4 Stage 6 |
| 10 | Progress bar inaccuracy during streaming scan | 2-phase `ProgressPhase::Enumerating` + `Processing` model | §10 UI |

---

## 14. Advanced Upgrade: Similarity Detection via LSH

> **Feature-flagged (behind `--features similarity`). Does not affect core pipeline performance.**

### What LSH Detects (beyond the core pipeline)

| Detection Type | Core Pipeline | LSH Advanced Mode |
|---|---|---|
| Exact byte-for-byte duplicates | ✅ | ✅ |
| Hard-linked files | ✅ | ✅ |
| Renamed duplicates | ✅ (same content → same hash) | ✅ |
| Slightly modified files | ❌ | ✅ |
| Near-duplicate images | ❌ | ✅ |
| Log files with appended lines | ❌ | ✅ |
| Shifted content (same data, different offset) | ❌ | ✅ (with CDC) |

### LSH Design

**Locality-Sensitive Hashing** maps similar inputs to the same hash bucket with high probability, while dissimilar inputs land in different buckets.

```rust
// Step 1: Generate MinHash signature for each file
fn minhash_signature(path: &Path, num_hashes: usize) -> Vec<u64> {
    // Read file in fixed-size shingles (e.g., 4-byte rolling windows)
    // Apply `num_hashes` independent hash functions
    // Take the minimum of each → signature of length `num_hashes`
}

// Step 2: Band the signature into b bands of r rows each
// Files sharing ≥1 band → candidate similar pair
// Tune: b=20, r=5 → ~80% similarity threshold

// Step 3: Output: Vec<SimilarityGroup { files, estimated_similarity_pct }>
```

**Parameters:**
```rust
const NUM_HASHES: usize = 100;  // signature length
const BANDS: usize = 20;        // controls threshold
const ROWS_PER_BAND: usize = 5; // b × r = NUM_HASHES
// Approximate threshold: (1/b)^(1/r) ≈ 79% similarity
```

**Workflow integration:**

```
After core pipeline completes:
           │
           ▼ (only if --features similarity)
┌────────────────────────────┐
│  LSH Fingerprinter          │  Reads each unique file once (shingles)
│  (src/lsh.rs)              │  O(n × shingle_read_cost)
└──────────┬─────────────────┘
           │
           ▼
┌────────────────────────────┐
│  Band Bucketer              │  Groups candidate similar pairs
└──────────┬─────────────────┘
           │
           ▼
┌────────────────────────────┐
│  Similarity Verifier        │  Jaccard similarity for candidates
│  (optional: byte diff)      │  Filters false positives
└──────────┬─────────────────┘
           │
           ▼
  results_panel.rs → "Similar Files" section (separate from exact dupes)
```

**UI section for LSH results:**
```
┌─ Similar Files (Not Exact Duplicates) ─────────────────────────┐
│  ~83% similar:                                                  │
│  📄 /Users/rishi/report_v1.docx   (14.2 MB)                   │
│  📄 /Users/rishi/report_v2.docx   (14.7 MB)                   │
│  [View Diff]  [Keep Both]  [Delete Older]                      │
└─────────────────────────────────────────────────────────────────┘
```

**Crate options:**
- `lsh-rs = "0.9"` — ready-to-use Rust LSH library.
- Or implement MinHash + band scheme manually (~150 LOC) for full control.

---

## 15. Key Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Hash algorithm | BLAKE3 | Fastest modern hash, hardware-accelerated, 256-bit output |
| Partial sampling | Fixed 3-region, `chunk_size = min(8192, size / 4).max(1)` | Consistent for all file sizes, better entropy for mid-sized files |
| I/O model | Dedicated reader threads (NOT Rayon) | Rayon is CPU-only; dedicated threads allow locality-aware sequential reads |
| Backpressure | `crossbeam::bounded` channels | Prevents memory blowup when I/O outpaces CPU or vice-versa |
| Cancellation | `Arc<AtomicBool>` checked at every loop | Lock-free, checked inside Rayon closures, inner read loops |
| Partial results on cancel | Always preserved | Core user requirement — no work is wasted |
| File identity | `FileId { volume_id, file_index }` (cross-platform) | Abstracts Unix `(dev, ino)` and Windows `BY_HANDLE_FILE_INFORMATION` |
| Hard-link detection | `FileId` pre-filter | Skip hashing entirely for OS-level duplicates on all platforms |
| mmap safety | Fallback to buffered read on mmap failure | Guards against network drives, sparse files, corrupted storage |
| HardLink atomicity | temp-link + atomic rename | Prevents data loss if process crashes mid-operation |
| Byte verification | O(N) against canonical file only | Avoids O(N²) pair comparison; one file is the reference |
| BFS scanner | Multi-threaded worker pool (2–4) | 2–3× faster enumeration on large/network trees |
| Large group protection | `MAX_GROUP_SIZE = 10_000` chunking | Prevents OOM on pathological equal-size datasets |
| Hash collision guard | Byte-level `verify_identical()` | BLAKE3 collision probability ≈ 2⁻²⁵⁶ but non-zero; eliminates false positives |
| Hash cache | `(path, size, mtime) → hash` persisted | Turns 75s scan into ~5s incremental scan on re-run |
| Keep policy | SmartScore (depth + atime + dir bias) | More accurate than Oldest/Shortest alone; fully deterministic |
| Action modes | DryRun / Delete / HardLink | Covers all real-world use cases; DryRun is safe default |
| Error handling | Skip-and-log, never panic | Scan continues through permission errors and mid-scan deletions |
| Progress model | Byte-based (bytes_processed / total_bytes) | Accurate for large files; file-count is misleading when sizes vary wildly |
| UI framework | egui/eframe 0.28.1 | Pure Rust, immediate-mode, zero JS/HTML overhead |
| Similarity detection | LSH (feature-flagged) | Near-duplicate detection without polluting core pipeline performance |

---

## 16. Final 5% — Critical Correctness Fixes

These are not obvious at small scale but cause silent failures or data loss at production scale.

---

### Fix A: Cross-Platform File Identity (`FileId`)

> **Problem:** `(metadata.dev(), metadata.ino())` works on Unix/macOS but **silently produces wrong results on Windows** — `ino()` always returns `0` on Windows via the standard library. Hard-link detection becomes entirely broken without any error or warning.

**Solution:** Abstract file identity behind a platform-agnostic `FileId` struct.

```rust
// src/types.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileId {
    pub volume_id:  u64,   // Unix: dev_t   | Windows: dwVolumeSerialNumber
    pub file_index: u64,   // Unix: ino_t   | Windows: nFileIndexHigh << 32 | nFileIndexLow
}
```

**Platform implementations (`src/platform.rs`):**

```rust
#[cfg(unix)]
pub fn file_id(path: &Path) -> std::io::Result<FileId> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path)?;
    Ok(FileId {
        volume_id:  meta.dev(),
        file_index: meta.ino(),
    })
}

#[cfg(windows)]
pub fn file_id(path: &Path) -> std::io::Result<FileId> {
    use std::os::windows::io::AsRawHandle;
    use winapi::um::fileapi::GetFileInformationByHandle;
    use winapi::um::fileapi::BY_HANDLE_FILE_INFORMATION;

    let file = std::fs::File::open(path)?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
    if ok == 0 { return Err(std::io::Error::last_os_error()); }

    Ok(FileId {
        volume_id:  info.dwVolumeSerialNumber as u64,
        file_index: ((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64),
    })
}
```

**Impact:** Hard-link detection now works correctly on macOS, Linux, and Windows. No silent failure.

**New dependency (Windows only):**
```toml
[target.'cfg(windows)'.dependencies]
winapi = { version = "0.3", features = ["fileapi", "handleapi"] }
```

**Add to `FileEntry`:**
```rust
pub struct FileEntry {
    pub path:    PathBuf,
    pub size:    u64,
    pub mtime:   SystemTime,
    pub file_id: FileId,     // replaces raw (dev, inode) fields
}
```

---

### Fix B: mmap Safety Guard

> **Problem:** `memmap2::Mmap::map()` is `unsafe` and can cause **undefined behaviour or SIGBUS** for:
> - Network drives (NFS, SMB) where the file disappears mid-map
> - Sparse files (apparent size ≫ physical size)
> - Corrupted filesystem blocks
> - Files on removable media that is ejected mid-scan

**Solution:** Wrap every mmap usage in a fallback chain.

```rust
fn full_hash(path: &Path, size: u64, token: &CancellationToken)
    -> Result<[u8; 32], FileError>
{
    // Only attempt mmap if all safety conditions are met
    let use_mmap = size > 64 * 1024 * 1024
        && !is_network_path(path)      // heuristic: check mount type
        && !is_sparse_file(path, size);

    if use_mmap {
        match try_mmap_hash(path) {
            Ok(hash) => return Ok(hash),
            Err(e) => {
                // Log the mmap failure, fall through to buffered read
                log_warn("mmap failed for {}: {}. Falling back to buffered read.", path, e);
            }
        }
    }

    // Guaranteed-safe fallback: streaming buffered read
    buffered_hash(path, token)
}

fn is_sparse_file(path: &Path, apparent_size: u64) -> bool {
    #[cfg(unix)] {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path).unwrap_or_else(|_| return false /* safe */);
        // 512-byte blocks; if allocated < apparent → sparse
        (meta.blocks() * 512) < apparent_size
    }
    #[cfg(not(unix))] { false }  // conservative: assume not sparse on Windows
}
```

**Rule of thumb:**
- mmap = fast path for local, dense, large files.
- buffered read = universal fallback that is always safe.
- Never panic on mmap failure — log + continue.

---

### Fix C: Atomic HardLink Mode

> **Problem:** The current two-step HardLink implementation:
> ```rust
> std::fs::remove_file(&entry.path)?;   // step 1
> std::fs::hard_link(keep_path, &entry.path)?;  // step 2
> ```
> Is **not atomic**. If the process crashes, is OOM-killed, or the machine loses power between steps 1 and 2, the file at `entry.path` is **permanently lost with no recovery path**.

**Solution:** Use an atomic rename strategy — the only POSIX-guaranteed atomic filesystem operation.

```rust
fn atomic_hardlink(keep_path: &Path, target_path: &Path) -> Result<(), FileError> {
    // Step 1: Create a hard link at a temp path in the SAME directory
    //         (must be on same filesystem as target for rename to be atomic)
    let temp_path = target_path.with_extension(
        format!("sfm_tmp_{}", std::process::id())
    );

    // If this fails → nothing has changed → safe
    std::fs::hard_link(keep_path, &temp_path)
        .map_err(FileError::Io)?;

    // Step 2: Atomically replace target with temp (POSIX rename guarantee)
    //         If this fails → temp_path exists → target_path still intact → safe
    //         If this succeeds → atomic swap, old inode ref count decremented
    std::fs::rename(&temp_path, target_path)
        .map_err(|e| {
            // Clean up temp on failure (best-effort)
            let _ = std::fs::remove_file(&temp_path);
            FileError::Io(e)
        })?;

    // At this point: target_path is a hard link to keep_path
    // The original inode's ref count was decremented atomically by rename()
    Ok(())
}
```

**Safety guarantee table:**

| Crash point | Result |
|---|---|
| Before `hard_link` | Nothing changed. Both files intact. |
| Between `hard_link` and `rename` | Temp file exists. Target intact. Temp can be cleaned up on next run. |
| After `rename` completes | Atomic success. Hard link installed. |

**Temp file cleanup:** On scan startup, remove any `*.sfm_tmp_*` files found in scanned directories — they are abandoned temps from a previous interrupted HardLink operation.

---

### Fix D: O(N) Byte Verification

> **Problem:** The original verification plan "verify every pair" in a group of N files is O(N²) comparisons. For a group with 1,000 identical files, that's 499,500 comparisons instead of 999.

**Solution:** Designate `files[0]` as the canonical reference. Verify all others against it only.

```rust
pub fn verify_group(files: &[FileEntry]) -> Result<Vec<&FileEntry>, FileError> {
    if files.len() < 2 { return Ok(files.iter().collect()); }

    let canonical = &files[0];  // reference — verified against all others
    let mut confirmed = vec![canonical];

    for candidate in &files[1..] {
        match verify_identical(&canonical.path, &candidate.path) {
            Ok(true)  => confirmed.push(candidate),
            Ok(false) => {  // hash collision (astronomically rare)
                log_warn("Hash collision detected: {} vs {}",
                    canonical.path.display(), candidate.path.display());
                // candidate is NOT added to confirmed group — silently dropped
            },
            Err(e) => log_error("Verification error for {}: {}", candidate.path.display(), e),
        }
    }

    Ok(confirmed)
}
```

**Complexity:**
- **Before (all pairs):** O(N²) — catastrophic for groups with many identical copies.
- **After (against canonical):** O(N) — linear regardless of group size.

**Why `files[0]` is a safe canonical:** All files in the group share the same BLAKE3 full hash. If `files[0]` and `files[k]` differ, it's a collision that we correctly catch. There is no "wrong choice" of canonical.

---

### Fix E: Parallel BFS Directory Scanner

> **Problem:** Single-threaded BFS becomes a bottleneck on:
> - Trees with 100K+ directories
> - Network drives with high per-`opendir` latency (NFS, SMB)
> - Systems where `readdir` itself has I/O cost (spinning HDD, cold cache)

**Solution:** Parallel directory scanning with a shared work-stealing queue.

```rust
// src/scanner.rs

pub fn parallel_scan(
    roots: Vec<PathBuf>,
    config: &ScanConfig,
    token: &CancellationToken,
) -> Vec<FileEntry> {
    // Shared BFS queue — worker threads fight for directories
    let queue: Arc<Mutex<VecDeque<PathBuf>>> = Arc::new(Mutex::new(
        roots.into_iter().collect()
    ));

    // Shared output accumulator
    let results: Arc<Mutex<Vec<FileEntry>>> = Arc::new(Mutex::new(Vec::new()));

    // Active worker count — used to detect when all work is done
    let active: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

    const NUM_SCAN_THREADS: usize = 4;
    let handles: Vec<_> = (0..NUM_SCAN_THREADS).map(|_| {
        let queue   = Arc::clone(&queue);
        let results = Arc::clone(&results);
        let active  = Arc::clone(&active);
        let token   = token.clone_token();
        let config  = config.clone();

        std::thread::spawn(move || {
            loop {
                if token.is_cancelled() { break; }

                let dir = {
                    let mut q = queue.lock().unwrap();
                    q.pop_front()
                };

                match dir {
                    None => {
                        // No work right now — check if any thread is still active
                        if active.load(Ordering::SeqCst) == 0 { break; }
                        std::thread::yield_now();  // briefly yield and retry
                        continue;
                    }
                    Some(path) => {
                        active.fetch_add(1, Ordering::SeqCst);

                        if let Ok(read_dir) = std::fs::read_dir(&path) {
                            let mut local_files = Vec::new();
                            let mut subdirs = Vec::new();

                            for entry in read_dir.flatten() {
                                if token.is_cancelled() { break; }
                                let Ok(meta) = entry.metadata() else { continue; };

                                if meta.is_dir() {
                                    if !should_skip_dir(&entry.path(), &config) {
                                        subdirs.push(entry.path());
                                    }
                                } else if meta.is_file() && meta.len() >= config.min_file_size {
                                    local_files.push(make_entry(&entry.path(), &meta));
                                }
                            }

                            // Batch-push subdirs into shared queue
                            queue.lock().unwrap().extend(subdirs);
                            // Batch-push file entries
                            results.lock().unwrap().extend(local_files);
                        }

                        active.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            }
        })
    }).collect();

    for h in handles { h.join().ok(); }

    Arc::try_unwrap(results).unwrap().into_inner().unwrap()
}
```

**Expected gain:** 2–3× faster enumeration on large directory trees. On network drives, the gain is even larger because per-directory latency dominates.

**Tuning:** `NUM_SCAN_THREADS = 4` is the sweet spot. Beyond 4, contention on the shared queue mutex starts to dominate.

---

## 17. Senior-Level Polish Enhancements

These are the features that separate "it works" from "it's fast every time".

---

### Enhancement 1: Hash Cache (Huge Real-World Win)

> **Impact:** Turns a 75-second full scan into a ~5-second incremental scan on re-run.

**Principle:** If a file's `(path, size, mtime)` triple is unchanged since the last scan, its hash is unchanged. Skip re-hashing it entirely.

```rust
// src/cache.rs

#[derive(Serialize, Deserialize)]
pub struct CacheEntry {
    pub size:         u64,
    pub mtime_secs:   u64,
    pub partial_hash: [u8; 32],
    pub full_hash:    Option<[u8; 32]>,
}

pub struct HashCache {
    // Key: canonical path string
    map: HashMap<String, CacheEntry>,
    dirty: bool,   // true if any new entries were written this run
}

impl HashCache {
    // Load from disk on startup (~1ms for 1M entries)
    pub fn load() -> Self { /* deserialize from ~/.config/sfm/cache.json */ }

    // Persist to disk after scan (~50ms for 1M entries)
    pub fn save(&self) { /* serialize if dirty */ }

    pub fn get(&self, path: &Path, size: u64, mtime_secs: u64)
        -> Option<&CacheEntry>
    {
        self.map.get(path.to_str()?)
            .filter(|e| e.size == size && e.mtime_secs == mtime_secs)
    }

    pub fn insert(&mut self, path: &Path, entry: CacheEntry) {
        self.map.insert(path.to_str().unwrap().to_owned(), entry);
        self.dirty = true;
    }
}
```

**Integration in `hasher.rs`:**
```rust
fn partial_hash_cached(
    entry: &FileEntry,
    cache: &mut HashCache,
    token: &CancellationToken,
) -> Result<[u8; 32], FileError> {
    let mtime_secs = entry.mtime
        .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

    // Cache hit → free
    if let Some(cached) = cache.get(&entry.path, entry.size, mtime_secs) {
        return Ok(cached.partial_hash);
    }

    // Cache miss → compute and store
    let hash = partial_hash(&entry.path, entry.size, token)?;
    cache.insert(&entry.path, CacheEntry {
        size: entry.size,
        mtime_secs,
        partial_hash: hash,
        full_hash: None,
    });
    Ok(hash)
}
```

**Cache file location:** `~/.config/smart-file-manager/hash_cache.json` (or OS-appropriate config dir via `dirs` crate).
**Cache invalidation:** Automatic — mismatched `size` or `mtime_secs` triggers recompute.

---

### Enhancement 2: File-Type Heuristics (Skip-Fast Rules)

> **Impact:** Eliminates hashing overhead for categories of files that are nearly always unique.

```rust
// src/heuristics.rs

pub enum FileDisposition {
    SkipEntirelyUnique,    // Don't even enter the size bucket
    CompareDirectly,       // Too small to benefit from hashing — byte-compare
    HashNormally,          // Standard pipeline
}

pub fn classify(entry: &FileEntry) -> FileDisposition {
    // Rule 1: Tiny files (<= 1KB) — faster to byte-compare directly
    if entry.size <= 1024 {
        return FileDisposition::CompareDirectly;
    }

    // Rule 2: Known-unique file types (almost always unique by nature)
    if let Some(ext) = entry.path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            // Log files with embedded timestamps → unique by definition
            // (skip unless user explicitly enables log dedup)
            "log" | "trace" => return FileDisposition::SkipEntirelyUnique,

            // Lock files — ephemeral, should never be deduped
            "lock" | "pid"  => return FileDisposition::SkipEntirelyUnique,

            _ => {}
        }
    }

    FileDisposition::HashNormally
}
```

**Pipeline integration:** Called immediately after `FileEntry` is constructed in the scanner. `SkipEntirelyUnique` files never enter the size bucket.

---

### Enhancement 3: Adaptive Chunk Size for Partial Hashing

> **Impact:** Better entropy capture for mid-sized files (1KB–1MB), reducing partial-hash collisions and therefore reducing the number of files that reach Stage 5 (full hash).

**Fixed 8KB** is optimal for large files (say, 10GB) but **too coarse** for a 16KB file where 8KB probes overlap significantly.

```rust
fn compute_chunk_size(file_size: u64) -> usize {
    // Probe each region at 1/4 of file size, capped at 8192
    // For a 4KB file: chunk = 1KB — captures 75% of each region
    // For a 1MB file: chunk = 8KB — standard
    // For a 10GB file: chunk = 8KB — standard (doesn't scale up)
    const MAX_CHUNK: u64 = 8192;
    (file_size / 4).min(MAX_CHUNK).max(1) as usize
}
```

| File Size | Chunk Size | Bytes Read | Coverage per Region |
|---|---|---|---|
| 512 B | 128 B | 384 B | 25% |
| 4 KB | 1 KB | 3 KB | 25% |
| 32 KB | 8 KB | 24 KB | 25% |
| 1 MB | 8 KB | 24 KB | 0.78% |
| 10 GB | 8 KB | 24 KB | <0.001% |

**Result:** Consistent 25% coverage for small files. Fixed 24KB cap for large files. Fewer false partial-hash collisions across the board.

---

### Enhancement 4: Byte-Based Progress Accuracy

> **Impact:** Progress bar reflects actual work done, not number of files (which is misleading when file sizes vary by orders of magnitude).

**Problem with file-count progress:**
```
Processing: 50,000 / 100,000 files (50%)   ← looks like 50% done
← But those 50,000 files are all tiny text files.
← The remaining 50,000 are 4GB video files.
← Actual work done: ~2%
```

**Solution: Track bytes, not files.**

```rust
pub struct ScanProgress {
    // Phase 1: Enumeration
    pub phase:           ProgressPhase,

    // Phase 2: Hashing — byte-based
    pub bytes_processed: u64,
    pub bytes_total:     u64,    // sum of sizes of all files entering the hash stages

    // Supplementary counters (for the detail line)
    pub files_hashed:    u64,
    pub groups_found:    u64,
    pub current_path:    PathBuf,
}

// UI display
let pct = if progress.bytes_total > 0 {
    (progress.bytes_processed as f32 / progress.bytes_total as f32) * 100.0
} else { 0.0 };

// "Stage 3/4  [=======>   ] 72%  (1.4 GB / 1.9 GB)  — 312 groups found"
```

**`bytes_total`** is computed at the end of Stage 2 (size bucketing) — the sum of sizes of all files that survived the size-singleton filter. This is the correct denominator: files with unique sizes are already eliminated and contribute no hashing work.

---

### Enhancement 5: NUMA-Aware Thread Pinning (High-Core Servers)

> **Applicability:** Primarily relevant for servers with 32+ cores across multiple NUMA nodes. Desktop machines can ignore this.

**Problem:** On a 2-socket server, a Rayon thread on socket 0 hashing a buffer that was read by an I/O thread pinned to socket 1 causes cross-NUMA memory traffic — effectively halving memory bandwidth.

**Solution:** Pin I/O reader threads and their matching Rayon hash workers to the same NUMA node.

```rust
// Feature-flagged: only compile on Linux with libnuma available
#[cfg(all(target_os = "linux", feature = "numa"))]
mod numa {
    pub fn pin_to_node(thread_idx: usize, num_nodes: usize) {
        let node = thread_idx % num_nodes;
        // Use nix::sched::sched_setaffinity or libnuma bindings
        // to pin the calling thread to CPUs on `node`
    }

    pub fn build_numa_aware_pool(num_nodes: usize) -> rayon::ThreadPool {
        rayon::ThreadPoolBuilder::new()
            .num_threads(num_cpus::get())
            .start_handler(move |idx| pin_to_node(idx, num_nodes))
            .build()
            .unwrap()
    }
}
```

**Dependency (Linux only):**
```toml
[target.'cfg(target_os = "linux")'.dependencies]
nix = { version = "0.27", features = ["sched"], optional = true }

[features]
numa = ["nix"]
```

**Expected gain on NUMA systems:** 1.5–2× throughput from eliminating cross-socket memory traffic during hashing.

---

## Updated Implementation Checklist Addendum (§16 & §17)

### Phase 6 — Cross-Platform & Safety (Final 5%)
- [ ] `src/platform.rs` — `file_id()` for Unix and Windows, `FileId` struct.
- [ ] `src/types.rs` — Replace `(inode, dev)` fields with `FileId` in `FileEntry`.
- [ ] `Cargo.toml` — Add `winapi` as Windows-only dependency.
- [ ] `src/hasher.rs` — `is_sparse_file()`, `try_mmap_hash()`, mmap fallback to `buffered_hash()`.
- [ ] `src/resolver.rs` — Replace `remove_file + hard_link` with `atomic_hardlink()`.
- [ ] `src/resolver.rs` — Startup cleanup: remove `*.sfm_tmp_*` orphaned temp files.
- [ ] `src/verifier.rs` — Refactor to `verify_group()` (O(N) canonical-vs-all strategy).
- [ ] `src/scanner.rs` — Refactor single-threaded BFS to `parallel_scan()` with worker pool.

### Phase 7 — Senior Polish
- [ ] `src/cache.rs` — `HashCache` struct: load/save JSON, get/insert with mtime validation.
- [ ] `src/hasher.rs` — `partial_hash_cached()` / `full_hash_cached()` wrappers.
- [ ] `Cargo.toml` — Add `dirs = "5"` for OS config dir detection, `serde`/`serde_json` (already present).
- [ ] `src/heuristics.rs` — `FileDisposition` classifier, plug into scanner output.
- [ ] `src/hasher.rs` — Replace `8192` magic constant with `compute_chunk_size(file_size)`.
- [ ] `src/types.rs` — Update `ScanProgress` with `bytes_processed` / `bytes_total` fields.
- [ ] `src/ui/progress_panel.rs` — Update to render byte-based progress bar.
- [ ] `src/platform.rs` — `build_numa_aware_pool()` behind `#[cfg(feature = "numa")]`.

---

## 18. Folder-Selection Mode Adjustments

> **Context:** The user selects one or more specific folders as the scan root(s), rather than scanning an entire drive. This changes the scope, I/O characteristics, and UX model at every layer of the pipeline. This section documents how each existing design decision is adjusted — and what new capabilities become possible.

---

### 1. I/O Scheduling — Per-Folder Thread Assignment

**Original design:** Global I/O scheduler, files sorted by parent directory across all roots.

**Folder-selection adjustment:**

With discrete folder roots, thread assignment can be folder-scoped, which is both simpler and more efficient.

```rust
// src/io_scheduler.rs

pub fn schedule_folder_jobs(folders: &[FolderScanResult]) -> Vec<IOJob> {
    let mut all_jobs: Vec<IOJob> = Vec::new();

    for folder in folders {
        // Assign one reader thread per folder (or per 10k files if folder is huge)
        // Sort files within the folder by path → sequential HDD access
        let mut folder_jobs: Vec<IOJob> = folder.entries
            .iter()
            .map(|e| IOJob { path: e.path.clone(), size: e.size, folder_id: folder.id })
            .collect();

        folder_jobs.sort_by(|a, b| a.path.cmp(&b.path));  // intra-folder locality
        all_jobs.extend(folder_jobs);
    }

    all_jobs
}
```

**Thread assignment rule:**
```
Folders ≤ NUM_IO_THREADS  → 1 dedicated reader thread per folder
Folders > NUM_IO_THREADS  → folders batched across thread pool, still sorted internally
Folder with > 10k files   → split into 10k-file chunks, each treated as a sub-job batch
```

**Why this is better than the global scheduler for folder-selection:**
- Reads from folder A never interrupt sequential reads from folder B.
- On HDDs: one folder at a time = nearly sequential seek pattern.
- On SSDs: parallelism across folders is safe — assign one thread per folder up to `NUM_IO_THREADS`.

---

### 2. Partial / Full Hash — Scope Unchanged

**Adjustment:** No change to the partial hash or full hash algorithms. The 3-region adaptive chunk strategy and BLAKE3 are correct regardless of how many folders are selected.

**Scope boundary enforced at:**
- The `scanner.rs` root list — only files under selected folders are ever emitted as `FileEntry`.
- Deduplication is computed only within the combined file set from all selected folders.

**Implication:** Two files that are identical but both live *outside* all selected folders are never seen and are correctly ignored.

---

### 3. Hard-Link / Inode Filtering — Cross-Selection Awareness

**Original design:** Detect `FileId` collisions within the scanned set → skip hashing.

**Folder-selection adjustment:**

Hard links can point *outside* the selected folders. This creates an important edge case:

```
Selected folder: /Users/rishi/Photos

File A: /Users/rishi/Photos/vacation.jpg   ← in selection
File B: /Volumes/Backup/vacation.jpg        ← OUTSIDE selection (hard link to A)
```

File B is invisible to the scanner. File A appears to have `nlink > 1` (multiple hard links), but its pair is not in scope.

**Behaviours to implement:**

```rust
pub enum HardLinkStatus {
    WithinSelection(Vec<PathBuf>),   // all hard-link partners are in the selected set
    HasExternalLinks(u32),           // nlink count exceeds files found in selection
}

fn classify_hardlink(entry: &FileEntry, in_selection: &HashSet<FileId>) -> HardLinkStatus {
    let total_links = entry.nlink;  // from metadata.nlink() on Unix
    let in_scope_links = in_selection.iter()
        .filter(|id| **id == entry.file_id)
        .count() as u32;

    if in_scope_links >= total_links {
        HardLinkStatus::WithinSelection(/* collect paths */)
    } else {
        HardLinkStatus::HasExternalLinks(total_links - in_scope_links)
    }
}
```

**UI warning for external links:**
```
⚠️ /Users/rishi/Photos/vacation.jpg has 2 hard links outside the selected folders.
   Deleting this file may affect locations not in your scan.
   [ Show External Paths ]  [ Skip This File ]  [ Delete Anyway ]
```

**Note:** "Show External Paths" requires reading the full inode link list — only available via OS-level tools (`find -samefile`, `fsutil hardlink list` on Windows). Implement as optional, async lookup triggered by user click.

---

### 4. Memory Pressure — Smaller Scope, Still Chunked

**Adjustment:** Folder-selection naturally bounds the file count. A user selecting `/Downloads` is unlikely to have 1M files. Memory pressure is lower in practice.

**However — chunking is still required:**

```rust
// Edge case: user selects a folder with 500k .log files all the same size
// Without chunking → same OOM risk as global scan

const MAX_GROUP_SIZE: usize = 10_000;  // unchanged — still necessary
```

**Additional optimization for folder-selection:** Process one folder's worth of size-groups at a time before moving to the next, allowing earlier memory reclaim:

```rust
for folder in &selected_folders {
    let groups = bucket_by_size(&folder.entries);
    process_groups(groups, &token);  // hash, verify, resolve
    // groups dropped here → memory reclaimed before next folder
}
// Cross-folder deduplication happens in a final merge pass over confirmed hashes only
```

**Memory footprint with folder isolation:** Each folder's working set is independent. Peak memory = `max(single_folder_working_set)` rather than `sum(all_folders)`.

---

### 5. Cancellation & Progress — Per-Folder Granularity

**Original design:** Global `bytes_processed / bytes_total` bar.

**Folder-selection upgrade:** Progress is now two-dimensional — folder-level and file-level.

```rust
pub struct FolderProgress {
    pub folder_path:     PathBuf,
    pub files_total:     u64,
    pub bytes_total:     u64,
    pub files_done:      u64,
    pub bytes_done:      u64,
    pub status:          FolderStatus,
}

pub enum FolderStatus {
    Queued,
    Scanning,
    Hashing,
    Done { groups_found: u32 },
    Cancelled,
    Error(String),
}

pub struct ScanProgress {
    pub folders:          Vec<FolderProgress>,  // one entry per selected folder
    pub overall_bytes:    u64,
    pub processed_bytes:  u64,
}
```

**UI layout:**
```
Overall: [================>   ] 81%  (4.1 GB / 5.0 GB)

  📁 /Users/rishi/Downloads    ✅ Done — 12 groups found
  📁 /Users/rishi/Photos       ⠋ Hashing...  [=====>    ] 54%
  📁 /Volumes/Backup           ⏳ Queued

                                            [ Cancel ]
```

**Cancellation behaviour with folder-level progress:**
- Cancel mid-scan → all `Done` folders have complete results, current folder has partial results.
- Partial results from the in-progress folder are preserved and labelled `"⚠️ Partial"`.
- Queued folders are marked `Cancelled` and show zero results.

---

### 6. Smart Keep Policy — Folder-Context Scoring

**Original design:** SmartScore based on path depth, atime, preferred dirs, junk dir penalties.

**Folder-selection adjustment:** Add a **folder-origin bias** to the scoring:

```rust
fn score_with_folder_context(
    entry: &FileEntry,
    selected_folders: &[PathBuf],
    preferred_folders: &[PathBuf],  // user-ranked, from UI
) -> i64 {
    let mut score = base_smart_score(entry);  // existing scoring (depth, atime, junk)

    // Bias 1: Prefer files inside selected folders over external locations
    // (relevant when duplicates span inside + outside selection)
    let in_selection = selected_folders.iter()
        .any(|root| entry.path.starts_with(root));
    if in_selection {
        score += 200;  // strong preference for files the user can see/manage
    }

    // Bias 2: Apply user-ranked folder order
    // If user selected [/Photos, /Downloads], /Photos files score higher
    for (rank, folder) in selected_folders.iter().enumerate() {
        if entry.path.starts_with(folder) {
            // Higher rank (lower index) = more preferred
            score += (selected_folders.len() - rank) as i64 * 50;
            break;
        }
    }

    score
}
```

**UI for folder ranking:**
- Drag-to-reorder list of selected folders in `scan_panel.rs`.
- Topmost folder = highest keep preference when resolving cross-folder duplicates.
- Tooltip: `"Files in this folder will be kept when duplicates are found across folders."`

---

### 7. Action Mode — Per-Folder Preview

**Original design:** Global `ActionMode` applies to all results uniformly.

**Folder-selection upgrade:** Allow per-folder action override before applying.

```rust
pub struct FolderActionPlan {
    pub folder:      PathBuf,
    pub mode:        ActionMode,          // can differ from global default
    pub groups:      Vec<DuplicateGroup>, // duplicates rooted in this folder
    pub confirmed:   bool,                // user has reviewed and approved
}
```

**UI: Per-Folder Preview Panel**

```
┌─ Folder: /Users/rishi/Downloads ──────────────────────────────────┐
│  12 duplicate groups  │  3.2 GB reclaimable  │  Mode: [ Delete ▾ ]│
│                                                                    │
│  Group 1: vacation.jpg  ×3  (148 MB wasted)                       │
│    ✅ KEEP  /Users/rishi/Photos/vacation.jpg   [cross-folder win] │
│    🗑 DELETE /Users/rishi/Downloads/vacation.jpg                   │
│    🗑 DELETE /Users/rishi/Downloads/vacation_copy.jpg             │
│                                                                    │
│  [ ✓ Approve This Folder ]   [ Skip This Folder ]                │
└────────────────────────────────────────────────────────────────────┘
```

**Workflow:**
1. User reviews each folder's plan independently.
2. Approves or skips per-folder.
3. "Apply All Approved" runs actions only for confirmed folders.
4. Global `ActionMode` is the default; per-folder override is optional.

---

### 8. Hash Collision Safety — No Change

**Adjustment:** No change. The O(N) canonical byte-verification (`verify_group()`) applies identically within folder-selection scope. The set of confirmed duplicates is smaller, so verification is even faster in practice.

---

### 9. Cross-Platform FileId — Network / Symlink Edge Cases

**Original design:** `FileId { volume_id, file_index }` abstracts `(dev, ino)` and Windows `BY_HANDLE_FILE_INFORMATION`.

**Folder-selection edge cases to handle:**

| Scenario | Risk | Handling |
|---|---|---|
| Selected folder is a NFS mount | `dev` may differ per connection | Treat as separate volume; cross-mount hard-link detection disabled with warning |
| Selected folder is a Windows SMB share | `BY_HANDLE_FILE_INFORMATION` may not be supported by the server | Fallback: disable hard-link detection for UNC paths, log warning |
| Selected folder is a symlink (`/foo → /bar/baz`) | Scanner resolves symlink → files appear under `/bar/baz` | Canonicalize all paths with `fs::canonicalize()` before `FileId` lookup |
| Selected folder spans two drives (macOS APFS volume group) | Same file may appear to have different `dev` values on different snapshots | Use `file_index` (inode) as primary key within same `volume_id` only |

**Symlink canonicalization in scanner:**
```rust
// src/scanner.rs — applied to every FileEntry path before storing
let canonical_path = std::fs::canonicalize(&raw_path)
    .unwrap_or(raw_path);  // fallback to raw if canonicalize fails (broken symlink)
```

**Network path detection heuristic:**
```rust
fn is_network_path(path: &Path) -> bool {
    #[cfg(unix)] {
        // Check if the mount point's filesystem type is NFS/CIFS/SMB
        // via /proc/mounts (Linux) or statfs(2) (macOS)
        check_statfs_type(path)  // returns true for NFS, CIFS, SMBFS, FUSE
    }
    #[cfg(windows)] {
        // UNC path: \\server\share\...
        path.to_str().map(|s| s.starts_with("\\\\")
            || s.starts_with("//")).unwrap_or(false)
    }
}
```

---

### 10. BFS Depth — Per-Folder Parallel Scan

**Original design:** Global `parallel_scan()` with a shared `VecDeque` across all roots.

**Folder-selection adjustment:** Run a dedicated `parallel_scan()` instance per selected folder, then merge results in a final pass.

```rust
// src/scanner.rs

pub fn scan_selected_folders(
    folders: &[PathBuf],
    config: &ScanConfig,
    token: &CancellationToken,
    progress: Arc<Mutex<ScanProgress>>,
) -> Vec<FolderScanResult> {
    folders
        .iter()
        .map(|root| {
            // Update per-folder progress before starting
            update_folder_status(&progress, root, FolderStatus::Scanning);

            // Each folder gets its own BFS worker pool
            let entries = parallel_scan(vec![root.clone()], config, token);

            let result = FolderScanResult {
                root:    root.clone(),
                entries,
            };

            update_folder_status(&progress, root, FolderStatus::Done {
                groups_found: 0  // filled in after hash pass
            });

            result
        })
        .collect()
}
```

**Why per-folder scan instances instead of one shared pool:**
- Per-folder `FolderStatus` transitions are clean and predictable.
- Cancellation of one folder doesn't bleed into another.
- On HDDs: serialized per-folder scanning is more disk-friendly than interleaved scanning.
- Progress reporting is naturally per-folder.

---

## Folder-Specific Enhancements

These are new capabilities that only make sense once folder-selection is the primary UX model.

---

### Enhancement: Folder-Level Summary

After scanning and resolving, each selected folder gets a summary card in the results panel:

```
┌─ Summary: /Users/rishi/Downloads ─────────────────────────────────┐
│  Files scanned:      8,291                                         │
│  Duplicate groups:   23                                            │
│  Duplicates found:   61 files                                      │
│  Space reclaimable:  4.7 GB                                        │
│  Errors:             2 (permission denied)                         │
│  Hard links found:   5 (3 with external targets)                   │
│  Action (pending):   Delete                                        │
└────────────────────────────────────────────────────────────────────┘
```

**Implementation:** Add `FolderSummary` to `ScanResult`, computed during resolution phase.

```rust
pub struct FolderSummary {
    pub root:                PathBuf,
    pub files_scanned:       u64,
    pub duplicate_groups:    u32,
    pub total_wasted_bytes:  u64,
    pub error_count:         u32,
    pub external_hardlinks:  u32,
    pub action_pending:      ActionMode,
}
```

---

### Enhancement: Selective Subfolder Exclusion

Allow the user to exclude specific subdirectories from within a selected folder before or during scanning.

**UI in `scan_panel.rs`:**
```
📁 /Users/rishi/Photos                            [ Remove ]
   └─ Subdirectories:                             [ Expand ]
       ✅ RAW                  (4,218 files)
       ✅ Edited               (1,002 files)
       ❌ Archived-2021        (9,847 files)   ← excluded
       ❌ node_modules         (auto-excluded)
```

**Implementation:**
```rust
pub struct FolderConfig {
    pub root:            PathBuf,
    pub excluded_subdirs: Vec<PathBuf>,  // user-excluded
}

// In scanner.rs — check exclusion per directory:
if config.folders.iter().any(|fc| {
    fc.excluded_subdirs.iter()
      .any(|ex| current_dir.starts_with(ex))
}) {
    continue;  // skip this directory branch
}
```

**Auto-exclusions** (always applied, shown as `auto-excluded`):
- `node_modules`, `.git`, `__pycache__`, `.Trash`, `$RECYCLE.BIN`
- User can override auto-exclusions via a toggle.

---

### Enhancement: Per-Folder Pre-Action Preview

Before applying any actions, present a per-folder preview that shows exactly what will happen on disk:

```
⚠️  Action Preview: DELETE mode

    Folder: /Users/rishi/Downloads
    ──────────────────────────────
    • 31 files will be moved to Trash
    • 0 files will be permanently deleted
    • 4.7 GB will be reclaimed
    • 2 groups have cross-folder duplicates (your /Photos copy will be kept)

    [ Confirm & Apply ]   [ Edit Selections ]   [ Cancel ]
```

**Implementation:** `DryRun` pass over the `FolderActionPlan` — walk every action, count moves, verify targets exist, detect cross-folder conflicts. All computed in-memory, no disk writes.

```rust
pub struct ActionPreview {
    pub folder:                PathBuf,
    pub files_to_delete:       u32,
    pub bytes_to_reclaim:      u64,
    pub cross_folder_conflicts: Vec<CrossFolderConflict>,
    pub hard_link_warnings:    Vec<ExternalHardLinkWarning>,
}
```

---

### Enhancement: Multi-Folder Conflict Resolution

When the same file (by hash) exists in multiple selected folders, a conflict arises: which folder's copy is kept?

**Detection:**
```rust
pub struct CrossFolderConflict {
    pub hash:     [u8; 32],
    pub copies:   Vec<(PathBuf, FileEntry)>,  // (folder_root, entry) for each copy
    pub resolution: ConflictResolution,
}

pub enum ConflictResolution {
    AutoSmartScore(usize),    // SmartScore winner index
    UserPicked(usize),        // user manually selected
    Deferred,                 // user skipped — no action taken
}
```

**Resolution algorithm:**
1. Apply `score_with_folder_context()` to all copies.
2. Highest score wins → becomes the `keep` file.
3. The keep's folder is shown in the UI as `"[cross-folder winner]"`.
4. User can override in the per-folder preview panel.

**UI for conflicts:**
```
🔀 Cross-Folder Conflict: 3 copies found

  📁 /Users/rishi/Photos/vacation.jpg    Score: 820  ← AUTO-KEEP
  📁 /Users/rishi/Downloads/vacation.jpg Score: 340  🗑 delete
  📁 /Volumes/Backup/vacation.jpg        Score: 290  🗑 delete

  [ Accept Auto-Resolution ]   [ Choose Manually ]
```

**Tie-breaking:** If SmartScores are within 10 points (rounding noise), prefer the copy in the folder that appears first in the user's selection order.

---

## Phase 8 — Folder-Selection Mode Checklist

- [ ] `src/types.rs` — Add `FolderScanResult`, `FolderProgress`, `FolderStatus`, `FolderSummary`, `FolderConfig`, `FolderActionPlan`, `CrossFolderConflict`, `ActionPreview`.
- [ ] `src/scanner.rs` — `scan_selected_folders()`: per-folder `parallel_scan()` instances, symlink canonicalization, per-folder progress updates.
- [ ] `src/scanner.rs` — Subfolder exclusion check (`FolderConfig::excluded_subdirs`).
- [ ] `src/io_scheduler.rs` — `schedule_folder_jobs()`: per-folder sorted job batches, thread-per-folder assignment.
- [ ] `src/pipeline.rs` — Folder-isolated bucketing/hashing pass + final cross-folder merge on confirmed hashes.
- [ ] `src/resolver.rs` — `score_with_folder_context()`: folder-origin bias + selection-order ranking.
- [ ] `src/resolver.rs` — `CrossFolderConflict` detection and `ConflictResolution` application.
- [ ] `src/resolver.rs` — External hard-link detection (`HardLinkStatus::HasExternalLinks`), async lookup hook for "Show External Paths".
- [ ] `src/resolver.rs` — `action_preview()`: dry-run pass producing `ActionPreview` before any disk writes.
- [ ] `src/platform.rs` — `is_network_path()`: `statfs`-based check (Unix) + UNC prefix check (Windows).
- [ ] `src/ui/scan_panel.rs` — Multi-folder selector with drag-to-reorder, subfolder expand/exclude UI, `FolderConfig` per entry.
- [ ] `src/ui/progress_panel.rs` — Per-folder `FolderStatus` row display + overall bytes bar.
- [ ] `src/ui/results_panel.rs` — Per-folder summary cards, `CrossFolderConflict` resolution panel, per-folder preview + confirm workflow.
- [ ] `src/ui/results_panel.rs` — External hard-link warning with "Show External Paths" async trigger.
