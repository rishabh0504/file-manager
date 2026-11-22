# Professional UI & Duplicate-Group Preview — Implementation Plan

This document describes the **product UX**, **visual system**, and **technical implementation** for upgrading Sortwise to a polished desktop experience with **duplicate-group browsing** and **file preview**. It complements [Implementation.md](Implementation.md) (engine) and [Features.md](Features.md) (shipping scope).

---

## 1. Goals

| Goal | Success criteria |
|------|------------------|
| **Professional appearance** | Consistent dark theme, spacing scale, hierarchy (title → section → detail), readable monospace for paths |
| **Scannable results** | User can quickly see how many groups, space reclaimable, and jump between groups without endless vertical scroll |
| **Group preview** | For the active duplicate group: see all members, pick one file, inspect **metadata + content preview** without leaving the app |
| **Safe actions** | Destructive actions remain explicit; preview is read-only |
| **Cross-platform** | macOS / Windows behaviors for “reveal in folder” match user expectations |

---

## 2. Information architecture

### 2.1 Screens (unchanged flow, richer chrome)

1. **Scan** — Sources, filters, action mode, advanced options (collapsible sections).
2. **Scanning** — Phase-aware status, byte progress, cancel (card-style center column optional).
3. **Results** — **Split layout**: **group list** (left) + **detail & preview** (right). Top toolbar: summary chips, export, back, apply.

### 2.2 Results screen regions

```
┌──────────────────────────────────────────────────────────────────────┐
│ Toolbar: [Summary chips] [Search]     [Export ▾] [Back] [Apply…]    │
├────────────────┬─────────────────────────────────────────────────────┤
│ Duplicate      │  Selected group header (hash snippet, count, waste) │
│ groups         │  ───────────────────────────────────────────────────│
│ (scroll)       │  File rows (KEEP / duplicate, path, score, actions) │
│                │  ───────────────────────────────────────────────────│
│  ▼ Group 1     │  PREVIEW                                            │
│    Group 2     │  · Metadata (size, modified, kind)                  │
│    Group 3     │  · Text / hex / image thumbnail                     │
│                │  · Reveal in Finder / Explorer                      │
├────────────────┴─────────────────────────────────────────────────────┤
│ Optional: Cross-folder / Hard-link collapsible sections (full width) │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 3. Visual design system

### 3.1 Theme (`ui/style.rs`)

- **Base:** `egui::Visuals::dark()` customized:
  - Panel backgrounds slightly lifted from pure black (`#1c1c20` / `#121216`).
  - **Accent** for selection and primary actions (e.g. blue-violet `#3c64b8` or teal `#2d8a7a` — pick one and use consistently).
  - **Warning** banner color for partial scan / destructive mode hints.
- **Spacing:** `item_spacing` ~ `(10, 8)`, `button_padding` comfortable for trackpad.
- **Typography:** Default egui font; **monospace** for paths, hashes, hex dumps.
- **Corners:** Slight rounding on “cards” via `Frame::NONE.fill().rounding(6).inner_margin(12)`.

### 3.2 Components

| Component | Role |
|-----------|------|
| `section_header` | Bold title + optional subtitle |
| `card` | `Frame` wrapper for grouped controls |
| `stat_chip` | Small pill: “12 groups”, “4.2 GB reclaimable” |
| `primary_button` | High-emphasis (e.g. Start scan, Apply) |
| `path_label` | Monospace, elide middle if needed (`…`) |
| `danger_zone` | Bordered sub-area before Apply when not dry-run |

---

## 4. Preview subsystem (`ui/preview.rs`)

### 4.1 Responsibilities

- **Read-only** inspection of **one selected path** (from duplicate group member).
- **No execution** of file content; no shelling out to open file by default (optional future).

### 4.2 Content strategy

| File kind | Behavior |
|-----------|----------|
| **Empty** | Message “Empty file” |
| **UTF-8 text** (heuristic: valid UTF-8 + high printable ratio) | Show first **N** chars in scrollable `TextEdit` (read-only) or `Label` in monospace |
| **Binary** | Hex dump of first **256 bytes** (16 bytes / line) |
| **Image** (`png`, `jpg`, `jpeg`, `gif`, `webp`) | If file size ≤ **12 MiB**, decode thumbnail max **320×320** (preserve aspect), show in `egui::Image` via `ctx.load_texture` |
| **Too large / decode error** | Show size + extension + “Preview not loaded” + hex option for first 256 B |

