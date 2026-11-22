# UI Redesign Plan — Sortwise

Written for: the product owner and the engineer doing the GUI work.
Scope: `src/ui/*` and the chrome in `src/app.rs`. No engine changes.
Status: Phase 0 done (tokens, light default, contrast tests). Phase 1 done (Phosphor icon font, emoji replaced, tab bar restyled). Phase 2 done (modal template applied). Phase 3 done (Explorer). Phase 4 done (staging panel). Phase 5 done (Duplicates scan and results, Compare, Empty folders, Archives; see §13). Phase 6 done (status bar, archive banner, dark toggle; see §14). Phase 7 next (manual, on Windows).

---

## 0. Decisions (locked)

| # | Topic | Decision |
|---|---|---|
| D-UI1 | Default theme | **Light.** Dark kept as an option with the same tokens. |
| D-UI2 | Icons | **Use icons** to save space. Icon-only buttons get a tooltip. Primary actions and modal titles keep a text label. |
| D-UI3 | Staging | **Side panel** (kept), with the detailed layout in §6.8. |
| D-UI4 | Navigation | **Keep six tabs** as they are, restyled and icon-labelled. No Browse/Find regrouping. |
| D-UI5 | Destructive verb | **"Trash"** everywhere. Keep as is. |
| D-UI6 | Modals | All modals follow one template (§5). Every modal in the app is listed and reviewed (§6.9). |
| D-UI7 | Space | Full window width used; no floating cards with empty margins; measured targets in §4. |

---

## 1. What is wrong today (from snapshots and code)

**Theme**
- Dark navy with violet gradients; green Scan button against blue everywhere else.
- Gradient badges and pills carry no meaning.

**Icons**
- Unicode glyphs and emoji in `src/ui/icons.rs` render differently on each OS. Shapes look random (Explorer tab, Duplicates tab). Fixed by D-UI2 and §3.

**Navigation**
- Six tabs with equal weight and no icons. Staging is a separate header button. Fine as a structure (D-UI4), but the tab bar needs clearer selection and spacing.

**Space**
- Scan screen: content fills about half the window.
- Explorer toolbar: 12+ controls in two rows; search is a fixed 180 px box.
- Staging: large modal, Play and Zoom on every row, a queue dropdown per row.

**Windows and modals**
- Nine separate windows in the code (§6.9). Two queue windows ("Queue" and "Movable Queue") show the same items in two places.
- Keyboard-shortcut, error-log and archive-result windows are free-floating with inconsistent sizes.

**Explorer**
- Toolbar mixes navigation, sorting, filtering, view and creation in one crowded row.
- "Group by type" is a checkbox that competes with sort.
- New-folder button is "+ 📁" (emoji glyph).

**Hierarchy**
- Results: the group list, detail, banner and action bar compete. "Dry run" banner and "Accept Suggestion" button add noise.
- Compare A→B looks the same as Duplicates. No label says which folder is A and which is B.

---

## 2. Design principles

1. **One primary action per screen.** Secondary actions visually quieter.
2. **One accent colour** (blue). Red only for Trash and destructive actions. Green and amber only for status.
3. **No decoration without meaning.** No gradients, glows, or pills without a count or state.
4. **Icons for common actions with tooltips; words for primary actions and modal titles.**
5. **Density with rhythm.** 8 px grid. Body text 13–14 px. One row height per list type.
6. **Fill the window.** Panels take the available space. Empty space is intentional (sidebar, a message), never leftover margin.
7. **Same thing looks the same everywhere.** One button style per role, one modal template, one row style per list.

---

## 3. Theme and icons

### 3.1 Tokens (defined once in `style.rs`)

| Token | Light (default) | Dark (option) | Use |
|---|---|---|---|
| `canvas` | `#F6F7F9` | `#121417` | Window background |
| `surface` | `#FFFFFF` | `#1A1D23` | Panels, lists, modals |
| `surface_raised` | `#F0F2F5` | `#23272F` | Hover, selected row |
| `border` | `#D9DDE4` | `#2C313B` | Dividers, outlines |
| `text` | `#1B1F27` | `#E7EAF0` | Primary text |
| `text_muted` | `#5B6472` | `#9AA3B2` | Secondary text, metadata |
| `accent` | `#2563EB` | `#3B82F6` | Primary button, selection, focus |
| `accent_soft` | `#E8F0FE` | `#1E2A44` | Selected tab background |
| `danger` | `#D1343A` | `#E5484D` | Trash and delete only |
| `success` | `#1F8A57` | `#30A46C` | Completed |
| `warning` | `#B7791F` | `#F5A524` | Report-only mode, partial results |

