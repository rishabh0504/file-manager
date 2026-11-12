# Implementation Plan — UI Safety, Performance & UX

Written for: engineers working on Sortwise who will implement these changes.

This plan turns the findings in [UI-Implementation-Plan.md §11](UI-Implementation-Plan.md) into ordered, testable work. Finding IDs (U1, P-1, R2, …) refer to §11 and must stay stable so commits and PRs can cite them. Engine design lives in [Implementation.md](Implementation.md); shipping scope lives in [Features.md](Features.md).

**Status:** Phases 0–6 implemented, Phase 5 partly (not committed); Phase 7 deferred (D4) · **Owner:** TBD · **Last updated:** 2026-10-31

---

## 1. Goals and non-goals

**Goals**
- No bulk destructive action runs without a visible plan and confirmation.
- Results view stays at 60 fps with 1M duplicate groups loaded.
- Scan of a 5 TB tree on an external HDD or SSD is measurably faster and does not thrash the drive.
- Scan results can be seen before the scan finishes.
- A first-time user can scan, review, and clean up without reading docs.

**Non-goals (this plan)**
- Similar / perceptual media detection (Phase 7 only, behind a feature flag).
- Cloud sync, background daemon, Mac App Store build.
- Rewriting the egui shell or switching UI framework.

---

## 2. Working rules

1. **One PR per phase**, or one PR per task group inside a phase if a phase exceeds ~800 changed lines.
2. **Commit messages cite IDs:** `fix(results): hold ScanResult in Arc (P-1)`.
3. **Destructive behavior needs a test** that asserts the exact set of paths that would be trashed, before and after the change.
4. **Perf claims need a number** from the Phase 0 harness. "Feels faster" is not acceptance.
5. **Do not start a phase** until the previous phase's acceptance criteria pass.
6. **Ask before removing user-visible controls.** Open decisions (§13) must be answered first.
7. Keep the engine (`src/pipeline.rs`, `src/hasher.rs`, `src/cache.rs`, `src/scanner.rs`) free of egui types. UI code only calls into it.

---

## 3. Phase overview

| Phase | Name | Covers | Blocks | Status |
|-------|------|--------|--------|--------|
| 0 | Baseline & harness | Benchmarks, dataset generator, WIP cleanup | everything perf | ◐ Mostly done (SSD only; WIP not committed) |
| 1 | Data safety | U1, U2, U3, R1, U15 (dialog) | release | ✅ Implemented, tests pass |
| 2 | Results view performance | P-1, P-2, P-3, P-9 | — | ◐ P-1/2/3 done; P-9 deferred |
| 3 | Scan pipeline | P-6, P-7, P-8, P-10, U7, P-13 | — | ✅ Implemented (macOS drive detection; Windows safe default) |
| 4 | Thumbnails & Explorer I/O | P-4, P-5, U13 | — | ◐ Done except EXIF thumbnails |
| 5 | UX clarity | U4, U5, U6, U8–U12, U14, U16 | user copy approval | ◐ Partly (see §16) |
| 6 | Production hardening | R2, R3, R4, R5, R6, R7 | release | ✅ Done (R7 docs updated) |
| 7 | Similar media | P-12 | after 1–3 | ⏸ Deferred (D4 answer: later) |

Phases 1 and 2 can run in parallel after Phase 0 (different files). Phase 3 can start once Phase 0 is done, but its U7 decision must be made first (§13).

---

## 4. Phase 0 — Baseline & harness

**Goal:** every later claim is backed by a number.

### Tasks
- [ ] Commit or stash current WIP — **not done**: changes left in the working tree, nothing committed (commit only on request).
- [ ] Decide what happens to deleted `AGENTS.md` (restore or remove) — **open (D5)**, left untouched.
- [x] Dataset generator `examples/gen_dataset.rs` (no new deps):
  - Seeded, reproducible tree.
  - Mix: JPEG (2–8 MB), RAW-sized (25–60 MB), MP4 (50 MB–4 GB), ZIP (1–500 MB), small files (4–100 KB).
  - Duplicate ratio parameter: 1%, 5%, 20%.
  - Scales to 1M+ files; writes a manifest of expected duplicate groups for correctness checks.
