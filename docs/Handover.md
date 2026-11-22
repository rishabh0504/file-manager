# Handover — Sortwise

Written for: the next engineer or AI session picking this repo up cold.
Last updated: 2026-11-23. Branch: `main`. Older plans and write-ups are in `docs/archive/` (do not follow them).

---

## 1. What the product is for

Sortwise is a macOS-first Rust desktop app (egui/eframe 0.29) for **gathering one person's photos and
videos that are scattered across many locations, moving them into organized folders, then removing the
extra copies**, without ever losing a file.

The workflow it is built around:

1. **Browse / All files** — find the scattered media.
2. **Stage** — put files into folder queues (one queue = one destination folder) or loose Items.
3. **Move** — submit the queues. If the destination already has the same name, **both files are kept**
   and the incoming one is renamed (`clip_1.mp4`, `clip_2.mp4`; folders `Album_2`). Duplicates are not
   resolved at move time.
4. **Duplicates** — scan the organized folders; exact duplicates are found by BLAKE3.
5. **Trash** — remove the extra copies. **Delete always means move to the system Trash. This is
   intentional; never add permanent delete.**

Other tools: Compare A→B (copies in B that exist in A), Empty folders, Archives, folder twins.

## 2. Non-negotiable safety rules

1. Never overwrite on move. Collisions get a unique name (`unique_file_dest`, `unique_dir_dest` in
   `src/app/fs_ops.rs`). Names that are not valid UTF-8 keep their real bytes.
2. A folder can never move into itself (`fast_move_dir` refuses it; the copy fallback would otherwise
   delete both). A failed cross-volume copy removes its partial destination; the source is deleted only
   after the copy fully succeeded.
3. Every duplicate group keeps at least one copy. `ActionPlan` never contains the keep file
   (`tests/action_plan.rs`). Folder twins protect the keep copy when both sides are selected, and the
   per-file twin trash refuses the last copy.
4. Before trash or hard-link, `stale_duplicates` (`src/ui/results_panel.rs`) skips any copy whose kept
   file is missing or changed size since the scan. The user is told to run Refresh findings.
5. Every bulk trash goes through `PendingAction` → `confirm_pending_action` → `execute_pending_action`.
   Never call `trash_batch` from a button.
6. Failed moves stay listed (queues, Items, archive list) so the user can retry. Nothing silently disappears.
   A file is staged in exactly one place (one folder queue or Items), so it can never be moved twice
   (`unstage_elsewhere`, `explorer_files_stage`).
7. Long work never runs on the UI thread: scans, queue submit, Items moves (`start_movable_move`), archive
   moves, recursive search and zip extract all use workers. Every poller handles
   `TryRecvError::Disconnected`, so a crashed worker unblocks the UI and reports instead of spinning forever.
8. Every trash asks first (confirm dialog), except the player window's quick-trash button, which is
   deliberate fast triage while flipping through media. Both go to the system Trash.

Any change to move, trash or hard-link code needs a test that proves which paths are touched.

## 3. Repo state

| Item | State |
|------|-------|
| Build | `cargo build --bin sortwise`: 0 errors, 0 warnings |
| Tests | `cargo test`: 78 passing, 1 ignored (the real cross-volume test, see §4) |
| Clippy | `cargo clippy --all-targets`: 0 warnings |
| Platform verified | macOS (aarch64) only. Windows and Linux not run. |
| Branches | Only `main` matters. `recovery/working-baseline`, `ui-rewrite`, `app-split`, `audit-fixes` are fully merged and can be deleted. |

### History you need to know

Around 2026-11-22 a series of commits (`442120e` … `267e562`) split `app.rs` and added keyboard,
notification and operation-history modules. **They broke the binary (176 compile errors).** Earlier checks
only ran `cargo build --lib`, which does not compile `src/app*` or `src/ui/` — so the breakage went
unnoticed. The fix:

- `96c1e60` restored the last working tree (`6d8acda`) plus the "All files" WIP, with `app.rs`
  reconstructed from session logs (about 3 lines may differ from the lost original).
- `a58f8d2` merged the broken history in without its code (`-s ours`), so `main` fast-forwarded.
- UI rewrite (`af3ff06`, `4b87ae8`, `92c6ba5`), `app.rs` split (`b3acb1a`), bug audit (`3d37f5b`).

