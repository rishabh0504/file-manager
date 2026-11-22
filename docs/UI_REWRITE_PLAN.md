# Sortwise UI Rewrite — Execution Plan

Audience: the agent that will do this work (Claude Haiku). Follow it literally. When something in the code
contradicts this plan, stop and report; do not improvise around it.

Goal: make the app look like a professional macOS utility (reference feel: Finder, Linear, Raycast) by
rewriting **layout and presentation only**. Every existing feature, button, shortcut, menu item, dialog and
behaviour must still exist and still work after each step.

---

## 0. Ground rules (read twice)

### 0.1 Branch and baseline
- Work on branch `ui-rewrite`, created from `recovery/working-baseline`.
  `main` is broken (176 compile errors) — never base work on it, never merge into it, never push.
- Baseline facts (verified): `cargo build --bin sortwise` → 0 errors; `cargo test` → all 61 tests pass.

### 0.2 The only valid verification
`cargo build --lib` does **not** compile `src/app.rs` or `src/ui/` (they belong to the binary).
A green `--lib` build proves nothing about UI work. After **every** step run all three:

```bash
cargo build --bin sortwise 2>&1 | grep -c '^error'   # must print 0
cargo test 2>&1 | grep -E 'test result|FAILED'        # every line "ok", no FAILED
cargo build --bin sortwise 2>&1 | grep -c '^warning'  # must not be higher than before the step
```

If any check fails, fix it before doing anything else. If you cannot fix it in 3 attempts:
`git restore --source=HEAD -- <files you touched>` and report the problem. Never commit a red build.

### 0.3 Visual verification (mandatory)
The app has a built-in screenshot mode. For each screen you touched, capture before and after, then open the
PNG with the Read tool and look at it:

```bash
mkdir -p /tmp/sortwise-shots
cargo run -q --bin sortwise -- --snapshot /tmp/sortwise-shots/<screen>-after.png <screen> --size 1280x800
cargo run -q --bin sortwise -- --dark --snapshot /tmp/sortwise-shots/<screen>-dark.png <screen> --size 1280x800
cargo run -q --bin sortwise -- --snapshot /tmp/sortwise-shots/<screen>-narrow.png <screen> --size 820x640
```

Screens: `results` (default), `scan`, `explorer`, `staging`, `queues`, `empty`, `archives`, `compare`,
`player`, `twins`, `confirm`, `shortcuts`, `errors`, `folder`. Step 1 adds `files`.
Check every screen in light mode, dark mode and the narrow size: no clipped text, no overlapping widgets,
no horizontal overflow, nothing invisible in dark mode.

### 0.4 Feature-preservation guard
Before editing a UI file, record its interaction counts; after editing, the counts must be **≥** the old ones.
(A count can only go down if you merged two identical controls; say which ones in the commit message.)

```bash
f=src/ui/<file>.rs
echo "clicked=$(grep -c 'clicked()' $f) changed=$(grep -c '\.changed()' $f) ctx=$(grep -c context_menu $f) keys=$(grep -c 'key_pressed\|Key::' $f) hover=$(grep -c on_hover_text $f)"
```

Baseline counts (clicked / changed / context_menu / keys / hover):

| file | clicked | changed | ctx | keys | hover |
|---|---|---|---|---|---|
| explorer_panel.rs | 72 | 6 | 1 | 19 | 37 |
| results_panel.rs | 32 | 1 | 0 | 4 | 8 |
| folder_explorer_panel.rs | 20 | 0 | 1 | 3 | 11 |
| folder_intelligence.rs | 17 | 4 | 0 | 0 | 5 |
| recursive_files_panel.rs | 11 | 1 | 0 | 0 | 9 |
| scan_panel.rs | 5 | 1 | 0 | 0 | 4 |
| archive_files_panel.rs | 3 | 1 | 0 | 0 | 0 |
| empty_folders_panel.rs | 3 | 0 | 0 | 0 | 1 |
| confirm_dialog.rs | 3 | 0 | 0 | 0 | 0 |
| progress_panel.rs | 2 | 0 | 0 | 0 | 0 |
| status_bar.rs | 2 | 0 | 0 | 0 | 2 |
| result_list.rs | 1 | 1 | 0 | 0 | 0 |
| modal.rs | 1 | 0 | 0 | 1 | 1 |
| app.rs | 2 | 0 | 0 | 6 | 2 |