- [x] Scan benchmark `examples/scan_bench.rs` (cold only; cache disabled so the real user cache is never touched). Warm/cache timing waits for Phase 3:
  - Cold cache and warm cache runs.
  - Reports wall time, peak RSS, cache load/save time.
  - Runs on SSD and external HDD (documented mount paths in the bench README).
- [x] UI frame-time probe: `sortwise --bench-ui [groups] [frames]` (in `src/main.rs`).
- [x] Clone cost measured (P-1, release build). Not added to `benches/`; the UI probe covers regressions.
- [x] Fill in the baseline table in §14 (SSD only; external HDD not available on this machine).

### Acceptance
- Baseline table complete for SSD and external HDD at 1M files.
- `cargo test` still passes (currently 10 passing).

---

## 5. Phase 1 — Data safety (P0)

**Goal:** no bulk trash or hard-link without a readable plan and a confirm step. Every action leaves a record.

**Covers:** U1, U2, U3, R1, U15 (shared dialog only).

### Tasks
- [x] **ActionPlan** (pure, no UI):
  - New `src/action_plan.rs`.
  - Input: `&ScanResult`, `ActionMode`, optional group filter.
  - Output: `paths: Vec<PathBuf>`, `total_bytes`, `group_count`, `sample: Vec<PathBuf>` (first 10).
  - Never includes `keep_index` in a trash set.
- [x] **Shared confirm dialog** (in-app modal, not `rfd`):
  - New `src/ui/confirm_dialog.rs`.
  - Shows count, bytes, sample paths, "Moved to Trash — recoverable".
  - Buttons: Cancel (default focus), Confirm.
  - Replaces native `MessageDialog` for bulk actions (U15 partial; per-file actions can keep `rfd` until Phase 5).
- [x] Wire dialog into:
  - `results_panel.rs` `apply_all` (Delete and HardLink modes).
  - `results_panel.rs` `clean_all_same_folder_duplicates`.
- [x] **U2 decision applied (D2: keep excluded):** "Delete All Copies" → renamed "Delete duplicates in group"; excludes `keep_index`. Remove the button entirely if decision D2 (§13) says so.
- [x] **Action log** (R1; "Open Trash" link not added, log path is shown instead):
  - Path: `dirs::data_dir()/sortwise/actions/<timestamp>.json`.
  - Fields: timestamp, mode, planned paths, trashed paths, failed paths with reason, app version.
  - Written on success and on partial failure.
  - Results view shows "Open Trash" and "Open log" links after an action.
- [ ] Reuse manifest serializer from `export_manifest` for the log format.

### Tests
- [x] Unit: ActionPlan for Delete, HardLink, DryRun across keep indices 0..n.
- [x] Unit: no plan ever contains a `keep_index` path.
- [x] Unit: "delete in group" never leaves a group with zero files.
- [ ] Integration (`tests/`): temp directory with duplicates → apply → assert trashed set equals plan. **Not done:** would move real files to the OS Trash; plan-level tests cover the selection set instead.
- [ ] Manual: bulk apply shows dialog; Cancel changes nothing; Confirm writes log. **Pending user check** (GUI not driven here).

### Acceptance
- All tests above pass.
- Manual walkthrough recorded in the PR description.

---

## 6. Phase 2 — Results view performance (P-1, P-2, P-3, P-9)

**Goal:** results panel holds 60 fps with 1M groups.

**Baseline finding:** `last_result.clone()` costs ~110 ms per call at 1M groups / 2M files (release). It runs twice per frame.

### Tasks
- [x] **P-1:** `app.last_result: Option<Arc<ScanResult>>`. Panels clone the `Arc` (µs cost). Update `results_panel.rs` lines 25 and 179, and `app.rs` users at 593, 637, 655 (read-only uses take `&`).
- [x] **P-2:** Virtualize `results_dup_list_panel` with `ScrollArea::show_rows`, fixed row height 84 px. Scroll-to-group uses a computed offset.
- [x] **P-3:** Memoize (group list done; per-group folder grouping still per frame, cheap):
  - Visible index list keyed on `(result_id, filter_hash, sort)`.
  - Invalidate on prune (`prune_paths_from_results`) and on new scan.
  - Per-group folder grouping for the detail pane keyed on `(result_id, group_index)`.
  - Add `result_id: u64` to `ScanResult` (monotonic, set at creation).
