# Smart File Manager — Product features and implementation plan

This document is the **shippable product plan** and delivery checklist. The engineering deep-dive lives in [Implementation.md](Implementation.md). **Desktop UX** (theme, results layout, duplicate-group preview) is specified in [UI-Implementation-Plan.md](UI-Implementation-Plan.md).

## Vision

- **Batch, deterministic deduplication:** select folders → scan → index (hashes) → report and optional actions. No background daemon.
- **Local-first:** all analysis runs on the user’s machine; optional JSON export only.
- **Cancellable:** user can stop at any time; **partial results** are still shown.
- **Trust:** dry-run by default; byte-level verification after full hash; atomic hard-link replacement; deletes go to **Trash (macOS)** / **Recycle Bin (Windows)** via the `trash` crate where supported.

## Non-goals (current generation)

- Continuous filesystem watching or cloud sync.
- Editing file contents or merging non-identical files.
- Full near-duplicate / perceptual image pipeline in the default build (see optional features).

## Platform support

| Area | macOS | Windows |
|------|--------|---------|
| **Architectures** | `aarch64-apple-darwin`, `x86_64-apple-darwin` (Apple Silicon + Intel iMac / Mac) | `x86_64-pc-windows-msvc`; `aarch64-pc-windows-msvc` when available |
| **Stable file identity** | `st_dev` + `st_ino` via `MetadataExt` | `GetFileInformationByHandle` → `FileId` (`winapi`) |
| **Hard links** | Same APFS/HFS+ volume | Same NTFS volume; cross-volume attempts return `CrossVolumeHardLink` |
| **Delete** | Finder Trash | Recycle Bin (where `trash` supports the path) |
| **Folder picker** | Native (`rfd`) | Native (`rfd`); UNC paths detected for network heuristics |
| **CI** | `macos-latest` in [.github/workflows/ci.yml](.github/workflows/ci.yml) | `windows-latest` |

**Release checklist (manual):** note OS version + arch verified for each release (e.g. macOS 14 arm64, Windows 11 x64).

## Feature matrix

| Capability | Status |
|------------|--------|
| Exact byte-for-byte duplicates (BLAKE3) | Implemented (`pipeline`, `hasher`) |
| Hard-link groups (same `FileId`, skip hashing) | Implemented |
| Partial → full hash pipeline + chunked size groups | Implemented |
| I/O reader pool + locality sort | Implemented (`io_scheduler`) |
| Byte verify O(N) vs canonical file | Implemented (`verifier`) |
| Smart keep scoring + root order | Implemented (`resolver`, UI root reorder) |
| Dry run / delete / hard link | Implemented |
| Hash cache (incremental rescans) | Implemented (`cache`, `dirs` cache dir) |
| Glob excludes + subtree path excludes | Implemented |
| Per-folder config types + summaries | Implemented (`FolderConfig`, `FolderSummary`) |
| Cross-folder conflict listing | Implemented (results UI) |
| Headless CLI | `sfm-cli` (`--features cli`) |
| Near-duplicate (LSH) | Stub (`--features similarity`) |
| CDC / Rabin | Stub (`--features cdc`) |
| Professional dark UI + sectioned scan screen | Implemented (`ui/style`, `scan_panel`, `progress_panel`) |
| Results: resizable group list + detail pane | Implemented (`SidePanel` + `results_*`) |
| Per-group / per-file preview (text, hex, image thumb) | Implemented (`ui/preview`, `image` crate) |
| Reveal in Finder / Explorer | Implemented (`preview::reveal_in_folder`) |

## User stories

- **Reclaim disk space:** scan Downloads + external drive; review groups; delete duplicates to Trash.
- **Same file, many paths:** replace duplicates with hard links on one volume (atomic rename strategy).
- **Photo / project folders:** add multiple roots, rank folders (top = preferred keep), resolve cross-folder conflicts in the results view.
- **Automation / NAS:** run `sfm-cli` with `SFM_NO_CACHE=1` if desired; consume JSON on stdout.

## Implementation phases (checklist)

### Phase A — Core engine