When a control moves to a new kit widget, the guard pattern may change (for example `kit::button(...).clicked()`
still matches `clicked()`). Keep calls in the form `<widget>.clicked()` so the guard keeps working.

### 0.5 What you may and may not touch
**May edit:** `src/ui/**`, the `update()` layout section and `mode_switch()` in `src/app.rs`
(lines from `let mode = self.mode;` to the end of `mode_switch`), `prepare_screen()` in `src/main.rs`
(only to add snapshot screens).

**Must not edit:** anything else in `src/app.rs` (struct fields, `Default`, methods, background threads,
polling), `src/operations/**`, and every engine file in `src/*.rs` (scanner, pipeline, cache, hasher, resolver,
heuristics, verifier, settings, types, action_*, cdc, similarity, excludes, platform, errors, recursive_files,
lib). If a UI change seems to need a new field or method on `SortwiseApp`, stop and report instead.
Exception: a pure view-state field (for example `sidebar_collapsed: bool`) may be added **only** if the step
below says so explicitly.

### 0.6 Editing discipline
- Use the Edit tool with exact `old_string`. **Never** use Python/sed/regex scripts to rewrite Rust files.
  (That is how the previous attempt destroyed `app.rs`.)
- Read a function fully before rewriting it.
- Keep business calls exactly as they are: same function, same arguments, same order relative to state reads.
  Only the widgets around them change.
- No new crates. `egui 0.29`, `eframe 0.29`, `egui-phosphor 0.7` and `image` are what you have.
- No hard-coded colours outside `src/ui/style.rs` (today there are none; keep it that way).
- Do not write status/summary markdown files. The only doc you update is this one (tick the checklist).
- Do not run the GUI except in `--snapshot` mode. Never kill processes.

### 0.7 Commits
One commit per step, message `ui: <step id> <summary>`, ending with the attribution line the harness gives.
Commit only after all §0.2 checks pass and the §0.3 screenshots look right.

---

## 1. Target design

### 1.1 App shell (replaces the six cramped header tabs)

```
┌──────────────┬───────────────────────────────────────────────────────────────┐
│ ▣ Sortwise   │  Page title            [contextual actions …]   [Staging · 3] │  ← toolbar 48px
│──────────────│───────────────────────────────────────────────────────────────│
│ BROWSE       │                                                               │
│  Explorer    │                                                               │
│  All files   │                 page content (CentralPanel)                   │
│ CLEAN UP     │                                                               │
│  Duplicates  │                                                               │
│  Compare A→B │                                                               │
│  Empty fold. │                                                               │
│  Archives    │                                                               │
│──────────────│                                                               │
│ PLACES       │   (only in Explorer / All files; Duplicate results keep their │
│  Home …      │    own group list as a second column inside the page)         │
│──────────────│                                                               │
│ ◐ theme      │                                                               │
├──────────────┴───────────────────────────────────────────────────────────────┤
│ status bar 24px: left = context text · right = reclaimable / counts           │
└──────────────────────────────────────────────────────────────────────────────┘
```

- **Left nav sidebar** (`SidePanel::left("nav")`, 220 px, resizable 180–280). It is shown in every mode.
  Below 900 px window width, or when the user collapses it, it becomes a 56 px icon rail (tooltips keep the names).
- Nav items map 1:1 to today's `mode_switch` branches, with **identical side effects** (copy the bodies:
  e.g. Files sets `mode = AllFiles`, `view = Scan`, and fills `files_view_dir` if `None`;
  Duplicates does not start a scan).
- The **Places** list (today `explorer_sidebar`) moves under the nav, shown only in Explorer and All files.
  "Open folder…" stays with it.
- **Top toolbar** (`TopBottomPanel::top`, 48 px): page title on the left, page-specific actions, and the
  Staging button on the right (same logic and visibility rule as today: Explorer and AllFiles only).
- **Duplicate / Compare results:** the group list that is in `SidePanel::left("sidebar")` today becomes a
  second resizable `SidePanel::left("groups")` placed after the nav panel (egui stacks them), default 300 px.