**Always verify with the binary build, never `--lib` alone.**

## 4. Commands

```sh
cargo build --bin sortwise 2>&1 | grep -c '^error'   # must be 0 — the only build check that covers the UI
cargo test                                           # all tests (64)
cargo clippy --all-targets                           # must be 0 warnings
cargo test --bin sortwise cross_volume -- --ignored  # real move onto a FAT32 disk image (macOS, hdiutil)
cargo run --bin sortwise                             # the app
cargo run --bin sortwise -- --dark                   # dark theme

# Screenshots (no screen-recording permission needed). Writes the PNG and exits.
cargo run -q --bin sortwise -- --snapshot out.png <screen> --size 1280x800
#   screens: results (default), scan, explorer, staging, queues, empty, archives, compare,
#            player, twins, confirm, shortcuts, errors, folder, files
#   narrow layout check: --size 820x640

# UI frame-time probe (headless). 2026-11-23: p50 0.53 ms, max 1.77 ms at 1M groups
./target/release/sortwise --bench-ui 1000000 300

# Scan benchmark. 2026-11-23, 20k files / 2.5 GB: exact expected groups, warm cache 0.12 s, ~46 MB RSS
cargo build --release --examples
./target/release/examples/gen_dataset <out_dir> 20000 0.2 256 7
SFM_BENCH_CACHE=<scratch.sqlite> ./target/release/examples/scan_bench <out_dir>
```

Scratch data goes in the session scratchpad, not the repo.

## 5. Architecture map

**Engine — library crate `sortwise` (`src/lib.rs`), no egui:**
- `scanner.rs` — parallel enumeration, empty-folder and archive finders.
- `pipeline.rs` — size buckets → partial BLAKE3 → full BLAKE3. `run_dedup_streaming` sends groups as found.
- `hasher.rs`, `cache.rs` (SQLite hash cache keyed by path + size + ctime), `verifier.rs`.
- `resolver.rs` — keep-file scoring, `trash_batch` (batch, then per-file fallback; already-gone counts as
  trashed), `atomic_hardlink`, `apply_hardlink_group`.
- `action_plan.rs` (pure planning), `action_log.rs` (JSON log per bulk action), `settings.rs`,
  `platform.rs`, `heuristics.rs`, `recursive_files.rs` (All files listing).

**App state — binary crate (`src/main.rs`, `mod app; mod ui;`):**
- `src/app.rs` (426 lines) — imports, the `SortwiseApp` struct (one flat state struct) and `Default`.
- `src/app/types.rs` — `AppMode`, `AppView`, explorer/files/queue enums and structs, constants.
- `src/app/scan.rs` — start/poll for duplicates, refresh, empty folders, archives; archive move.
- `src/app/staging.rs` — folder queues, movable Items, settings persistence, queue submit.
- `src/app/explorer.rs` — navigation, history, recursive search, listing, sort, new folder, zip extract.
- `src/app/files.rs` — All files tab listing and staging.
- `src/app/preview.rs` — preview and folder modal, `set_last_result`.
- `src/app/ui_loop.rs` — `impl eframe::App`: per-frame polling and the window layout.
- `src/app/fs_ops.rs` — `fast_move`, `fast_move_dir`, unique destinations, search collapsing.
- `src/app/tests.rs` — app-level tests.

**UI (`src/ui/`):**
- `style.rs` — the design system: `Palette` (light + dark only, no gradients), tokens `R_SM/R_MD/R_LG`,
  `H_CONTROL` (30), `H_TOOLBAR` (48), `NARROW_W` (900), `is_narrow()`, widget helpers
  (`primary_button`, `secondary_button`, `danger_button`, `ghost_button`, `chip`, `icon_button`,
  `card`, `panel_card`, `stat_chip`, `pill`, `eyebrow`, `section_title`, `muted`).
  **No hard-coded colours outside this file.**
- `shell.rs` — left nav sidebar (Browse / Clean up groups; Places in Explorer), icon rail below 900 px or
  when staging is open on windows under 1200 px; top toolbar with page title and Staging button.