Rules:
- Remove `ACCENT_2`, `gradient_badge`, and the green Scan button.
- Body text on `surface` must reach 4.5:1 contrast in both themes. Check with a script before Phase 1 is signed off.
- Keep the bundled font set.

### 3.2 Icon set

- Replace the glyphs in `src/ui/icons.rs` with one vector icon set (Lucide or Phosphor, ISC/MIT licence). Bundle only the icons in §3.3.
- Render as SVG through the image loader so every OS shows the same shape.
- Icon size: 16 px in toolbars and rows, 20 px in tabs and headers. Stroke colour follows the text token.

### 3.3 Icon inventory

| Use | Icon | Label shown? |
|---|---|---|
| Back, forward, up | arrow-left, arrow-right, arrow-up | No (tooltip) |
| Refresh | rotate-cw | No |
| Search | search | Placeholder text |
| List view / grid view | list, grid | No (tooltip) |
| Sort | arrow-up-down | No (tooltip) |
| Filter | filter | No (tooltip) |
| New folder | folder-plus | No (tooltip) |
| Stage | layers or bookmark-plus | No (tooltip) |
| Open | external-link | No (tooltip) |
| Trash | trash-2 | Yes on primary, no on rows |
| Folder | folder | In lists |
| File types | image, film, music, archive, file | In lists |
| Close | x | No |
| Preview (play) | play | No |
| Status: ok, warning, error | check-circle, alert-triangle, x-circle | Yes |

---

## 4. Space and layout

Measured targets (window 960×720 is the reference; minimum window 640×480):

| Element | Size |
|---|---|
| Top tab bar | 44 px high, full width |
| Status bar | 26 px high, full width |
| Explorer sidebar | 200 px; collapses to icons below 760 px width |
| Explorer toolbar | 44 px high, one row, no wrap at 760 px and up |
| Explorer list row | 30 px |
| Duplicate group row | 52 px |
| Staging side panel | 340 px; resizable 280–480 px |
| Modal (standard) | 560 × 420 px |
| Modal (wide: twins, player) | 880 × 600 px, resizable |
| Confirm dialog | 460 px wide, height by content |
| Gutter | 16 px sides, 8 px between groups |

Rules:
- No control is wider than its label needs. Fixed widths for number and unit fields.
- Lists fill the height. No list ends in unused space unless it is empty (then show an empty state, §4.1).
- Below 760 px width: hide the sidebar, keep the toolbar. Below 640 px: hide the Kind column.

### 4.1 Empty states
- One line of text and the action that fixes it. No illustrations.

---

## 5. Modal template (applies to every modal and dialog)

- **Header:** title (16 px, semibold) left, close icon right. 48 px high.
- **Body:** padded 16 px; scrolls internally, never the window.
- **Footer:** 56 px high. Secondary (Cancel) on the left, primary on the right. Destructive primary is red and labelled "Trash N items".
- **Keys:** Esc closes (and discards a pending action, as today). Enter activates the primary only when it is not destructive.
- **Sizes:** standard 560×420, wide 880×600. Fixed, centred, resizable only where marked.
- **Backdrop:** a single 40 % black scrim. No blur.
- **Focus:** on open, focus goes to the first actionable control.

---

## 6. Per-screen plan

### 6.1 Shell
- **Top bar (44 px):** app name left. Six tabs centre-left in order: Explorer · Duplicates · Compare A→B · Empty folders · Archives. Each tab has an icon and a label. Active tab: `accent_soft` background, `accent` text and a 2 px underline. Staging button right, with a count badge ("Staging 3") only when non-zero.
- **Status bar (26 px):** left: current folder or scan state. Middle: item count and selection ("128 items · 3 selected"). Right: reclaimable size (on Duplicates) and an error count that opens the error log.

### 6.2 Explorer (detailed)

**Toolbar (one row, 44 px):**
- Left group: back, forward, up, refresh (icons, 32 px hit areas).
- Breadcrumb fills the middle. Click each segment to navigate. Overflow collapses leading segments to "…".
- Right group: search (260 px, placeholder "Search this folder"), view toggle (list / grid), sort menu (field and direction in one menu), filter menu (type), new-folder icon.
- "Group by type" moves into the sort menu as a check item.