- The staging `SidePanel::right` and the modals/windows stay where they are; they are only restyled.
- Theme toggle moves from the status bar to the bottom of the nav; keep the status bar one also if removing
  it would drop a `clicked()` (guard).

### 1.2 Design tokens (`src/ui/style.rs`)
Keep the `Palette` struct and `pal()` API (every panel uses them). Add to it; do not rename existing fields.

- Neutral scale (light): canvas `#F5F5F7`, surface `#FFFFFF`, surface_raised `#F0F0F3`, border `#E4E4E8`,
  border_strong `#C9C9D1`, text `#18181B`, text_muted `#6B6B76`.
  Dark: canvas `#111113`, surface `#18181B`, surface_raised `#222226`, border `#2E2E33`,
  border_strong `#45454D`, text `#EDEDF0`, text_muted `#9A9AA6`.
- Accent: keep blue. Light `#2563EB`, accent_soft `#EEF3FE`; dark `#3B82F6`, accent_soft `#1C2740`.
- New fields: `sidebar` (nav background: light `#EFEFF2`, dark `#141416`), `hover` (light `#EBEBEF`,
  dark `#26262B`), `focus_ring` (= accent at 60% alpha).
- Spacing constants (`pub const`): `SP_1=4, SP_2=8, SP_3=12, SP_4=16, SP_5=24, SP_6=32`.
- Radius: `R_SM=6` (inputs, buttons, chips), `R_MD=8` (cards, rows), `R_LG=12` (modals).
- Type scale: caption 11, small 12, body 13, body_strong 13 strong, title 15 strong, h2 18 strong, h1 22 strong.
  Set them in `configure_app_style` via `style.text_styles`, and add a `Heading2` custom style if needed.
- Control heights: `H_CONTROL=30` (buttons, inputs, combos), `H_ROW=32` (list/table rows), `H_TOOLBAR=48`.
- `configure_app_style`: button padding `(10,5)`, `interact_size.y = H_CONTROL`, item_spacing `(8,6)`,
  widget rounding `R_SM`, window rounding `R_LG`, a visible 1 px border on inactive widgets
  (fixes the borderless text inputs), hovered bg = `hover`, and the focus stroke = `focus_ring`.

### 1.3 Widget kit (`src/ui/kit.rs`, new; re-exported from `ui/mod.rs`)
Build these once. Every panel uses them instead of ad-hoc `egui::Button::new(...).fill(...)`.

| fn | purpose |
|---|---|
| `button(ui, text) -> Response` | secondary: surface fill, 1 px border, text colour. Not grey-on-grey. |
| `button_primary(ui, text)` | accent fill, on_accent text |
| `button_danger(ui, text)` | danger fill (danger_soft + danger_on_soft when not destructive-final) |
| `button_ghost(ui, text)` | no fill, hover bg only (toolbar actions) |
| `icon_button(ui, icon, tooltip, active) -> Response` | 30×30, ghost; `active` = accent_soft fill |
| `text_input(ui, &mut String, hint, icon: Option<&str>) -> Response` | bordered, leading icon, H_CONTROL tall |
| `segmented(ui, &mut T, &[(T, &str)]) -> bool` | one joined control, used for filters that wrap today |
| `chip(ui, text, selected) -> Response` | filter chip, never wraps text |
| `badge(ui, text, Tone)` | small count/state label (Tone = Neutral/Accent/Success/Warning/Danger) |
| `card(ui, \|ui\| …)` | surface + border + R_MD + SP_4 padding (replace old `card`/`panel_card` internals) |
| `section_header(ui, title, right: impl FnOnce(&mut Ui))` | title left, actions right |
| `page_header(ui, icon, title, subtitle)` | used at the top of scan pages |
| `empty_state(ui, icon, title, body, action: Option<&str>) -> bool` | centred illustration-less empty state |
| `stat(ui, label, value)` | compact metric (replaces `stat_chip` look) |
| `list_row(ui, selected, \|ui\| …) -> Response` | H_ROW row with hover + selected fill |
| `toolbar(ui, \|ui\| …)` | horizontal bar, SP_2 gaps, vertically centred |
| `progress(ui, fraction, label)` | thin 6 px bar + label |
| `kbd(ui, "⌘A")` | key hint used in the shortcuts window |