- [ ] **P-9 deferred:** `progress_panel.rs` reads only displayed fields. Not done: cloning is cheap compared with the other costs, so no measured reason yet. Avoid cloning `folders` each tick; read under lock and copy the needed scalars.
- [x] Remove the per-frame `max_wasted` scan; computed once with the memoized visible list.

### Tests
- [ ] Unit: memo cache returns same list for same key; invalidates on filter change and on prune. **Partly:** invalidation is by epoch (`set_last_result` and `prune_paths_from_results` bump it); no unit test yet.
- [ ] Unit: `prune` followed by memo read never returns a removed group.
- [ ] Bench: UI probe from Phase 0 with 1M groups.

### Acceptance
- `update()` p95 ≤ 16 ms with 1M groups while scrolling the list.
- `update()` p95 ≤ 4 ms when idle with 1M groups.
- Per-frame path no longer copies result data: panels clone the `Arc` handle only (`results_panel.rs`).

---

## 7. Phase 3 — Scan pipeline (P-6, P-7, P-8, P-10, U7, P-13)

**Goal:** faster, drive-aware, streaming scans with a cache that scales.

**Current facts (verified):**
- `ScanConfig.io_profile` selects the hash reader count in `hash_thread_count` (`pipeline.rs:24`). Wired per D1.
- `io_scheduler` module deleted; the three partial-hash regions now live in `hasher.rs` (`partial_regions`).
- Hash thread count follows `IoProfile` and device type (macOS and Windows drive detection; unknown → 2 readers).
- Full hash skips mmap for removable, network, and sparse files (`hasher.rs`).
- Cache is SQLite (WAL), upserted per file, with a one-time migration from the old blob.
- Hashing streams groups to the UI (`run_dedup_streaming`). Enumeration is still batch (`parallel_scan_roots` returns one `Vec`).

### Tasks
- [x] **U7 decision applied (decision D1, §13):** if wiring, `IoProfile` selects reader count and buffering; if removing, delete the control and `io_scheduler` pool.
- [x] **Device detection** (`src/platform.rs`). macOS verified by tests; Windows compiles, not run on hardware:
  - macOS: `diskutil info -plist <mount>` → `SolidState`.
  - Windows: `DeviceIoControl` with `IOCTL_STORAGE_QUERY_PROPERTY` (seek penalty).
  - Cache result per mount point for the scan.
  - Unknown → treat as HDD (safe default).
- [x] **Thread policy** (`pipeline.rs`): HDD → 1–2 readers; SSD → current 2–6. `IoProfile::Auto` uses detection; manual values override.
- [x] **Buffered reads on removable/network** (`hasher.rs`): skip mmap when device is removable or network. Extend the existing `is_network_path` check.
- [~] **Streaming** (P-6): hashing streams; enumeration still batch. Remaining work in `docs/Handover.md` §7:
  - `parallel_scan_roots` sends size buckets over a channel as they complete.
  - `run_dedup` consumes the stream and hashes buckets as they arrive.
  - Emit partial `ScanResult` snapshots to the UI at a fixed interval (e.g. every 2 s or every 10k groups).
  - Batch mode stays available behind a flag until parity is proven.
- [x] **Cache → SQLite (WAL)** (P-7):
  - Table `hashes(dev, ino, size, mtime, partial_hash, full_hash, PRIMARY KEY(dev, ino))`.
  - Upsert per file; no whole-cache rewrite.
  - One-time migration from existing bincode / legacy JSON (`cache.rs` load path).
  - Atomic by design (transactions); remove the non-atomic `fs::write`.
  - Adds `rusqlite` (bundled). Evaluate load time against bincode before committing.