**Sidebar (200 px):**
- Places: Home, Desktop, Documents, Downloads, Pictures, Movies, Music, then drives. Each item is an icon plus label, 30 px rows.
- "Open folder…" becomes an icon button at the bottom with a label.

**List view:**
- Columns: Name (fills), Size (90 px, right aligned), Kind (110 px), Modified (140 px). Sortable headers with arrow icon.
- Folders first in name sort. Selected row uses `accent_soft`.
- Double-click opens. Single click selects. Ctrl/Cmd and Shift extend selection.
- Checkbox-free rows; selection is by highlight, not checkbox.

**Grid view:**
- Tiles 128 px wide, thumbnail 96 px, name below (one line, elided). Tiles fill the row width.

**Selection bar:**
- Appears at the bottom of the list only when items are selected. 40 px. "3 selected · Stage · Trash · Clear". Hidden otherwise to save space.

**Right-click menu (proposed, needs your yes/no):** Open, Reveal in Finder, Stage, Trash, Copy path. Same items as the selection bar, so no new behaviour.

**Empty folder:** "This folder is empty." One line, no illustration.

**Archive result:** currently a separate window. Becomes an inline banner above the list: "Extracted 12 files to Downloads/backup · Open folder · Dismiss". Auto-dismiss only on the next navigation.

### 6.3 Duplicates — scan
- Two columns that fill the window height.
- Left column (flex): "Folders to scan" heading, list of folders (path muted, remove icon per row). Add folder button full width, primary. Up/down order icons only when more than one folder.
- Right column (340 px): "Scan settings" heading. Checkboxes for skip system and hidden. Min size: number field 72 px and unit dropdown 64 px on one line. Action mode: dropdown ("Report only" default, "Trash duplicates"). Advanced as a disclosure.
- Primary button "Find duplicates" pinned to the bottom of the right column, `accent`. One line under it: "Scans photos, videos, audio and archives."
- The green Scan button is removed.

### 6.4 Duplicates — results
- Top summary strip (36 px): "6 groups · 85.05 KB reclaimable". Right: "Report only" as a warning-coloured label when the mode is report-only, with a "Switch to Trash mode" text button. The "Dry run" banner is removed.
- Left column (300 px): group list, 52 px rows. Group number, copies count, reclaimable size. No per-row progress bar.
- Right column (flex): the selected group. Header: group number, copies, size. Each copy is a row: checkbox (for Trash), path (muted), size, a "Keep" label on the keep file, and an icon button to open the file. Preview image below if the file is an image.
- Bottom bar (fixed, 56 px): primary "Trash 2 selected" (red), secondary "Keep all", text button "Export".
- "Accept suggestion" is removed. The suggested keep file is the default and is labelled "Keep". One line explains why ("kept: in your first-ranked folder").

### 6.5 Compare A→B
- Top: two labelled cards side by side: **Source (A)** and **Destination (B)**. Each lists its folders with an add icon. A one-line explanation under the title: "Finds copies in B that also exist in A. Trash applies to B only."
- Results use the Duplicates layout, with a column "In A" showing the matching path.
- Trash button says "Trash copies in B".

### 6.6 Empty folders
- Same layout as Duplicates results: list left, selection bar bottom.
- Primary: "Trash N empty folders" (red). Rows: folder icon, path, select checkbox. "Select all" in the header.

### 6.7 Archives
- List left with type icon, name, size, folder. Select all in header.
- Target folder row above the list: "Move to: [path] Choose…". Primary "Move N archives" disabled until a target is chosen, with a tooltip explaining why.

### 6.8 Staging side panel (340 px)
- Opens from the right edge, pushes the content (does not cover it). Closes with the close icon or Esc.
- **Header (44 px):** title "Staged" with count and total size ("3 items · 42 KB"). Close icon.
- **Tabs:** Items · Folder queues (text tabs, same style as §6.1).
- **Items tab:** rows 48 px. Thumbnail 40 px, name (one line, elided), path (muted, one line), size right, one overflow menu icon with: Preview, Move to queue, Remove. No Play or Zoom buttons on each row. Preview opens the shared preview window (§6.9).
- **Folder queues tab:** queue list with name, count, target path. Queue selector is a dropdown on each item, only inside the Items tab's overflow menu.
- **Footer (56 px):** "Move all to queue" primary when a queue is chosen, else disabled with a tooltip. Clear text button.
- This replaces the two windows "Queue" and "Movable Queue" (see §6.9).

### 6.9 Modal and window inventory