### 4.3 Heuristics

- Read at most **64 KiB** for text detection; if longer, show truncation notice.
- **MIME/extension** hint from path extension; fallback to content sniff (optional magic bytes).

### 4.4 Texture lifecycle

- Store `(PathBuf, egui::TextureHandle)` on app state; if selected path changes, **drop** old handle and load new texture (egui refcount).
- Avoid reloading every frame: compare path + optional mtime key.

### 4.5 Reveal in folder

| OS | Command |
|----|---------|
| macOS | `open -R <path>` |
| Windows | `explorer /select,<path>` (comma as per `explorer` syntax) |
| Other | Best-effort `xdg-open` on parent directory |

---

## 5. Interaction details

### 5.1 Selection model

- `selected_group: Option<usize>` — index into `duplicate_groups`.
- `selected_file_in_group: usize` — which member’s preview is shown.
- On opening Results: select group `0`, file `0` (usually keep candidate).
- Changing group resets file index to `0` or keep index.

### 5.2 Search / filter (lightweight)

- Filter group list by substring match on **any member path** or hash prefix (client-side only).
- Does not re-run scan.

### 5.3 Keyboard (incremental)

- **↑/↓** in group list when focused (future); initial ship can be mouse-first.

---

## 6. Performance & safety

- Preview I/O runs on **UI thread** for small reads only (≤ 64 KiB text sniff + bounded image decode). If stutter appears, move to `std::thread::spawn` + `channel` with cancel token (v2).
- Image decode: reject files > 12 MiB for thumbnail path; still allow hex preview of head.
- **Do not** mmap user-selected arbitrary paths for preview without caps (use `File::open` + `read_limit`).

---

## 7. File layout (code)

| File | Purpose |
|------|---------|
| [src/ui/style.rs](src/ui/style.rs) | `configure_app_style(ctx)`, frames, chips, helpers |
| [src/ui/preview.rs](src/ui/preview.rs) | `PreviewData`, `load_preview`, `reveal_in_folder`, `format_bytes` |
| [src/ui/results_panel.rs](src/ui/results_panel.rs) | Split layout, toolbar, integration with preview |
| [src/ui/scan_panel.rs](src/ui/scan_panel.rs) | Sectioned layout using style helpers |
| [src/ui/progress_panel.rs](src/ui/progress_panel.rs) | Card + progress styling |
| [src/main.rs](src/main.rs) | Call `configure_app_style` in `eframe` setup |
| [src/app.rs](src/app.rs) | Selection state + `preview_texture` cache |

---

## 8. Dependencies

- **`image`** (optional features: `png`, `jpeg`, `gif`, `webp`) for thumbnails — `default-features = false` to keep binary lean.

---

## 9. Testing checklist

- [ ] Results: select each group; preview updates.
- [ ] Text file: readable preview; long file truncated message.
- [ ] Binary: hex appears.
- [ ] PNG/JPEG under size cap: thumbnail visible.
- [ ] Reveal works on macOS and Windows (manual).
- [ ] Resize window: split panes behave (`egui` side panels or proportional `horizontal` with `min_width`).
- [ ] After Apply, UI does not crash (texture/path invalid — handle gracefully).

---

## 10. Roadmap after v1 UI

- Async preview loader + spinner.
- Perceptual diff for two selected files (out of scope for exact dedupe).
- List virtualization for 10k+ groups (`Table` or custom virtual scroll).
- Localization strings.

---

## 11. Implementation status

| Item | Status |
|------|--------|
| `style.rs` — `configure_app_style`, cards, chips, primary button | Done |
| `main.rs` — theme applied at startup | Done |
| `scan_panel` — collapsible sections, cards | Done |
| `progress_panel` — card layout, spinner | Done |
| `results_dup_list_panel` — `SidePanel`, filter, selectable rows | Done |
| `results_main_panel` — toolbar, group card, file rows, preview | Done |
| `preview.rs` — text / hex / image (≤12 MiB), reveal | Done |
| Async preview + virtualized group list | Planned (see §6, §10) |

Tracked in [Features.md](Features.md) feature matrix. This plan is the **source of truth** for UX intent; adjust here first, then align code.

---

## 11. Review findings — all views (2026-10-31)