The old helpers in `style.rs` (`primary_button`, `secondary_button`, `danger_button`, `card`, `panel_card`,
`stat_chip`, `icon_button`, `pill`, …) are kept for now and re-implemented to call the kit, so untouched
panels improve automatically. Remove them only in step 9, once nothing uses them.

### 1.4 Per-screen targets
- **Explorer:** toolbar row = back/forward/up/refresh (icon buttons) · breadcrumb (flex) · search input with
  icon (240 px) · view toggle as `segmented` (list/grid) · filter · new folder. Table: H_ROW rows, zebra-free,
  hover fill, muted Size/Kind/Modified columns, right-aligned size, 16 px folder icons (not 24 px blue blocks).
  Selection bar = floating bottom card with count + actions.
- **Scan pages (Duplicates/Compare/Empty/Archives):** a `page_header`, then two columns: left card
  "Folders" (list rows with path + remove/reorder icon buttons; "Add folder" as a normal primary button,
  **not** full width), right card "Settings" (labelled form rows, aligned labels 110 px, controls H_CONTROL).
  The primary action ("Find duplicates", etc.) goes in the page toolbar (top right) **and** at the bottom of the
  settings card. No huge empty white areas: cards size to their content, empty folder list shows `empty_state`.
- **Progress pages:** centred card 560 px: title, `progress`, phase line, counts as `stat`s, Cancel button
  (existing cancel logic unchanged).
- **Duplicate results:** group list column = compact `list_row`s (Group #n · files badge · reclaimable right
  aligned), filter `text_input` + `segmented` type filter (fixes the "Archives" wrap), sort combo.
  Main column: toolbar (New scan, Refresh, Export ▾, Clean up ▾) · mode banner (Report only + switch) ·
  group header with `stat`s · file cards in a 2-column grid when width > 1100 px · preview limited to
  max 420 px tall · sticky bottom action bar (Trash selected / Keep all).
- **All files:** same toolbar pattern as Explorer; filter chips → `segmented`/`chip`s that never wrap;
  file cards use the kit card.
- **Staging panel / modals / player / confirm / shortcuts / errors / twins:** restyle with kit widgets,
  R_LG modal rounding, consistent paddings. No logic changes.

---

## 2. Steps (do them in order; tick each when committed)

- [ ] **S0 — Setup.**
  `git switch recovery/working-baseline && git switch -c ui-rewrite`. Run §0.2 and record the warning count.
  Capture baseline screenshots of all screens into `/tmp/sortwise-shots/<screen>-before.png`.
  Read `src/ui/style.rs`, `src/ui/mod.rs`, `src/app.rs` lines from `let mode = self.mode;` through `mode_switch`.
  No commit.

- [ ] **S1 — Snapshot screen for All files.** In `src/main.rs` `prepare_screen`, add `"files"`:
  `app.mode = app::AppMode::AllFiles; app.view = app::AppView::Scan;` and set `files_view_dir` to the demo
  library dir (call `demo_result(4)` first so the files exist, as `"staging"` does). Verify the screenshot.
  Commit `ui: S1 add files snapshot screen`.

- [ ] **S2 — Tokens.** Update `src/ui/style.rs` per §1.2: palette values, new fields (add them to both
  `LIGHT` and `DARK`), constants, `configure_app_style`. Do not change any function signature.
  Screenshots of all screens (light + dark). Commit.

- [ ] **S3 — Kit.** Create `src/ui/kit.rs` with every fn in §1.3; `pub mod kit;` in `ui/mod.rs`.
  Re-implement the old `style.rs` helpers on top of the kit (same signatures). Unused-fn warnings for kit
  functions are acceptable in this step only; add `#![allow(dead_code)]` at the top of `kit.rs` and remove it in S9.
  Commit.