| Current window | Source | New form | Size |
|---|---|---|---|
| Confirm (bulk trash, delete) | `confirm_dialog.rs` | Dialog template §5. Lists up to 10 paths, "and N more" | 460 px wide |
| Delete zip prompt | `explorer_panel.rs` | Same confirm template | 460 px wide |
| Error log | `results_panel.rs` | Standard modal. Scrolling list, Copy button in footer | 560×420 |
| Keyboard shortcuts | `results_panel.rs` | Standard modal, two-column table | 560×420 |
| Folder twins overlap | `folder_intelligence.rs` | Wide modal. Pair list left, detail right. Overlap as number plus thin accent bar. Remove the heat row | 880×600 |
| Folder modal and folder contents | `folder_explorer_panel.rs`, `folder_contents_modal` | One modal, "Folder contents". **Check first:** confirm both are needed; merge if they show the same data | 880×600 |
| Media player | `explorer_panel.rs` `player_window` | Wide modal, 16:9 video area, transport row, close. Fixed min size 640×480 | 880×600 |
| Queue and Movable Queue | `explorer_panel.rs` | **Removed**; replaced by the staging side panel (§6.8) | — |
| Archive extracted | `explorer_panel.rs` | **Removed**; replaced by an inline banner (§6.2) | — |

Rules for this table:
- Every modal uses §5.
- Opening one modal closes another (no stacked windows).
- Preview (images and video) is the only modal that can be opened from the staging panel.

---

## 7. Phases

| Phase | Scope | Exit check |
|---|---|---|
| 0 | Tokens in `style.rs` (light and dark); remove gradients, `ACCENT_2`, green button. Contrast check | Contrast script passes; snapshot shows one accent |
| 1 | Icon set (§3.2–3.3) and tab bar restyle (§6.1) | Snapshot review; no emoji glyphs left (`grep` for non-ASCII in `icons.rs`) |
| 2 | Modal template (§5) applied to every modal in §6.9 table | Each modal snapshot at standard size; Esc test |
| 3 | Explorer toolbar, sidebar, list and grid (§6.2) | Snapshot at 960×720, 760×600, 640×480; toolbar does not wrap at 760+ |
| 4 | Staging side panel and removal of the two queue windows (§6.8) | Staging actions still reach the same paths (test); no stacked windows |
| 5 | Duplicates scan, results, Compare, Empty, Archives (§6.3–6.7) | Snapshot review of each; one primary per screen |
| 6 | Archive banner, folder modal merge, status bar (§6.1, §6.9) | Banner dismisses on navigation; status counts match list |
| 7 | Windows check of icons and fonts | Snapshot on Windows (manual) |

Each phase is one commit. Before and after snapshots go in the session scratchpad, not the repo.

---

## 8. Verification

- **Headless:** existing `gui_tests` for keyboard and Esc. Add a test for every moved control and every removed window.
- **Set-of-paths test:** staging, trash, and archive moves still touch exactly the selected paths (required by the working agreements).
- **Visual:** `--snapshot` per screen before and after each phase, at 960×720, 760×600, and 640×480.
- **Contrast:** script over every text/surface pair in both themes.
- **Manual on macOS:** click through every button and every modal; check Esc, Cancel and Trash.
- **Windows:** icon and font snapshot after Phase 7.

---

## 9. Open (needs your yes/no)

1. **Right-click menu** in Explorer (§6.2). Proposed, not required.
2. **Folder modal merge** (§6.9): confirm the two folder windows show the same data before merging.
3. **Media player** stays a modal (recommended) or moves into the staging panel.

---

## 10. Phase 2 record (modal template)

Done: `src/ui/modal.rs` implements §5. Applied to: confirm (Trash and apply flows), Error log, Keyboard shortcuts, Archive extracted prompt, Folder twins, Folder contents (one window, not two), Media player.

Deviations from §5 and §6.9:
- **Wide modals are fixed 880 × 600** (clamped to 92% of the screen), not resizable. Resizable windows grew with their content and pushed the frame off-screen.
- **Folder contents**: the header actions moved to the footer (Trash folder, Open in grid, Reveal). Body keeps the toolbar, grid and breadcrumb.
- **Media player**: keeps its own control row inside the body and has no footer bar (`show_no_footer`). Video stage unchanged.
- **Esc in folder contents** is ignored while the file preview is open, so the preview handles it.

Still open for later phases:
- Queue and Movable Queue windows: removed in Phase 4 (staging panel).
- Archive extracted: becomes an inline banner in Phase 6.
- Results toolbar: the "Errors" button runs under the DRY RUN pill when a scan has errors (Phase 5).