- [x] **Min size default** 4 KB (P-13) in `app.rs` default. UI selector arrives in Phase 5.

### Tests
- [ ] Parity: streaming and batch modes produce identical duplicate sets on 3 seeded datasets (from Phase 0 generator).
- [ ] Cache migration: bincode and JSON fixtures load and produce the same entries.
- [ ] Cache: kill-during-write test leaves DB readable (SQLite WAL).
- [ ] Platform: `diskutil` parse test with fixture output; Windows detection behind `cfg(windows)` with CI run.

### Acceptance
- Cold scan wall time on external HDD and SSD at 1M files: improvement over Phase 0 baseline by the target set in §14 (target set after baseline is recorded).
- Cache load time at 1M entries reported and within the target set in §14.
- Partial groups visible in UI within 10 s of scan start on 1M-file set.
- Manual: USB drive unplugged mid-scan does not crash the process.

---

## 8. Phase 4 — Thumbnails & Explorer I/O (P-4, P-5, U13)

**Goal:** browsing photo and RAW folders does not spike RAM or block the UI.

### Tasks
- [ ] **Thumbnail decode** (`explorer_panel.rs` `load_image_thumb`):
  - Replace `std::fs::read` + full decode with `image::ImageReader` and decode limits.
  - Use embedded EXIF JPEG thumbnail when present.
  - Use JPEG DCT scaling when available.
- [ ] **Concurrency cap**: bounded queue, max 4 concurrent decodes (`app.rs` thumbnail worker around line 325).
- [ ] **P-5:** `movable_queue_window` uses `ExplorerEntry.size` instead of `fs::metadata` per frame.
- [ ] **U13:** remove `Diagnostic: ...` status message in `ensure_mpv`.

### Tests
- [ ] Unit: thumbnail function returns bounded dimensions for a 6000×4000 fixture.
- [ ] Unit: queue never exceeds 4 in-flight decodes (counter test).

### Acceptance
- Peak RSS browsing a 2,000-image folder stays under the budget set from Phase 0 baseline.
- No frame over 50 ms while thumbnails load (probe).

---

## 9. Phase 5 — UX clarity (U4, U5, U6, U8–U12, U14, U16)

**Goal:** users understand what each mode does and what will happen before they click.

**Gate:** all copy changes (U8) need user approval before merge.

### Tasks
- [ ] **U4** labelled mode tabs (icon + text) in `app.rs` `mode_switch`.
- [ ] **U5** Duplicate Finder click from Explorer shows roots picker; no implicit scan.
- [ ] **U6** Min Size input: unit dropdown (KB / MB / GB) or typed value ("10 MB"); human display, not raw bytes.
- [ ] **U8** rename and merge (after copy approval):
  - "A → B Target Comparison" → "Compare A to B".
  - "Smart Clean" → "Clean same-folder duplicates".
  - "Folder Twins Analysis" → "Folder twins".
  - Movable inbox + Folder Queues → one "Staging" concept.
- [ ] **U9** dry-run banner in results with "Switch to Delete" action (goes back to Scan with mode preselected).
- [ ] **U10** `keep_reason: String` on `FileEntrySnapshot` (set in `resolver.rs`); shown next to keep badge.
- [ ] **U11** type chips in group list: Images, Videos, Archives, Audio. Reuse extension lists in `heuristics.rs`.
- [ ] **U12** progress panel: human units, MB/s, ETA, elapsed time; cancel label "Stop — keep results so far".
- [ ] **U14** icon set: replace emoji and mixed glyphs with one verified set; test on macOS and Windows.
- [ ] **U16** settings persistence: new `src/settings.rs`, versioned JSON in app config dir; loads on start, saves on change (debounced).

### Tests
- [ ] Unit: settings round-trip and version migration.
- [ ] Unit: `keep_reason` present for every group.
- [ ] Walkthrough script (below) runs with no docs open.

### Acceptance
- **Walkthrough:** scan two folders → review a group → apply with dialog → cancel once → apply → open Trash link. Completed by someone who has not read the docs; issues logged.

---

