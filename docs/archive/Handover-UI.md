# Handover — Sortwise GUI redesign

Written for: the next engineer or AI session picking up the GUI work cold.
Date: 2026-11-01. Branch `main`. Last commit `009b982`. **Nothing from this work is committed.** Commit only when the user asks.

Read first:
1. [UI-Redesign-Plan.md](UI-Redesign-Plan.md) — the source of truth. §0 decisions, §6 per-screen spec, §7 phases, §10–22 records of finished work.
2. This file — state, open items, pitfalls.
3. [Handover.md](Handover.md) — the older engine handover. Partly stale (see §9).

---

## 1. Product and naming

- Product name: **Sortwise**. Crate, library and binaries are `sortwise` (app) and `sortwise-cli` (CLI).
- macOS bundle: `Sortwise.app`, bundle ID `com.sortwise.desktop`, icon `packaging/macos/Sortwise.icns`.
- Windows: installer script `packaging/windows/Sortwise.iss`; `scripts/build-windows.ps1`.
- In-app mark: `assets/sortwise-icon.png` (top bar and window icon). Source SVG in `packaging/brand/`.
- Storage folders are `…/sortwise/` (cache, action log, settings). They replaced `…/smart-file-manager/`. Existing settings and action log under the old folder are not read. The old files still exist on disk.
- **Still in `/Applications`:** the old `Smart File Manager.app`. Not removed by any script. Delete by hand if wanted.

## 2. Working agreements (from the user)

- Plain, verified results. Say what was checked and what was not. Do not claim a click-through that did not happen.
- **Every existing feature must keep working.** Loopholes found while redesigning get fixed, and the fix is reported.
- Keep "Trash" as the destructive verb. Keep the Trash confirm flow exactly as it is. Every destructive action goes through `PendingAction::Trash` and the shared confirm.
- Destructive changes need a test that pins the set of paths affected (`tests/action_plan.rs`).
- Do not commit or push unless asked.
- Replies to the user are short and direct (caveman style per `.claude/CLAUDE.md`).
- Install into `/Applications` only when asked. The user has asked for "run the build" / "rebuild" several times; that means the install script.

## 3. Commands

```sh
cargo test                                   # 51 passing
cargo clippy --lib                           # 0 warnings
cargo build --features cli --bin sortwise-cli
cargo check --target x86_64-pc-windows-gnu --lib --bins --tests --features cli   # Windows type-check
cargo run                                    # the app
./scripts/build-macos-app.sh                 # release build, ad-hoc sign, install to /Applications
```

Snapshot tool (visual review; no clicks):

```sh
./target/debug/sortwise --snapshot <out.png> <screen> [--dark] [--size WxH]
```

Screens: `results`, `scan`, `compare`, `compare-scan`, `empty`, `empty-scan`, `archives`, `archives-scan`, `explorer`, `explorer-grid`, `explorer-select`, `explorer-extracted` (archive banner), `player` (media player), `staging`, `staging-sent`, `queues`, `twins`, `confirm`, `shortcuts`, `errors`, `folder`.
Default size 960×720. Minimum window 640×480 (`--size 640x480`). Write scratch files to the session scratchpad, not `/tmp`.

## 4. Plan status

| Phase | Scope | Status |
|---|---|---|
| 0 | Tokens, light default, contrast | Done |
| 1 | Icons, tab bar | Done |
| 2 | Modal template | Done |
| 3 | Explorer | Done |
| 4 | Staging side panel | Done |
| 5 | Duplicates, Compare, Empty folders, Archives | Done (plan §13) |
| 6 | Status bar, archive banner, dark toggle | Done (plan §14) |
| 7 | Windows icon and font check | **Not done.** Manual, needs Windows hardware. Parked by the user. |