---

## 11. Phase 3 record (Explorer)

Done in `src/ui/explorer_panel.rs`, `src/ui/icons.rs`, `src/app.rs`, `src/action_plan.rs`:
- **Toolbar** is one 40 px row: navigation, breadcrumb (fills the middle), search (shrinks with the window), view toggle, one **Sort & filter** menu (sort field, direction, type filter, group by type), new folder. Recursive-search status moved to its own line, shown only while searching.
- **Sidebar** is a fixed 200 px panel (still resizable). Under 760 px window width it becomes an icon-only strip with names as tooltips.
- **List** rows are 30 px. Columns: Name (fills), Size 90, Kind 110, Modified 140. Header sort arrows unchanged.
- **Grid** tiles are 128 px wide.
- **Selection**: Ctrl/Cmd-click toggles, Shift-click selects a range. The selection bar (N selected · Stage · Move to Trash · Clear) appears only while something is selected. The counts line stays at the bottom until the status bar (Phase 6).
- **Context menu** adds Stage / Unstage (files) and Copy path.

Loopholes fixed:
1. **Single-item "Move to Trash" ran immediately** with no confirmation. It now goes through the shared confirm dialog, like every other trash action.
2. **Folder modal "Trash folder" ran immediately.** Same fix.
3. **Multi-selection trash** goes through the confirm dialog and only touches items that are currently listed. A test pins the set.
4. **Toolbar icon buttons** were about 60 px wide (app-wide 16 px padding), which squeezed the breadcrumb. Padding is tightened inside the toolbar.
5. **Row height** was about 46 px, not the planned 30. The column helper hard-coded a 30 px height.
6. **Table overflow**: the bottom strips were clipped. Space is now reserved for the header and strips.
7. **Top tabs** overlapped the app title below 900 px. They are icon-only there, with tooltips.
8. **Dead branch** in the row background (both branches were the same colour) removed.

Kept on purpose, not as the plan literally says:
- **Single click still previews files** (and opens folders). The plan said "single click selects". Changing it would break existing behaviour, so multi-select uses Ctrl/Cmd and Shift instead.
- **Checkboxes remain** in recursive-search results, where they drive Add to Queue. Elsewhere rows have no checkboxes.
- **Header sort arrows** stay in the list view, alongside the menu.

Tests: 48 passing. New this phase: 2 plan-path tests and 4 selection-to-trash tests. Snapshots reviewed at 960×720 and 640×480: list, grid, selection, narrow.

Not verified: right-click menus and drag/keyboard behaviour with a real mouse; the new Trash confirm on real files (the trash itself is covered by plan tests, not by moving files).

---

## 12. Phase 4 record (staging side panel)

Done: the Queue and Movable Queue windows are removed. One right-hand panel (`staging_panel` in `src/ui/explorer_panel.rs`) has two tabs. It opens from the top-bar Staging button and pushes the content aside.

Every existing action is kept:
- **Items:** per-item queue choice, Move now, Remove, Preview (images and video), Play and Stop for video in the row, Assign all, Move ready items, Clear.
- **Folder queues:** queue picker, New queue, queue chips, target folder with Change, remove per file, Move all with progress, Delete queue.

Deviations from §6.8:
- **Fixed 340 px, not resizable.** A resizable panel grew to its maximum (480) on the first frame and stayed there. Rows take their width from the available space, so any slack feeds back into the panel width.
- **Sidebar collapses to icons while the staging panel is open**, so the toolbar keeps room for its breadcrumb and search.
- **Row actions sit in one menu (⋯)**: Preview, Play here, Move to queue, Move now, Remove. Inline video playback is kept there rather than dropped.
- **Fullscreen toggle removed**: the panel is a fixed width now.
- **Header size** shows the item count only. The byte total would need a file-size lookup for queue files on every frame.

Loopholes fixed:
- **Opening a folder queue left the Items tab flagged**, so the panel could show the wrong tab. Opening a queue now switches the tab.

Tests: 49 passing (new: a staging dedupe test). Windows type-check passes. Snapshots reviewed at 960×720 for Items and Folder queues.

Not verified: the popup row menu and the Move-to-queue submenu with a real mouse; inline playback with the mpv feature (the build passes, the playback path was not run).

---

## 13. Phase 5 record (Duplicates, Compare, Empty folders, Archives)