## 10. Phase 6 — Production hardening (R2–R7)

### Tasks
- [ ] **R2** raise `TEXT_TER` to ≥ 4.5:1 on `SURFACE_1`, `SURFACE_2`, `CANVAS_FILL` (`style.rs`). Current value 2.97:1.
- [ ] **R3** keyboard: Delete (trash selected, goes through Phase 1 dialog), Cmd/Ctrl+A select all in results and Explorer, `?` shows shortcut help.
- [ ] **R4** Full Disk Access: scanner records EPERM on protected paths; scan result shows banner with "Open Privacy Settings" (reuse Explorer banner, `explorer_panel.rs:845–856`).
- [ ] **R5** error table: path, reason, copy-to-clipboard; replaces joined `apply_errors` text.
- [ ] **R6** repo hygiene: add `.DS_Store` and `mpv_log.txt` to `.gitignore`; untrack them; restore or remove `AGENTS.md`.
- [ ] **R7** re-audit the status table in [Features.md](Features.md) and [UI-Implementation-Plan.md §10](UI-Implementation-Plan.md) against findings.

### Tests
- [ ] Contrast: script or unit test that computes the ratio for each text/background token pair and fails below 4.5:1.
- [ ] Keyboard: scripted pass of scan → results → apply without mouse (manual, recorded).

### Acceptance
- Contrast test passes in CI.
- Keyboard-only pass recorded in PR.

---

## 11. Phase 7 — Similar media (P-12)

**Start only after Phases 1–3 ship.** Behind `--features similarity`. Results in a separate "Similar" section. Never merged with exact-match groups. Actions limited to Trash with confirm; no hard-link.

### Tasks
- [ ] pHash / dHash for images; decode via Phase 4 thumbnail path; store hashes in SQLite cache from Phase 3.
- [ ] Video keyframe signature via `ffmpeg` (optional runtime dependency). If missing, show notice; do not fail scan.
- [ ] Similar-group UI with per-pair similarity score and threshold slider (default conservative).
- [ ] Action path: Trash-only, dialog from Phase 1.

### Acceptance
- Precision check on a labeled sample of at least 200 pairs, result recorded in this doc.

---

## 12. Risks

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Streaming (Phase 3) changes final results | Wrong duplicate sets | Parity tests on 3 seeds; batch mode stays behind flag until parity holds. |
| SQLite adds a dependency and migration | Cache bugs, upgrade pain | Migration tests with fixtures; keep old cache file for one release. |
| Wrong HDD/SSD detection on USB enclosures | Slow scans or HDD thrash | Unknown → HDD-safe; manual override in settings. |
| Scope creep in Phase 5 copy | Delays | Copy approved in one batch before merge. |
| Confirm dialog fatigue | Users click through | Dialog shows real counts and sizes; single dialog per action, not per file. |
| Phase 0 finds different numbers than §11 estimates | Priorities shift | Re-rank phases from baseline; update §11 tags. |

---

## 13. Open decisions

Each needs an answer before the phase it blocks.

| # | Question | Plan assumes | Blocks | Answer |
|---|----------|--------------|--------|--------|
| D1 | Wire the I/O profile dropdown, or remove it? | Wire it, with detection in Auto | Phase 3 | ☐ |
| D2 | "Delete All Copies": remove, or keep as "Delete duplicates in group" (keep excluded)? | Keep, excluding keep file | Phase 1 | ☐ |
| D3 | Default min file size: 4 KB or 1 KB? | 4 KB | Phase 3 | ☐ |
| D4 | Similar-media in next release, or later? | Later (Phase 7 after 1–3) | Phase 7 | ☐ |
| D5 | Keep `AGENTS.md` deleted, or restore it? | Ask user | Phase 0 | ☐ |

---

## 14. Baseline & targets

Fill in during Phase 0. Targets are set from the baseline, not guessed.