- [ ] **S4 — App shell.** In `src/app.rs` `update()` layout section only:
  1. Replace the header contents with: toolbar = page title (from a new `fn page_title(mode, view) -> &'static str`
     in `src/ui/shell.rs`) + Staging button (move its existing code verbatim into `shell::staging_button(app, ui)`).
  2. Add `SidePanel::left("nav")` **before** any other side panel, rendered by new `shell::nav(app, ui)`.
     Move the body of `mode_switch` into `shell::nav` item handlers, keeping each side effect verbatim. Delete
     `mode_switch` afterwards (its `clicked()` calls now live in `shell.rs`; guard counts move with them).
  3. Places: call `crate::ui::explorer_sidebar(app, ui)` inside the nav under a "PLACES" header when
     `mode` is Explorer or AllFiles. Remove the Explorer branch from the old `"sidebar"` panel; keep that panel
     only for `results_dup_list_panel`, renamed id `"groups"`.
  4. Narrow rule: icon rail when `ctx.screen_rect().width() < 900.0`. Keep the existing
     "collapse Places while staging is open" behaviour by rendering Places in rail form (icons only).
  Verify every screen, especially `explorer`, `staging`, `results`, at 1280 and 820 widths. Commit.

- [ ] **S5 — Explorer.** `src/ui/explorer_panel.rs`, functions `toolbar`, `icon_nav`, `view_toggle`,
  `breadcrumb_bar`, `sort_filter_menu`, `file_table`, `header`, `row_ui`, `group_header`, `tile_ui`,
  `selection_bar`, `empty_state`, `banner`, `place_row`. Presentation only. This file is 3,600 lines; do it in
  three commits: S5a toolbar+breadcrumb, S5b table+tiles, S5c selection bar+banners+places. Guard after each.

- [ ] **S6 — Scan & progress pages.** `scan_panel.rs` (`scan_header`, `scan_columns`, `scan_form`,
  `render_settings_panel`, `launch_card`), `progress_panel.rs`, `empty_folders_panel.rs` (scan+progress),
  `archive_files_panel.rs` (scan+progress). One commit per file.

- [ ] **S7 — Results.** `results_panel.rs` (`results_dup_list_panel`, `draw_dup_group_card`,
  `results_main_panel`, `action_row`, `group_action_bar`, `video_controls_bar`, `shortcuts_window`),
  `result_list.rs`, `folder_intelligence.rs` (visual parts of `folder_intel_section`, `folder_twins_modal`),
  results parts of `empty_folders_panel.rs` / `archive_files_panel.rs`. Keyboard navigation functions
  (`apply_dup_list_keyboard_navigation`, `group_keyboard_shortcuts`) must not change. Two to four commits.

- [ ] **S8 — All files, staging, modals.** `recursive_files_panel.rs`, `staging_panel` and its helpers in
  `explorer_panel.rs`, `folder_explorer_panel.rs`, `modal.rs`, `confirm_dialog.rs`, `status_bar.rs`,
  `player_window`/player helpers (style only, never touch mpv/video logic). One commit per file.

- [ ] **S9 — Cleanup.** Remove style helpers that are no longer referenced (grep each name first), remove
  `#![allow(dead_code)]` from `kit.rs` and delete kit fns nothing uses. Full screenshot pass of all 15 screens in
  light, dark and narrow. Warning count must be ≤ baseline. Commit.

- [ ] **S10 — Report.** Reply with: the commits list, before/after screenshot paths for every screen, final
  §0.2 output, the guard table after the rewrite, and anything you could not do. Do not merge.

---

## 3. Known traps
- egui panel order: all `TopBottomPanel`s and `SidePanel`s must be added **before** `CentralPanel`, and the order
  of `SidePanel::left` calls decides left-to-right stacking. The staging `SidePanel::right` is added from
  `crate::ui::staging_panel(self, ctx)`; keep that call before `CentralPanel`.
- Panel ids must be unique and stable (`"nav"`, `"groups"`, `"top"`, `"status"`, `"staging_panel"`), otherwise
  egui forgets widths.
- `egui::Key` variants are upper-case (`Key::A`, not `Key::a`).
- Borrowing: panels take `&mut SortwiseApp`; collect actions into a local enum and apply after rendering,
  exactly like `explorer_panel.rs` does with `Act`. Do not hold `&app.x` across a call that mutates `app`.
- Phosphor icons come from `crate::ui::icons::Icons` constants; do not paste emoji (macOS renders them in
  colour and misaligns rows).
- `src/app.rs` in this branch was reconstructed from session logs; about 3 lines may differ from the lost
  original. If you see code that looks half-finished outside the layout section, report it — do not fix it.
- Do not reintroduce the code that exists only on `main` (`keyboard.rs`, `notifications.rs`,
  `context_menu.rs`, `explorer_interactions.rs`, operation history). It never compiled. Out of scope here.