Work after the phases (each has a plan section):
- §15 consistency pass: Empty folders and Archives use the same scan and results layout as Duplicates and Compare (`scan_panel.rs` shared pieces, `result_list.rs`).
- §16 staging footer: fixed the crop; "Send all to ▾" then "Move N ready".
- §17 rebrand to Sortwise, with in-app logo.
- §18 media player: no second header, 96% of the window (`modal::show_bare`).
- §19 Empty and Archive results: name-first rows, one toolbar card, singular/plural labels.
- §20 Explorer responsive pass: list columns drop out by width (Kind, then Modified); grid tiles stretch to fill; staging panel covers the window below 900 px.
- §21 codebase rename.
- §22 staging tab order: Folder queues first, and the Staging button opens on it.

## 5. Open items

**Needs a decision or a fix**
- **Phase 7** (Windows check). Manual.
- **`src/ui/mpv_player.rs`** writes an mpv log to a hard-coded developer path. Should be a per-user path.
- **"Move N ready" in staging** runs without a confirm. It moves files, not Trash, so it follows the existing behaviour. Ask the user whether it should confirm.
- **Decisions open:** `AGENTS.md` deleted vs restored (engine; needs the user).

**Cosmetic / known limits**
- At 640 px the preview card's right border sits at or just past the window edge. Cause not found.
- Duplicates results keep their group layout (plan §6.4). Intentional; different task.
- Results summary strip (plan §6.4) and Export in the results bottom bar were not added. Deliberate.
- Empty folders keeps its primary action in the header, not the bottom bar (plan §6.6). Deliberate.

**Not verified by hand** (snapshots only)
- All clicks: the archive banner buttons and its Trash confirm; Empty folders Trash on a real folder; Compare "In A" rows (below the fold in every snapshot); the theme toggle; the status bar's Errors item; the staging "Send all to" and ⋯ menu; grid selection and drag; the toolbar checkboxes on Empty and Archive results; "Choose folder…" and "Move".
- Real Trash moves and Undo from the action log.
- Inline video and the media player's transport with the mpv feature.
- Windows visuals and drive detection.
- Dark theme on every screen (checked on Duplicates results and Explorer only).

**Unreviewed on-disk changes**
Several files were changed on disk during this work by something other than the assistant (notices appeared on `app.rs`, `explorer_panel.rs`, `results_panel.rs`, `main.rs`, `modal.rs`, `style.rs`, `status_bar.rs`, `scan_panel.rs`, `mod.rs`, and the plan and handover docs). They were built and tested with the rest of the work, but nobody has read those changes. Review the whole diff before any commit.

## 6. Pitfalls learned

1. **Panels grow to their content.** A resizable panel follows what its content needs, so a row sized from `available_width()` on the first frame pushes the panel to its maximum. Fix with fixed widths (`exact_width`, `allocate_ui(vec2(w,h))`). The staging panel is fixed-width (340 px beside content, full window below 900 px).
2. **Sub-panels and fixed-width content ignore the parent's margins.** Footers and bodies inside a `SidePanel` ran into the right margin. Add the inset explicitly (`inner_margin` on the `TopBottomPanel`/`CentralPanel`).
3. **`ui.horizontal` centres children vertically.** Mixed-height tiles look misaligned. Use `horizontal_top` for rows of tiles. Use `horizontal_wrapped` where content can exceed the width, or it clips.
4. **A label frame adds its own margin.** An inner `Frame` with `inner_margin` 8 is 16 px wider than its `set_width`. Subtract the margin when sizing the content.
5. **`ui.menu_button` reports a much wider size than its icon.** Use a plain button with `popup_below_widget`.
6. **App-wide button padding is 16 px sideways.** Tighten locally in toolbars.
7. **`set_min_size` / `set_min_height(available_height())` on an unbounded scroll body** grows the window or breaks layout. Pass a fixed height into the body (see `scan_scroll`).
8. **Text colour must match the fill token.** `on_accent` only on strong fills; `*_on_soft` on tints; `text` on surfaces.
9. **Two closures cannot both borrow `app` mutably in one call.** Compute values first, let the closures set flags, act after the call.
10. **Esc is handled in several places.** Guard when another layer owns it.
11. **`Edit` needs a prior `Read` of the file in the session.** Files changed on disk show a notice. Read them before editing.
12. **zsh does not word-split unquoted variables.** Loop with `while read`.
13. **macOS sed needs `sed -i ''`.**
14. **Snapshots are the only visual check.** Clicks, popups and drag need a real mouse.
15. **The sidebar's own width is content-driven too.** The results list sort combo once asked for the panel's maximum on the first frame; now fixed at 200 px.