Done:
- **Duplicates scan** (`scan_panel.rs`): both columns fill the window height. Action mode reads "Report only" / "Trash duplicates". Min size is a 72 px number field next to the unit. The primary button reads "Find duplicates" and is pinned to the bottom of the settings card, with its one-line hint under it.
- **Duplicates results** (`results_panel.rs`): "Dry run" banner and DRY RUN pill replaced by a status strip with a warning "Report only" label and "Switch to Trash mode". "Errors (n)" moved into the strip. The mode label reads "TRASH MODE". "Accept suggestion" removed. A fixed bottom bar (56 px) holds "Trash N selected" (red, disabled at 0) and "Keep all". The group-level "Delete Duplicates" is now a secondary "Trash all copies except keep".
- **Compare A→B**: cards titled "Source (A)" and "Destination (B)" with add buttons in blue (red was used for Destination; red is for Trash only). Explanation line "Finds copies in B that also exist in A. Trash applies to B only." Results show "In A: <reference file>" on each B copy, and the bottom bar reads "Trash N copies in B".
- **Empty folders**: the "Delete Selected" button is now "Trash N empty folders" (red) and goes through the shared Trash confirm.
- **Archives**: target row reads "Move to:". Primary reads "Move N archives" and is disabled with a tooltip until a target and a selection exist.

Loopholes fixed:
1. **Empty folders deleted with no confirm and permanently** (`delete_selected_empty_folders` called `remove_file` on dotfiles and `remove_dir`). Removed. Trash now goes through `PendingAction::Trash` with action `empty_folders`, and the list drops trashed folders. Failures stay listed and show in a message card. Plan builder `action_plan::for_empty_folders` is pinned by `empty_folder_plan_is_exactly_the_checked_listed_folders`.
2. **Errors button ran under the DRY RUN pill** (Phase 5 item). Fixed by moving it into the strip.
3. **Results toolbar clipped at 640 px.** Toolbar now wraps (`horizontal_wrapped`).
4. **Duplicates settings card clipped at 640 px** (Min size row wider than the card). Settings column has a 240 px floor.
5. **Folder path text ran under the remove button** in Duplicates and Compare folder cards. Now truncates.
6. **Compare "+ Add Source" was unreadable** (light text on a pale tint). Fixed with the accent fill.
7. **Green Start Scan** in Empty folders and Archives replaced with accent (plan §0 rule).

Deviations from §6:
- §6.4 summary strip (groups and reclaimable) not added at top of results. The left column already shows those figures.
- §6.4 bottom bar has no "Export"; Export stays in the toolbar.
- §6.6 Empty folders keeps its primary in the header (not the bottom bar).
- §6.3 "remove the green Scan button" was already done in Phase 0.

Tests: 50 passing (new: 1 empty-folder plan test). `cargo clippy --lib` 0 warnings. Windows type-check passes.

Snapshots reviewed (960×720): results, errors, scan, compare-scan, compare (top of list), empty, archives. Narrow 640×480: results, scan.

Not verified:
- "In A" rows on Compare results: below the fold in every snapshot (preview takes the top). Code path only.
- Results at 640 px: the right column still runs past the window edge (the list column takes about 360 px). Not fixed in Phase 5. Likely needs the list width capped at 300 (§6.4).
- Clicks, popups, and the Trash confirm on real empty folders (the Trash move itself is not run in tests).

---

## 14. Phase 6 record (shell, status bar, archive banner)

Done:
- **Status bar** (`src/ui/status_bar.rs`, 26 px, bottom). Left: current folder, or scan and screen state. Middle: Explorer item and folder counts with selection; selection counts on the other screens. Right: dark theme toggle, "Errors (n)" (opens the error log), and reclaimable size on Duplicates and Compare. The Explorer counts line moved here.
- **Archive extracted** is an inline banner above the Explorer list (`extract_banner_ui` in `explorer_panel.rs`), not a modal. Buttons: Open folder, Move archive to Trash…, Dismiss. It clears when you navigate away from the folder it appeared in (test `extract_banner_stays_in_its_folder_and_dismisses_on_navigation`).
- **Dark theme toggle** in the status bar (`style::toggle_dark`). Light stays the default. Snapshot checked with `--dark` and with the toggle icon.
- **Errors button** moved from the results strip into the status bar (plan §6.1).
- **Folder modal merge**: already done in Phase 2 (one "Folder contents" window). The Folder twins modal stays separate, as the §6.9 table lists it.

Loopholes fixed:
1. **Archive "Move to Trash" ran with no confirm** (old modal called `trash_single_path` directly). It now goes through the shared Trash confirm (action `extracted_archive`).