- [x] `Cargo.toml` dependencies (BLAKE3, rayon, crossbeam-channel, memmap2, serde, trash, dirs, glob, rfd, winapi on Windows)
- [x] `types`, `errors`, `cancel`, `platform` (`FileId` Unix + Windows)
- [x] `scanner` (walkdir + filters + canonical paths)
- [x] `io_scheduler`, `hasher` (adaptive chunk size per §17), `verifier`, `pipeline`, `resolver` (atomic hard link)

### Phase B — Desktop UI

- [x] `scan_panel`, `progress_panel`, `results_panel`, `app` routing
- [x] Background thread + `CancellationToken` + channel handoff to results
- [x] JSON export + action manifest export

### Phase C — Folder-selection mode (essentials)

- [x] Multiple roots, drag-free reorder (↑ / remove) for keep priority
- [x] `FolderConfig` + subtree exclude list (shared list applied per root)
- [x] `FolderSummary` + cross-folder conflict section
- [x] Per-folder progress placeholders (`FolderProgress` / `FolderStatus`) for future refinement

### Phase D — Polish

- [x] Hash cache with `RwLock` for parallel reads
- [x] Heuristics (optional skip `.log` / `.lock` / `.pid`)
- [x] Byte-based progress during hashing
- [x] Criterion bench scaffold (`benches/hash_bench.rs`)
- [x] Integration tests (`tests/dedup_integration.rs`)

### Phase E — Advanced (optional Cargo features)

- [x] `similarity` / `cdc` feature flags with documented stubs in `similarity.rs` / `cdc.rs`
- [ ] Full MinHash + LSH pipeline (see Implementation.md §14)
- [ ] CDC integration crate when ready

## Safety guarantees

| Topic | Behavior |
|-------|-----------|
| Default action | Dry run — no filesystem mutations from actions |
| Hash collisions | Byte comparison against canonical copy before reporting |
| Hard link replace | Temp hard link + `rename` (atomic on POSIX); temp suffix `sfm_tmp_*` |
| Deletes | `trash::delete` (platform Trash / Recycle Bin) |
| Cross-volume hard link | Error per file; no silent fallback to delete |
| Cancel | Pipeline stops cooperatively; `stats.partial = true` |

## Performance notes (targets from Implementation.md §12)

- Pipeline is **O(n)** in file count for enumeration and filtering; hashing is proportional to bytes read (partial cap ~3× chunk per file, full hash only on collision groups).
- **Memory:** size groups chunked at **10,000** files; bounded I/O channels in `io_scheduler`.
- **Tuning:** UI exposes **I/O profile** (Auto / HDD-safe / SSD-fast) to limit reader threads.

## Roadmap

1. **v0.1 (current):** exact dedupe + GUI + CLI + cache + tests + CI matrix (macOS + Windows).
2. **v0.2:** richer per-folder progress, optional “Reveal in Finder / Explorer”, sparse external-hard-link path lookup.
3. **v0.3:** optional `similarity` MinHash pipeline + separate “Similar files” UI section.
4. **Future:** perceptual image near-dup (clearly labeled, never mixed with exact-delete without confirmation).
5. **Optional:** Next.js + shadcn shell (e.g. Tauri) — see [docs/NextJS-Shadcn-Integration-Plan.md](docs/NextJS-Shadcn-Integration-Plan.md) (Rust keeps all hashing/I/O).

## Commands

```bash
# GUI
cargo run --bin smart-file-manager

# Headless JSON report
cargo run --features cli --bin sfm-cli -- /path/to/folder

# Disable hash cache in CLI
SFM_NO_CACHE=1 cargo run --features cli --bin sfm-cli -- /path

# Optional feature stubs
cargo build --features similarity
cargo build --features cdc

# Bench
cargo bench
```

## Verified partial-hash formula (shipped)

Three regions per file; chunk size = **`min(8192, max(1, file_size / 4))`** bytes per region (Implementation.md §17 Enhancement 3). Hashes are **BLAKE3**; full file is hashed only for partial-hash collision groups.