## 7. Where things are

| Area | File |
|---|---|
| Tokens, theme, buttons, pills, dark toggle | `src/ui/style.rs` |
| Icon names | `src/ui/icons.rs` |
| Modal template (`show`, `show_fill`, `show_bare`) | `src/ui/modal.rs` |
| Brand logo for the top bar | `src/ui/brand.rs`, `assets/sortwise-icon.png` |
| Shell: top tabs, bottom status panel, sidebar placement | `src/app.rs` (`mode_switch`, `update`) |
| Status bar | `src/ui/status_bar.rs` |
| Explorer toolbar, sidebar, list (`table_widths`), grid (`tile_ui`), selection, context menu, archive banner (`extract_banner_ui`), media player (`player_window`) | `src/ui/explorer_panel.rs` |
| Staging panel (`staging_panel`, `staging_width`, `items_body`, footers) | `src/ui/explorer_panel.rs` |
| Folder contents modal | `src/ui/folder_explorer_panel.rs` |
| Duplicates results, Compare results, action bar | `src/ui/results_panel.rs` |
| Duplicates and Compare setup; shared scan pieces (`scan_scroll`, `scan_header`, `scan_columns`, `launch_card`, `render_folder_box`) | `src/ui/scan_panel.rs` |
| Empty folders setup and results | `src/ui/empty_folders_panel.rs` |
| Archives setup and results | `src/ui/archive_files_panel.rs` |
| Shared list results (`list_header`, `selection_toolbar`, `list_card`, `list_row`, `count_label`) | `src/ui/result_list.rs` |
| Twins modal | `src/ui/folder_intelligence.rs` |
| Confirm dialog | `src/ui/confirm_dialog.rs` |
| Plan builders (pure) | `src/action_plan.rs`, tests `tests/action_plan.rs` |
| Snapshot scenes, `--dark`, `--size`, GUI tests | `src/main.rs` (`prepare_screen`, `gui_tests`) |
| Packaging | `packaging/macos/`, `packaging/windows/`, `scripts/build-macos-app.sh` |

## 8. Verification status

Verified by test or snapshot:
- 51 tests passing. Clippy `--lib` 0 warnings. Build 0 warnings. Windows type-check passes. CLI build passes.
- Contrast on every text/surface pair (light and dark).
- Tests added across the work: Empty-folder plan set; extract banner dismisses on navigation.
- Snapshots reviewed: explorer at 1280, 960 and 640; explorer grid at 640 and 1280; list and staging at 960 and 640; queues; folder modal; confirm; shortcuts; error log (status bar shows Errors (2)); twins; results (light and dark); empty and archive scan and results; compare and compare-scan; player; archive banner; scan.

## 9. Stale docs

- `docs/Handover.md` (engine): §4 and §6 are partly out of date. Update when the engine is next committed.
- `docs/Implementation-Plan.md`, `docs/Implementation.md`, `docs/UI-Implementation-Plan.md`, `docs/Features.md`: older history. Trust the code.
- `docs/UI-Redesign-Plan.md`: current. §13–§22 are the latest records.
- `dist/`: generated build output. Do not edit.

## 10. Commits (when asked)

The plan says one commit per phase. The work since the phases is not split into commits yet. Propose a split when the user asks, and review the on-disk changes first (§5).