- `explorer_panel.rs` (largest file) — toolbar, breadcrumb, table/grid, staging panel, player window.
- `scan_panel.rs` — shared scan page pieces (`scan_header`, `scan_columns` stacks below 720 px,
  `render_folder_box`, `launch_card`) used by Duplicates, Compare, Empty folders, Archives.
- `results_panel.rs` — duplicate results, executors (`trash_plan`, `apply_hardlinks`, `stale_duplicates`).
- `status_bar.rs` — location, counts, Explorer action messages, reclaimable size, theme toggle.
- Others: `recursive_files_panel.rs`, `folder_explorer_panel.rs`, `folder_intelligence.rs` (twins),
  `confirm_dialog.rs`, `modal.rs`, `progress_panel.rs`, `empty_folders_panel.rs`,
  `archive_files_panel.rs`, `preview.rs`, `video_inline.rs`, `mpv_player.rs` (feature `mpv`).

**Invariants:** `last_result` changes only through `set_last_result` (or `Arc::make_mut` when pruning);
`prune_paths_from_results` follows the keep file to its new index; engine code has no egui types; batch
and streaming results must match (`tests/streaming_parity.rs`).

## 6. How to change UI safely

1. Presentation only: keep every `.clicked()` / `.changed()` call and the business call it triggers.
2. Build the binary, run tests, take light, dark and narrow (`820x640`) snapshots of every touched screen
   and look at them.
3. Use `style.rs` tokens and helpers; never `Color32::from_rgb` in a panel.
4. Prefer the Edit tool with exact matches. Do not rewrite Rust files with regex scripts.

`docs/archive/UI_REWRITE_PLAN.md` is the plan the rewrite followed; it is done and kept for reference.

## 7. Verified and not verified

**Verified:** build, tests and clippy above; real cross-volume moves (file + nested folder onto FAT32,
keep-both rename, source removed); scan correctness and speed on 20k generated files; UI frame time at
1M groups; the CLI on a real folder; screenshots of every screen in light, dark, and at 700 / 1000 /
1280 px wide.

**Not verified:**
- Clicking through the app with a real mouse.
- Real trash to the OS Trash (tests cover the plans and the safety filters, not the Trash itself).
- Very large real libraries (hundreds of GB of video) and slow USB hard drives.

## 8. Known gaps and next steps (priority order)

1. **Manual pass on macOS** with real folders: stage → submit with a name collision (expect `name_1`),
   run Duplicates, trash with confirm, check the Trash, try folder twins with both sides ticked.
2. **Metadata-only edits are matched** for JPEG and MP4/MOV/M4V/3GP (`src/media_hash.rs`, scan setting
   "Match photos/videos with different metadata", on by default): only the encoded media bytes are hashed
   (MP4 `mdat`; JPEG minus APPn/COM). Such groups have hash prefix `media-`, show "metadata differs", and are
   never hard-linked (that would drop the copy's tags). **Not matched:** re-encoded or re-compressed files
   (different media bytes, e.g. `sips` edits re-compress JPEGs), HEIC, MKV/AVI. Those need perceptual
   "similar media" matching (`--features similarity`, not started).
3. A folder and a file inside it can both be staged; the folder moves first and the file then fails
   (reported, stays staged). Harmless but noisy.

## 9. Working agreements

- Plain, verified results. Say what was checked and what was not.
- The user does not want confirmation questions for safe, reversible work. Ask only for destructive or
  shared actions (pushing, deleting branches) unless told to go ahead.
- Commit only when asked; push only when asked.
- The user's `.claude/CLAUDE.md` asks for terse chat replies. Write docs for their audience, not the chat.
- `.claude/settings.local.json` (git-ignored) allows edits, cargo and safe git without prompts and denies
  `rm -rf`, `kill`, force-push, `reset --hard`, `git clean`.
- Never kill or stop the user's running processes.

## 10. Where things are on disk (this machine)

- Repo: `/Users/rishiwork/Desktop/New Folder With Items 2/ai-agents/file-manager`
- Demo library created by `--snapshot`: `$TMPDIR/sfm_demo_library` (safe to delete).
- Hash cache: `~/Library/Caches/sortwise/hash_cache.sqlite`.
- Settings: `~/Library/Application Support/sortwise/settings.json`.
- Action logs: `~/Library/Application Support/sortwise/actions/`.
