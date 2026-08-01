# Professional UI & Duplicate-Group Preview — Implementation Plan

This document describes the **product UX**, **visual system**, and **technical implementation** for upgrading Smart File Manager to a polished desktop experience with **duplicate-group browsing** and **file preview**. It complements [Implementation.md](Implementation.md) (engine) and [Features.md](Features.md) (shipping scope).

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
