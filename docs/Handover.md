# Handover — Sortwise

Written for: the next engineer or AI session picking this repo up cold.
Last updated: 2026-10-31. Branch: `main` (HEAD `009b982`). **Nothing from the last two passes is committed.**

Read first: [Implementation-Plan.md §16](Implementation-Plan.md) (what was done and why), then [UI-Implementation-Plan.md §11](UI-Implementation-Plan.md) (finding IDs: U1, P-1, R2…). Finding IDs are stable; cite them in commits.

---

## 1. Project in one paragraph

Rust desktop app (egui/eframe 0.29) that finds duplicate files (exact, BLAKE3) and also browses files (Explorer). Crate `sortwise`: a library (`src/lib.rs`, the engine, no egui) and a binary (`src/main.rs` + `src/ui/*`, the GUI). Also `sortwise-cli` (feature `cli`).

## 2. Repo state

| Item | State |
|------|-------|
| Commits | Nothing new committed. Working tree holds all work. |
| Index | `.DS_Store`, `packaging/.DS_Store`, `mpv_log.txt` are **staged as deleted** (`git rm --cached`). Review before committing. |
| `AGENTS.md` | **Deleted in working tree, undecided** (decision D5). Restore or remove deliberately. |
| `.claude/` | Untracked local config. Do not commit unless intended. |
| Tests | `cargo test`: **40 passing** (8 lib, 18 bin, 10 action_plan, 3 dedup_integration, 1 streaming_parity). |
| Build | `cargo build` and `--release` clean, zero warnings at last check. |
| Platform verified | macOS (aarch64) only. Windows is **not compiled or tested**. |

Commit only when asked. When committing, use the attribution lines given in the session reminder.

## 3. Commands

```sh
cargo test                                   # all tests
cargo run                                    # the app
cargo build --release --examples             # benchmarks and dataset tool

# Visual review (no screen-recording permission needed)
./target/debug/sortwise --snapshot out.png results   # also: scan, explorer, staging, queues

# UI frame-time probe (headless)
./target/release/sortwise --bench-ui 1000000 300

# Dataset and scan benchmark
./target/release/examples/gen_dataset <out_dir> <files> <dup_ratio> <max_kb> [seed]
SFM_BENCH_CACHE=<scratch.sqlite> ./target/release/examples/scan_bench <root>
```

Scratch data goes in the session scratchpad, not the repo. Do not point `scan_bench` at a real user folder without thinking about it: it reads the whole tree.

## 4. Architecture map

**Engine (no egui):**
- `scanner.rs` — parallel enumeration (`parallel_scan_roots`).
- `pipeline.rs` — size buckets, partial then full BLAKE3 hash. `run_dedup` (batch) and `run_dedup_streaming` (groups sent as found). Thread count from `IoProfile` + drive type (`hash_thread_count`).
- `hasher.rs` — partial and full hashing. mmap only off removable and network paths.
- `resolver.rs` — keep-file scoring (`score_entry`), `keep_reason`, trash and hard-link helpers.
- `action_plan.rs` — **pure** planning of bulk actions (`for_apply`, `for_selected`, `for_group`, `for_folder`, `for_same_folder`). Invariant: never includes a group's keep file.
- `action_log.rs` — JSON record per bulk action in the data dir.
- `cache.rs` — SQLite (WAL) hash cache, incremental upserts, one-time migration from the old `hash_cache.bin`.
- `platform.rs` — `volume_info` (macOS via `diskutil`, cached per mount; Windows via `GetDriveTypeW` and `IOCTL_STORAGE_QUERY_PROPERTY`, cached per drive letter; compiles, not run on Windows hardware), `is_removable`, `file_id`.
- `settings.rs` — versioned JSON scan preferences. Action mode and folder list are **deliberately not persisted**.
- `heuristics.rs` — `media_kind` (Images, Videos, Audio, Archives, Other) and dedupe-candidate extensions.

**GUI (`src/ui/`, bin crate):**
- `app.rs` (crate root `src/app.rs`) — `SortwiseApp` state, `update()`, mode tabs, staging button, `set_last_result` (bumps `results_epoch`, clears pending action), `show_live_results`, settings load and save.
- `results_panel.rs` — duplicate results: group list (memoized, virtualized), group detail, toolbar, keyboard shortcuts, error table, executors (`execute_pending_action`, `trash_plan`, `apply_hardlinks`).
- `confirm_dialog.rs` — the single confirm for every bulk action. Esc or Cancel discards the plan.
- `explorer_panel.rs` — file browser (list and grid), staging windows (`movable_queue_window` = Items tab, `queue_window` = Folder queues tab), thumbnails (`load_image_thumb`, EXIF path), `table_widths` and `column` helpers.
- `scan_panel.rs`, `progress_panel.rs` (human units, ETA), `style.rs` (colours, fonts: bundled egui fonts + Hack fallback + OS emoji fallback), `preview.rs` (`open_privacy_settings`, `is_privacy_denied`, `format_bytes`).
- Other panels: `empty_folders_panel.rs`, `archive_files_panel.rs`, `folder_intelligence.rs` (twins), `folder_explorer_panel.rs`, `video_inline.rs`, `mpv_player.rs` (feature `mpv`, off by default).

**Key invariants (do not break):**
1. Every bulk trash goes through `PendingAction` → `confirm_pending_action` → `execute_pending_action`. Never call `trash_batch` directly from a button.
2. `ActionPlan` never contains the keep file. Test: `tests/action_plan.rs`.
3. `last_result` is `Option<Arc<ScanResult>>`. Change it only through `set_last_result` (or `Arc::make_mut` for pruning), so the epoch and pending action stay in sync.
4. Engine code has no egui types. The streaming and batch results must match (`tests/streaming_parity.rs`).