| Metric | SSD, 1M files | External HDD, 1M files | Target (set after baseline) |
|--------|---------------|------------------------|-----------------------------|
| Cold scan wall time (100k files, 3.5 GB, dup 20%) | 4.4–6.9 s total (enumerate 0.16 s, hash+group 4.3–6.7 s) | not run | TBD |
| Warm scan wall time | TBD | TBD | TBD |
| Peak RSS (100k files) | 105–109 MB | not run | TBD |
| Cache load time | not measured (cache disabled in bench) | not run | TBD |
| Cache save time | not measured | not run | TBD |
| UI `update()` p95, 1M groups, scrolling (headless probe) | 0.60 ms (p50 0.55) | not run | ≤ 16 ms |
| UI `update()` p95, 1M groups, idle | 0.60 ms (same probe; idle not measured separately) | not run | ≤ 4 ms |
| Time to first partial groups | not measured (no streaming yet, Phase 3) | not run | ≤ 10 s |

---

## 15. Change log

| Date | Change | Author |
|------|--------|--------|
| 2026-10-31 | Initial plan created from UI-Implementation-Plan §11 findings | TBD |
| 2026-10-31 | Phases 0–2 implemented: action plan + confirm dialog + action log; Arc result, virtualized and memoized group list; examples + UI probe; SSD baseline | TBD |

---

## 16. Status after the final pass (2026-10-31)

### Done
- **Phase 3:** `IoProfile` drives reader count; `Auto` uses 2–6 readers only when every root is solid-state (macOS via `diskutil`, cached per mount). Removable media never uses mmap. Groups stream to the UI while hashing (`run_dedup_streaming`, parity-tested). Cache is SQLite (WAL), incremental upserts, one-time migration from the old blob. Benchmark (100k files, SSD): warm rescan about 1.0 s vs 6 s cold; identical groups and bytes.
- **Phase 4:** thumbnails use the camera's embedded EXIF preview when present, otherwise a streamed decode with an allocation cap; orientation is applied; small images are no longer enlarged. Four decode workers.
- **Phase 5:** copy renames as proposed (Compare A to B, Clean up, Clean same-folder duplicates, Folder twins). One Staging entry point with Items / Folder queues tabs. Labelled mode tabs; Duplicates never scans implicitly; min-size unit picker (default 4 KB); keep reason per group; kind filter chips; progress shows units, elapsed, rate, ETA; scan preferences persist (not action mode or folder list). Icons: every glyph the UI uses now renders in egui's bundled fonts plus Hack (arrows, triangles); the two symbols nothing covers were replaced.
- **Phase 6:** contrast ≥ 4.5:1 on all surfaces; Delete, Ctrl/Cmd+A, `?`; privacy banner; copyable error table; `.gitignore` and index cleanup for `.DS_Store` and `mpv_log.txt`.
- **Layout fixes found by viewing the screens:** results toolbar no longer overlaps; explorer table columns line up and rows fill the width; staging rows no longer squeeze (thumbnail slots were claiming the full row); queue path truncates instead of running under its button; scan settings no longer overflow the window edge; scrollbars appear only when needed.
- **Verification:**
  - 40 unit and integration tests, all passing.
  - Five headless GUI tests that feed real key events through egui frames (Delete, Esc, Ctrl/Cmd+A, `?`, Delete with only the keep file checked).
  - Visual review: `sortwise --snapshot <out.png> [results|scan|explorer|staging|queues]` renders a real screen to PNG via egui's own screenshot, with no OS permissions needed. The screens above were reviewed this way.

### Not done, and why
1. **Enumeration streaming.** Only hashing streams. A size group is only known to be complete once enumeration ends, so streaming enumeration would change which groups appear early. Not worth the parity risk here.
2. **Windows drive detection.** Not written: this machine has no Windows target, so the code could not be compiled or tested. Windows uses the safe 2-reader path.
3. **P-9** (progress struct clone each tick). Cheap; deferred.
4. **EXIF rotation on the full-decode fallback** is applied; the EXIF preview path has a fixture test. No real-camera RAW file was available to test.
5. **Phase 7** (similar media): deferred per D4.
6. **Not checked:** real external HDD timings, unplugging a drive mid-scan, a 5 TB tree, and clicking through every dialog by hand. The screenshots show layout, not behavior under a mouse.