640 px results layout (fixed after the first pass):
- Cause: the list sidebar's content set its own minimum width (about 363 px), so the centre column got the rest. Measured with a temporary panel-rect print; the print is removed.
- Culprits: the Groups filter chip row was a non-wrapping horizontal, and the sort combo asked for `min(260, available)`, which on the first frame is the panel max. Chips now wrap; the combo is 200 px. The sidebar is 300 px at 640 and 960.
- Also wrapped: the report-only strip, the Group header and "Copies in Group" rows, and the preview stat chips.
- Image frame now subtracts its 12 px margin (`avail_w` in `preview_body_ui`).
- Residual: at 640 the preview card's right border still sits at or just past the window edge. The card width comes from elsewhere in the card; not found. Cosmetic.

Tests: 51 passing (new: archive banner dismissal). `cargo clippy --lib` 0 warnings. Windows type-check passes.

Snapshots reviewed (960×720): explorer, results (light and dark), archive banner (`explorer-extracted` scene, new), errors (status bar shows Errors (2)), empty, archives, scan.

Not verified:
- Clicks: the banner buttons, the Trash confirm from the banner, the toggle, and Errors (2) from the status bar.
- The status counts matching the list are by construction (same data), not tested.
- The 640 px results layout.

---

## 15. Consistency pass (scan and list screens)

Problem: Empty folders and Archives were laid out differently from Duplicates and Compare (different title size, no icon, a narrower column, a side action panel, and different results headers).

Shared pieces now used by every screen:
- `scan_panel.rs`: `scan_scroll` (scroll wrapper and column height), `scan_header` (icon, 22 px title, one muted line), `scan_columns` (left folders, right launch card), `launch_card` (title, body, primary button and hint pinned to the card bottom), `render_folder_box` (folder list).
- `result_list.rs` (new): `list_header` (title, summary, Back), `selection_toolbar` (select all, count, one primary action), `list_card` (scrolling list), `list_row` (checkbox, number, path, trailing action).

Screens:
- Duplicates and Compare setup: the same header and launch card as before, now built from the shared pieces.
- Empty folders and Archives setup: same header, folder card, and launch card. Empty folders' Start button is now the same blue primary as the others.
- Empty folders and Archives results: header with Back, toolbar card (select all, count, primary action), list card. Archives keeps its "Move to:" card above the toolbar.
- Duplicates results keep their group layout (plan §6.4). This is intentional: the group view is a different task.

Not changed: the right card on Empty folders and Archives is about 10 px narrower than on Duplicates. The Duplicates settings row needs the extra width.

Tests: 51 passing. `cargo clippy --lib` 0 warnings.
Snapshots reviewed: scan, compare-scan, empty-scan, archives-scan, empty-results, archives-results.
Not verified: clicks on the new toolbar checkboxes and primary buttons.

---

## 16. Staging footer fix and send-then-move flow

- **Crop fixed.** The staging footer was reserved at a fixed 46 px, but the Items footer has two rows, so its buttons were cut at the bottom. The footer is now a bottom panel inside the staging panel, and the body takes the rest.
- **Flow.** Staged items (Items tab) go in three steps: send to a queue (per item from the ⋯ menu, or "Send all to ▾" for all), then one click on "Move N ready" moves every sent item into its queue's folder. The footer shows "N of M sent" and says "Create a queue first" when no queue exists.
- **Not changed:** "Move N ready" runs without a confirm. It moves files, not Trash, so it follows the existing behaviour. Say if you want a confirm there too.

Tests: 51 passing. Clippy 0 warnings. Snapshots reviewed: `staging`, `staging-sent` (new scene), `queues`.
Not verified: clicks on "Send all to", the ⋯ menu and "Move N ready".

---

## 17. Rebrand: Sortwise

- **Name:** Sortwise. The crate and binary keep the name `sortwise` (internal; no user-visible change).
- **Mark:** `packaging/brand/sortwise-icon.svg` (source), `packaging/brand/sortwise-1024.png`, `packaging/macos/Sortwise.icns` (bundle icon), `assets/sortwise-icon.png` (in-app logo and window icon, bundled with `include_bytes!`).
- **Where the name shows:** window title, top bar text, macOS bundle (`Sortwise.app`, `com.sortwise.desktop`, icon file set in Info.plist), privacy usage strings, README, Run.md, Windows installer name and build script text.
- **Top bar:** the mark replaces the folder glyph. Falls back to the glyph if the PNG cannot be decoded.