**Scope:** Explorer (list/grid/folders, folder modal, queues, movable inbox, player), Duplicate Finder (scan → progress → results), A→B Compare, Empty Folders, Archive Files, Folder Intelligence (twins modal).
**Basis:** code read of `src/app.rs`, `src/ui/*.rs`, `src/pipeline.rs`, `src/hasher.rs`, `src/cache.rs`, `src/scanner.rs`, plus a release-mode micro-benchmark of the per-frame clone (P-1). `cargo test`: 10 passed, 0 failed. No 5 TB run yet.

**Verification status (2026-10-31, second pass):** rows tagged ⚠️ were corrected, ❓ are reasoned-only (not run), and untagged or ✅ rows were confirmed by code read (and the P-1 benchmark where noted). Corrections are applied inline.

Legend: **P0** = data-safety / blocks 5 TB use · **P1** = needed for production · **P2** = polish.

### 11.1 Product & UX

| # | Pri | Finding | Where | Fix |
|---|-----|---------|-------|-----|
| U1 | P0 | ✅ **Apply actions has no confirmation.** Delete / hard-link mode touches every non-keep copy in all groups in one click. Per-group delete does confirm; global apply does not. | `results_panel.rs` `apply_all` (~1113) | Summary dialog: file count, bytes, sample paths, "Trash, recoverable". |
| U2 | P0 | ✅ **"Delete All Copies" leaves zero copies** (keep candidate included; `results_panel.rs:336–357` passes all `g.files`). | `results_panel.rs` ~336 | Exclude `keep_index`, or drop action. |
| U3 | P0 | ✅ **"Clean Same-Folder Duplicates" trashes in bulk, no confirm** (no `MessageDialog` in `clean_all_same_folder_duplicates`). | `results_panel.rs` ~620 → `clean_all_same_folder_duplicates` | Same summary dialog as U1. |
| U4 | P1 | Mode switch is **icon-only**; Archives / Empty / A→B meaning is guessed. | `app.rs` `mode_switch` (~2162) | Labelled tabs or segmented control with text. |
| U5 | P1 | **Hidden side effect:** clicking Duplicate Finder inside Explorer with no roots silently scans the Explorer folder. | `app.rs` `mode_switch` | Show roots picker; no implicit scan. |
| U6 | P1 | **Min Size = 1 byte** default; DragValue shows raw bytes (`1 B`). Floods groups with tiny files. | `scan_panel.rs` `render_settings_panel` | Default ≥ 1 KB; unit dropdown (KB/MB/GB) or typed "10 MB". |
| U7 | P0 | ⚠️ **I/O profile control does nothing.** `ScanConfig.io_profile` is never read outside config copying (`types.rs:84`, `app.rs:508`). Hasher and pipeline ignore it; `io_scheduler` is used only for `partial_jobs` (`hasher.rs:3`), its reader pool (`run_io_jobs`) has no callers. The "HDD Safe / SSD Fast" dropdown is a dead control that also hides under "Advanced Filters". | `scan_panel.rs`, `pipeline.rs`, `io_scheduler.rs` | Wire profile into thread count (P-8) or remove the control. Then surface it. |
| U8 | P1 | **Jargon / overlapping concepts:** "A → B Target Comparison", "Smart Clean", "Folder Twins Analysis", Movable inbox + Folder Queues (two staging systems). | `app.rs`, `results_panel.rs`, `explorer_panel.rs` | Rename; merge staging into one "Staging" concept. |
| U9 | P1 | Dry-run state not explained in results; user must return to Scan to change mode. | `results_panel.rs` `action_row` | Banner "Dry run — nothing changes" + "Switch to Delete" in results. |
| U10 | P1 | **Keep choice not explained.** Keep marker shown, reason (newest / root priority / shortest path) not shown. | `results_panel.rs` | Show `keep_reason` next to keep badge. |
| U11 | P1 | No **type filter** in group list (images / videos / archives) though product is media-focused. | `results_panel.rs` `results_dup_list_panel` | Type chips using existing `FileKind`-style classification. |
| U12 | P1 | Progress shows **raw bytes** (`123456 / 9876543 bytes`), no ETA, throughput or elapsed time. Cancel does not say partial results are kept. | `progress_panel.rs` | Human units, MB/s, ETA, elapsed; cancel copy: "Keep results so far". |
| U13 | P2 | ⚠️ Diagnostic text leaks to users: `"Diagnostic: Rust successfully read file …"`. Only compiled with `--features mpv` (not default); re-set each frame until `mpv_loaded_path` matches. | `explorer_panel.rs` 2789–2804 (`ensure_mpv`) | Remove or log via `tracing`. |
| U14 | P2 | Icon-only glyphs (`← → ⟳ ✖ 🗑 📂`) depend on font coverage; render inconsistently across OS. | `icons.rs`, all panels | Verify on macOS + Windows; prefer egui icon font or SVG. |
| U15 | P2 | Native `rfd` confirm dialogs block the egui loop and cannot match theme. | `results_panel.rs`, `folder_intelligence.rs`, `app.rs` | In-app modal component (one shared `confirm_dialog`). |
| U16 | P2 | No settings persistence — scan config resets each launch. | `app.rs` (no settings file in `src/`) | Persist to app config dir (JSON, versioned). |