## 5. Decisions taken (and their status)

| # | Decision | Taken | Note |
|---|----------|-------|------|
| D1 | Wire I/O profile (not remove) | Wired | Auto is SSD-only for 2–6 readers. |
| D2 | Keep file never trashed; "Delete Duplicates" label | Applied | Enforced in every trash path. |
| D3 | Min size default 4 KB | Applied | `ScanConfig::default` and app default. The CLI sets its own value; check `src/bin/cli.rs`. |
| D4 | Similar-media later | Deferred | Phase 7 not started. |
| D5 | `AGENTS.md` deleted vs restored | **Open** | Ask the user. |

Copy renames (plan §9, U8) were applied because the user said "do it". Ask if they want any reverted.

## 6. What is verified and what is not

**Verified by tests:** planning rules, keep-file rule, group and folder delete selections, streaming parity, SQLite cache (reopen, dirty-only save, migration), settings (round trip, corrupt file, partial file), EXIF thumbnail with orientation (fixture `tests/fixtures/exif_thumb_rotated.jpg`), thumbnail size bounds, progress formatting, memo invalidation, keyboard shortcuts and Esc via headless frames (`src/main.rs` `gui_tests`).

**Verified by screenshot (layout only):** results, scan, explorer list, staging Items, folder queues.

**Benchmarks (macOS, SSD, 100k files, 3.5 GB, 20% dupes):** cold ~6–7 s; warm cache ~1.0 s; first groups ~0.06 s into hashing; cache load ~70 ms. UI probe: ~0.54 ms p50 per frame at 1M groups.

**Not verified:**
- Clicking through dialogs and windows with a real mouse.
- Actual trash moves to the OS Trash (tests stop at the plan).
- External HDD timings, unplugging a drive mid-scan, a 5 TB tree.
- Windows drive detection on real hardware (HDD, SSD, USB flags). The Windows target type-checks and CI `windows-latest` builds and tests, but nothing has run on a Windows machine with real drives.
- Linux builds.
- Real camera RAW thumbnails (only a generated fixture).

## 7. Known gaps and next steps (priority order)

1. **Review and commit.** Split into logical commits by phase (suggested split in §8 below). Confirm the staged deletions are intended.
2. **Manual GUI pass** on macOS: scan two folders, review, staged Delete with Cancel and with Confirm, check the Trash, Undo-by-Trash. Check the Staging tabs.
3. **Decide D5** (`AGENTS.md`).
4. **Windows drive flags:** run the app on Windows with an HDD, an SSD, and a USB drive. Confirm the seek-penalty and removable results. Code is written and type-checks; CI `windows-latest` builds and tests.
5. **Enumeration streaming** (plan P-6, partial). Deferred on purpose: a size group is only complete when enumeration ends.
6. **P-9** progress struct clone. Cheap, low value.
7. **Phase 7 similar media** (perceptual hashing) — only after 1–3, behind `--features similarity`.
8. **Icons** (U14): glyphs render in egui's bundled fonts plus Hack. Visual check on Windows still pending (Windows also adds Segoe emoji).
9. **Old panels not reviewed visually:** empty folders, archive files, folder twins modal, A→B compare results. Snapshot scenarios for them do not exist yet; add to `prepare_screen` in `src/main.rs`.
10. **Clippy:** engine lib at zero warnings. GUI binary has about 10 style warnings left, not addressed.

## 8. Suggested commit split (if asked)

1. Engine: `action_plan`, `action_log`, `cache` (SQLite), `platform` volume info, `pipeline` streaming and thread policy, `settings`, `resolver` keep reason, `heuristics::media_kind`.
2. Results UX and safety: `results_panel`, `confirm_dialog`, `app` state and staging button, keyboard shortcuts.
3. Performance: `Arc` result, virtualized and memoized group list, progress.
4. Explorer and visuals: `explorer_panel` (table, thumbnails, staging rows), `style` (fonts, contrast), `scan_panel`, `preview`.
5. Tooling and docs: `examples/`, `--snapshot`, `--bench-ui`, `docs/*`, `.gitignore`, index cleanup.

## 9. Working agreements from this session

- The user wants plain, verified results. Say what was checked and what was not.
- Ask before removing user-visible controls or changing copy beyond what the plan already lists.
- Do not commit unless asked. Do not push.
- Destructive behaviour changes need a test that proves the set of paths affected.
- The user's `CLAUDE.md` (in `.claude/`) asks for minimal, caveman-style replies. Write docs for the audience they are for, not for the chat.

## 10. Where things are on disk (this machine)

- Repo: `/Users/rishiwork/Desktop/New Folder With Items 2/ai-agents/file-manager`
- Demo library created by `--snapshot`: `$TMPDIR/sfm_demo_library` (safe to delete).
- Hash cache: `~/Library/Caches/sortwise/hash_cache.sqlite` (plus legacy `hash_cache.bin`, kept as fallback).
- Settings: `~/Library/Application Support/sortwise/settings.json`.
- Action logs: `~/Library/Application Support/sortwise/actions/`.
- Session scratchpad (benchmarks, screenshots, 3.5 GB synthetic dataset): `/private/tmp/claude-501/-Users-rishiwork-Desktop-New-Folder-With-Items-2-ai-agents-file-manager/a71b0c8e-e23b-4d20-93a5-ce890dcb4271/scratchpad/`. Temporary; delete the dataset when done.