Notes:
- The bundle ID changed, so macOS treats this as a new app. Folder permissions (Documents, Desktop, Downloads) need to be granted once more.
- The older `Sortwise.app` in /Applications is not removed by the script. Delete it by hand if you don't want both.
- Not yet reinstalled: the installed app is still the previous build until the install script runs again.

Tests: 51 passing. Clippy and build 0 warnings. Snapshot reviewed: top bar with the mark.

---

## 18. Media player: no second header, bigger picture

- The player had two headers: the modal title bar ("Media player", 48 px) and its own toolbar row. The title bar is gone; the toolbar row (name, queue, stage, trash, counter, close) is the only header.
- The window is `ModalSize::Screen` (96% of the window), up from 880 × 600. Shown with `modal::show_bare`: no header, no footer, no body padding.
- `show_no_footer` is replaced by `show_bare`. Its only caller was the player.
- Esc still closes the player (handled in `player_window`).

Tests: 51 passing. Clippy and build 0 warnings. Snapshot reviewed: `player` (new scene).
Not verified: clicks on the toolbar, and video playback with the mpv feature.

---

## 19. Empty and Archive results: layout and buttons

- **Rows** (`list_row`): name in bold, parent path under it, same as the Duplicates rows. The raw full path in monospace is gone. Index numbers are gone.
- **Archives:** destination and selection are one toolbar card (was two stacked cards). "Choose folder…" is secondary, so the card has one primary action: "Move N archives".
- **Labels:** singular and plural handled with `count_label` ("1 empty folder" / "3 empty folders").

Tests: 51 passing. Clippy and build 0 warnings. Snapshots reviewed: `empty`, `archives`.
Not verified: clicks on the toolbar checkboxes, "Choose folder…", "Move" and the row Open buttons.

---

## 20. Explorer responsive pass (sizes from the window, not fixed pixels)

- **List columns** (`table_widths`): Kind drops out first, then Modified, based on the table's real width. Name takes the rest. Hidden columns are skipped in both header and rows. At 640 px Modified is hidden; at 1280 all four show.
- **Grid** (`explorer-grid`): columns come from the available width (up to 8). Tiles stretch to fill each row, so the right edge lines up at every width. Rows are top-aligned, so tiles no longer sit at different heights. Label frame was 16 px too wide and pushed the last tile past the window edge; fixed.
- **Staging panel**: 340 px beside the content at 900 px and wider. Below 900 px it takes the whole window (minus the 56 px icon sidebar) so the toolbar and list are never squeezed. Close it to get the content back.
- **Staging rows and footer**: the row menu no longer runs into the panel's right edge; footer and body keep a 12 px right inset.

Tests: 51 passing. Clippy and build 0 warnings.
Snapshots reviewed: explorer at 1280, 960, 640; explorer grid at 640 and 1280; staging at 960 and 640.
Not verified: clicks, drag and selection in the grid; the staging panel's popup menu.

---

## 21. Codebase rename: Smart File Manager → Sortwise

Renamed everywhere in the repo (source, tests, examples, benches, docs, README, CI, packaging, scripts):
- **Crate, library and binaries:** `sortwise` (library, `sortwise::…` imports), `sortwise` (app binary), `sortwise-cli` (CLI, was `sfm-cli`).
- **Types and display text:** `SortwiseApp`; "Sortwise" in every title, label and installer name.
- **Packaging:** `packaging/windows/Sortwise.iss`; macOS bundle executable `sortwise`; build scripts build and copy `sortwise`.
- **Storage folders:** cache, action log and settings now live under `…/sortwise/` (was `…/smart-file-manager/`). Existing settings and the action log under the old folder are not read by the new name. The old files are still on disk; copy them over if you need the history.

Known leftover (not changed here): `src/ui/mpv_player.rs` sets an mpv log file to a hard-coded developer path. It should be a per-user path. Flagged for a separate fix.

Checks: no old name remains in the repo (text search). Build, `sortwise-cli` build, 51 tests, and clippy all pass.
The installed app in /Applications is still the previous build under the old binary name.

---

## 22. Staging tab order and default

- Staging tabs: **Folder queues** first, then **Items**.
- The top-bar Staging button opens on **Folder queues**. Items opens only from its own tab (or from the item-level actions that open it).

Tests: 51 passing. Clippy 0 warnings. Snapshot reviewed: `staging` (Items shown, Folder queues first in the tab row).