### 11.2 Performance — 5 TB, duplicate images / video / archives

| # | Pri | Finding | Where | Fix |
|---|-----|---------|-------|-----|
| P-1 | P0 | ✅ **Full `ScanResult` clone every frame.** `last_result.clone()` runs in both list (`results_panel.rs:25`) and main panel (`:179`), both called from `update` each frame. **Measured:** release build, 1M groups / 2M files → clone ≈ 100–120 ms per call; ×2 per frame ≈ 200+ ms/frame (≈ 4–5 fps ceiling). Debug builds are slower. | `results_panel.rs` L20-ish and L173-ish | Store `Arc<ScanResult>`; clone the `Arc` (cheap) or borrow. |
| P-2 | P0 | ✅ **Dup group list not virtualized.** Every card rendered each frame; grep finds no `show_rows` in `results_panel.rs` (explorer uses it). Not timed. | `results_panel.rs` `results_dup_list_panel` (~line 100–170) | `ScrollArea::show_rows` with fixed row height. |
| P-3 | P0 | **Per-frame recompute:** filter (`to_lowercase` per group), sort, `by_folder` BTreeMap rebuild, max-wasted scan. O(groups × files) each frame. | `results_panel.rs` `filtered_group_indices` (~734), `sort_visible_dup_groups` (~642) | Memoize keyed on (result epoch, filter, sort). |
| P-4 | P1 | ⚠️ **Thumbnail reads whole file into RAM** then decodes full-resolution (`std::fs::read` + `load_from_memory`). Runs on a worker thread (`app.rs:325`), so cost is RAM and I/O spikes, not a UI stall. RAW / 4K JPEG = tens of MB per thumb. | `explorer_panel.rs` `load_image_thumb` (~3412) | Stream + `ImageReader` with embedded EXIF thumb first; JPEG DCT downscale; cap concurrent decodes. |
| P-5 | P2 | ⚠️ **Per-frame disk I/O in UI, scoped:** `fs::metadata` per queue item (~2063) runs only while `movable_queue_window` is open (`movable_queue_open` gate, ~1850). `File::open` diagnostic (~2802) runs per frame only in `--features mpv` builds. | `explorer_panel.rs` | Use `ExplorerEntry.size` / cached size; remove diagnostic. |
| P-6 | P1 | ✅ **Batch, not streaming.** `app.rs:551` collects all entries, then `app.rs:572` calls `run_dedup` on the full `Vec`. `parallel_scan_roots` collects all `FileEntry` into one `Vec` before hashing; no groups visible until enumeration + hashing finish. | `scanner.rs` (~128), `pipeline.rs` `run_dedup` | Stream size buckets as enumerated; emit partial groups. |
| P-7 | P1 | ✅ **Hash cache = one blob.** Whole map loaded at start, rewritten whole at end; `fs::write` not atomic — crash mid-write loses cache. | `cache.rs` `load`/`save` | SQLite (WAL) keyed by `(dev, ino, size, mtime)`; or temp-file + rename as minimum. |
| P-8 | P1 | ✅ **Thread policy ignores device type and the I/O profile.** `hash_thread_count` (`pipeline.rs:20`) = cores/2 clamped 2–6 for every drive; `IoProfile` is dead (see U7). Parallel random reads on a single 5 TB HDD likely thrash. Not measured on HDD. | `pipeline.rs` ~20 | Detect rotational vs SSD per volume; default 1–2 readers on HDD. |
| P-9 | P2 | ⚠️ **Progress clone each 150 ms.** `progress_panel.rs` clones the whole `ScanProgress` (incl. folder vec) per tick. Workers lock once per size group (`pipeline.rs:189–202`), not per file, so contention is low. | `progress_panel.rs`, `pipeline.rs` | Snapshot only the fields shown; cheap fix. |
| P-10 | P1 | ❓ **mmap for files > 4 MB.** `hasher.rs:79` skips mmap only for network paths and sparse files; removable drives still use mmap. Unplug mid-read → SIGBUS is standard mmap behavior, not reproduced here. | `hasher.rs` `full_hash` | Use buffered reads for removable volumes; extend `is_network_path`-style check. |
| P-11 | P1 | ❓ **Memory per entry** (`PathBuf` + snapshot per file). Estimate only; the P-1 benchmark (2M files) did not measure RSS. | `types.rs` `FileEntry` / `FileEntrySnapshot` | Measure with synthetic set; intern dir paths. |
| P-12 | P1 | ✅ **Exact-only detection misses real-world dupes:** resized / re-saved images, re-encoded videos. `similarity.rs` and `cdc.rs` are stubs. | `similarity.rs`, `cdc.rs` | Phase 3: pHash/dHash for images, keyframe signature for video, in a separate "Similar" section (never mixed with exact delete). |
| P-13 | P2 | Min Size 1 B default (see U6) inflates hashing work. | `app.rs` default | Default ≥ 4 KB. |
| P-14 | P2 | No benchmark for 5 TB profile. `benches/hash_bench.rs` covers hashing only. | `benches/` | Add synthetic-tree bench: 1 M files, mixed sizes, cold/warm cache, HDD vs SSD. |

### 11.3 Usability → production-grade

| # | Pri | Finding | Fix |
|---|-----|---------|-----|
| R1 | P0 | No **undo / operation history.** Trash is only recovery path, and results are gone after "New scan". | Auto-write action log (JSON per run, reuses manifest code); "Open Trash" link. |
| R2 | P1 | ✅ **Low contrast:** `TEXT_TER` (95,100,125) on `SURFACE_1` (26,26,34) = **2.97:1** (computed, WCAG formula), below AA 4.5:1. `TEXT_SEC` on same surface = 6.53:1 (passes). Used for hints and paths. | Raise `TEXT_TER` to ≈ 4.6:1 minimum; re-check `TEXT_SEC`. |
| R3 | P1 | ⚠️ **Keyboard coverage minimal:** arrows/J/K in results and Esc in explorer/folder modals exist (`explorer_panel.rs:1178,1835,2287`; `folder_explorer_panel.rs:317`). Missing: Delete key to trash, Cmd/Ctrl+A select-all (no `Key::Delete` or modifier-A found), documented shortcut map. | Document shortcut map; add Del (trash selected), Esc (close), Cmd/Ctrl+A. |
| R4 | P1 | ⚠️ **macOS Full Disk Access guidance exists only in Explorer** (`explorer_panel.rs:845–856`). Duplicate scan protected paths go only to the error log (`files_skipped_error`) with no prompt, so 5 TB totals can look wrong. | Detect `EPERM` on protected paths; show banner with "Open Privacy settings". |
| R5 | P1 | Errors shown as joined text blob (`apply_errors`). | Error table: path, reason, retry. |
| R6 | P2 | Repo hygiene: `.DS_Store`, `mpv_log.txt` tracked in tree; `AGENTS.md` deleted in working tree. | Add to `.gitignore`; restore or remove `AGENTS.md` deliberately. |
| R7 | P2 | Status table in §10 says "Done" for items that still carry the issues above. | Re-audit §10 against §11 each release. |

### 11.4 Suggested order

1. **Safety:** U1, U2, U3, R1 — confirm + undo before any wider release.
2. **UI perf:** P-1, P-2, P-3 — single-session fix, largest win on large result sets.
3. **Scan perf:** P-6, P-7, P-8, P-10.
4. **UX clarity:** U4–U12, R2–R4.
5. **Feature gap:** P-12 (similar media) after exact path is solid.

Doc owner note: keep §11 IDs stable so commits and issues can reference them (e.g. "fix P-1").

---

## 12. Execution plan

Phased implementation plan, tasks, acceptance criteria and open decisions now live in [Implementation-Plan.md](Implementation-Plan.md). Finding IDs in §11 map to phases there.
