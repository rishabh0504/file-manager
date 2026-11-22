mod app {
    pub mod state;
}
mod ui;
mod operations;
mod helpers;
mod types;
mod constants;

use app::SortwiseApp;
use eframe::egui;
use sortwise::types::{DuplicateGroup, FileEntrySnapshot, ScanResult, ScanStats};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> eframe::Result {
    // Headless frame-time probe for the results view (Implementation-Plan Phase 0).
    // Usage: sortwise --bench-ui [groups] [frames]
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--bench-ui") {
        let groups = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
        let frames = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(300);
        bench_ui(groups, frames);
        return Ok(());
    }

    // Visual review aid: `--snapshot <out.png> [results|scan|explorer|staging|queues|empty|archives|compare|twins]`.
    let snapshot = args.iter().position(|a| a == "--snapshot").and_then(|i| {
        let path = PathBuf::from(args.get(i + 1)?);
        let screen = args.get(i + 2).cloned().unwrap_or_else(|| "results".into());
        Some((path, screen))
    });

    // Dark theme is opt-in for now (light is the default).
    ui::style::preset_dark(args.iter().any(|a| a == "--dark"));

    // Dev aid for snapshots: `--size WxH` overrides the default 960×720 window.
    let size = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        })
        .unwrap_or([960.0, 720.0]);

    // Window and Dock icon: the Sortwise mark, bundled at build time. A decode failure falls back
    // to the default icon instead of stopping the app.
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size([640.0, 480.0]);
    match eframe::icon_data::from_png_bytes(include_bytes!("../assets/sortwise-icon.png")) {
        Ok(icon) => viewport = viewport.with_icon(std::sync::Arc::new(icon)),
        Err(e) => eprintln!("Sortwise: window icon not loaded: {e}"),
    }

    let native_options = eframe::NativeOptions {
        viewport,
        // The embedded MPV player renders via egui_glow::CallbackFn which
        // requires an active OpenGL context — so we must use the Glow backend.
        #[cfg(feature = "mpv")]
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Sortwise",
        native_options,
        Box::new(|cc| {
            crate::ui::style::configure_app_style(&cc.egui_ctx);
            let mut app = SortwiseApp::default();
            let loaded = match sortwise::settings::settings_path() {
                Some(path) => sortwise::settings::load_from(&path),
                None => Ok(sortwise::settings::Settings::default()),
            };
            match loaded {
                Ok(s) => app.apply_settings(&s),
                Err(e) => app.apply_errors = Some(format!("Settings file could not be read; using defaults. {e}")),
            }
            app.saved_settings = app.current_settings();
            if let Some((path, screen)) = snapshot {
                prepare_screen(&mut app, &screen);
                app.snapshot = Some(app::SnapshotPlan { path, frames: 0, requested: false });
            }
            Ok(Box::new(app))
        }),
    )
}

/// Put the app on one named screen with sample data (used by `--snapshot`).
fn prepare_screen(app: &mut SortwiseApp, screen: &str) {
    match screen {
        "scan" => {
            app.mode = app::AppMode::Dedupe;
            app.view = app::AppView::Scan;
            app.roots = vec![PathBuf::from("/Users/Shared"), PathBuf::from("/Volumes/Backup")];
        }
        "explorer" => {
            app.mode = app::AppMode::Explorer;
        }
        "staging" => {
            app.mode = app::AppMode::Explorer;
            let _ = demo_result(4); // makes the demo files exist
            let dir = std::env::temp_dir().join("sfm_demo_library").join("Photos");
            for i in 0..3 {
                app.toggle_movable(dir.join(format!("IMG_{:04}.jpg", 1000 + i)));
            }
            app.movable_queue_open = true;
        }
        "staging-sent" => {
            // Items tab with one queue made and one item already sent to it.
            app.mode = app::AppMode::Explorer;
            let _ = demo_result(4);
            let dir = std::env::temp_dir().join("sfm_demo_library").join("Photos");
            for i in 0..3 {
                app.toggle_movable(dir.join(format!("IMG_{:04}.jpg", 1000 + i)));
            }
            app.explorer_queues.push(app::FileQueue {
                name: "Sorted photos".into(),
                target: dir.clone(),
                items: vec![],
            });
            app.movable_items[0].target_queue = Some("Sorted photos".into());
            app.movable_queue_open = true;
        }
        "queues" => {
            app.mode = app::AppMode::Explorer;
            let _ = demo_result(4);
            let dir = std::env::temp_dir().join("sfm_demo_library").join("Photos");
            app.explorer_queues.push(app::FileQueue {
                name: "Sorted photos".into(),
                target: dir.clone(),
                items: vec![dir.join("IMG_1000.jpg"), dir.join("IMG_1001.jpg")],
            });
            app.explorer_active_queue = Some(0);
            app.explorer_queue_view_open = true;
        }
        "empty" => {
            app.mode = app::AppMode::EmptyFolders;
            app.view = app::AppView::Results;
            let root = demo_library_root();
            app.empty_folders_results = ["Photos/2019/Trip", "Downloads/old/unused", "Documents/Drafts"]
                .iter()
                .map(|rel| {
                    let dir = root.join(rel);
                    let _ = std::fs::create_dir_all(&dir);
                    dir
                })
                .collect();
            app.empty_folders_selected = app.empty_folders_results.iter().take(1).cloned().collect();
        }
        "archives" => {
            app.mode = app::AppMode::ArchiveFiles;
            app.view = app::AppView::Results;
            let root = demo_library_root();
            let dir = root.join("Downloads");
            let _ = std::fs::create_dir_all(&dir);
            app.archive_files_results = ["backup-2021.zip", "photos.tar.gz", "project.7z"]
                .iter()
                .map(|name| {
                    let path = dir.join(name);
                    let _ = std::fs::write(&path, b"demo archive");
                    path
                })
                .collect();
            app.archive_files_selected = app.archive_files_results.iter().take(2).cloned().collect();
        }
        "empty-scan" => {
            app.mode = app::AppMode::EmptyFolders;
            app.view = app::AppView::Scan;
            app.roots = vec![demo_library_root().join("Photos"), demo_library_root().join("Downloads")];
        }
        "archives-scan" => {
            app.mode = app::AppMode::ArchiveFiles;
            app.view = app::AppView::Scan;
            app.roots = vec![demo_library_root().join("Downloads")];
        }
        "compare" => {
            app.mode = app::AppMode::CompareSourceDest;
            app.view = app::AppView::Results;
            app.source_roots = vec![demo_library_root().join("Photos")];
            app.dest_roots = vec![demo_library_root().join("Downloads")];
            app.set_last_result(Some(demo_result(6)));
            app.results_selected_group = Some(0);
        }
        "compare-scan" => {
            app.mode = app::AppMode::CompareSourceDest;
            app.view = app::AppView::Scan;
            app.source_roots = vec![demo_library_root().join("Photos")];
            app.dest_roots = vec![demo_library_root().join("Downloads")];
        }
        "player" => {
            app.mode = app::AppMode::Explorer;
            let dir = demo_library_root().join("Photos");
            app.explorer_dir = dir.clone();
            app.explorer_ensure_loaded();
            app.explorer_open_preview(dir.join("IMG_1000.jpg"));
        }
        "explorer-extracted" => {
            app.mode = app::AppMode::Explorer;
            let dir = demo_library_root().join("Downloads");
            app.explorer_dir = dir.clone();
            app.extract_banner = Some(app::ExtractBanner {
                zip: dir.join("backup-2021.zip"),
                dest: dir.join("backup-2021"),
                dir,
            });
        }
        "explorer-grid" => {
            app.mode = app::AppMode::Explorer;
            let _ = demo_result(4);
            app.explorer_dir = demo_library_root().join("Photos");
            app.explorer_view_mode = app::ExplorerViewMode::Grid;
        }
        "explorer-select" => {
            app.mode = app::AppMode::Explorer;
            let _ = demo_result(4);
            let dir = demo_library_root().join("Photos");
            app.explorer_dir = dir.clone();
            app.explorer_multi.insert(dir.join("IMG_1000.jpg"));
            app.explorer_multi.insert(dir.join("IMG_1001.jpg"));
        }
        "twins" => {
            app.mode = app::AppMode::Dedupe;
            app.view = app::AppView::Results;
            let result = demo_result(6);
            app.folder_overlap = ui::folder_intelligence::compute_folder_overlap(&result);
            app.set_last_result(Some(result));
            app.dedupe_twins_modal_open = true;
        }
        "confirm" => {
            app.mode = app::AppMode::Dedupe;
            app.view = app::AppView::Results;
            let result = demo_result(6);
            let plan = sortwise::action_plan::for_apply(&result, sortwise::types::ActionMode::Delete);
            app.set_last_result(Some(result));
            app.pending_action = Some(app::PendingAction::Apply {
                mode: sortwise::types::ActionMode::Delete,
                plan,
            });
        }
        "shortcuts" => {
            app.mode = app::AppMode::Dedupe;
            app.view = app::AppView::Results;
            app.set_last_result(Some(demo_result(6)));
            app.shortcuts_open = true;
        }
        "errors" => {
            app.mode = app::AppMode::Dedupe;
            app.view = app::AppView::Results;
            let mut result = demo_result(6);
            result.error_log = vec![
                (PathBuf::from("/Volumes/Backup/Locked/secret.jpg"), "Permission denied".into()),
                (PathBuf::from("/Volumes/Backup/Old/broken.mov"), "Input/output error".into()),
            ];
            app.set_last_result(Some(result));
            app.show_error_log = true;
        }
        "folder" => {
            app.mode = app::AppMode::Explorer;
            let _ = demo_result(4);
            app.folder_modal_open_dir(demo_library_root().join("Photos"));
        }
        _ => {
            app.mode = app::AppMode::Dedupe;
            app.view = app::AppView::Results;
            app.set_last_result(Some(demo_result(6)));
            app.results_selected_group = Some(0);
            app.results_selected_file_in_group = 0;
        }
    }
}

/// Demo library with real JPEG files, so previews and thumbnails render in snapshots.
/// Two copies per group: one in "Photos" (kept, shorter path) and one in "Downloads".
fn demo_library_root() -> PathBuf {
    std::env::temp_dir().join("sfm_demo_library")
}

fn demo_result(groups: usize) -> ScanResult {
    let root = demo_library_root();
    let photos = root.join("Photos");
    let downloads = root.join("Downloads");
    let _ = std::fs::create_dir_all(&photos);
    let _ = std::fs::create_dir_all(&downloads);
    let mut duplicate_groups = Vec::new();
    for i in 0..groups {
        let img = image::RgbImage::from_fn(640, 480, |x, y| {
            let t = (x as f32 / 640.0 * 255.0) as u8;
            image::Rgb([t, ((y as f32 / 480.0) * 255.0) as u8, (i * 37 % 256) as u8])
        });
        let keep_path = photos.join(format!("IMG_{:04}.jpg", 1000 + i));
        let copy_path = downloads.join(format!("IMG_{:04}.jpg", 1000 + i));
        let _ = img.save(&keep_path);
        let _ = std::fs::copy(&keep_path, &copy_path);
        let size = std::fs::metadata(&keep_path).map(|m| m.len()).unwrap_or(0);
        duplicate_groups.push(DuplicateGroup {
            hash: format!("{i:064x}"),
            files: vec![
                FileEntrySnapshot { path: keep_path, size, score: 40, is_keep: true, root_index: 0 },
                FileEntrySnapshot { path: copy_path, size, score: 10, is_keep: false, root_index: 0 },
            ],
            total_wasted_bytes: size,
            keep_index: 0,
            keep_reason: "in your first-ranked folder".into(),
        });
    }
    let wasted: u64 = duplicate_groups.iter().map(|g| g.total_wasted_bytes).sum();
    ScanResult {
        stats: ScanStats {
            files_scanned: (groups * 2) as u64,
            files_skipped_error: 0,
            duplicate_groups: groups as u64,
            total_wasted_bytes: wasted,
            partial: false,
            hard_link_groups: 0,
        },
        duplicate_groups,
        hard_link_groups: vec![],
        error_log: vec![],
        folder_summaries: vec![],
        cross_folder_conflicts: vec![],
    }
}

/// Synthetic result: `groups` duplicate pairs, one keep file per group.
fn synthetic_result(groups: usize) -> ScanResult {
    let duplicate_groups: Vec<DuplicateGroup> = (0..groups)
        .map(|i| {
            let files = (0..2)
                .map(|j| FileEntrySnapshot {
                    path: PathBuf::from(format!("/data/photos/2019/IMG_{i:08}_{j}.jpg")),
                    size: 4_000_000,
                    score: 10 - j as i64,
                    is_keep: j == 0,
                    root_index: 0,
                })
                .collect();
            DuplicateGroup {
                keep_reason: "shallowest path".into(),
                hash: format!("{i:064x}"),
                files,
                total_wasted_bytes: 4_000_000,
                keep_index: 0,
            }
        })
        .collect();
    ScanResult {
        stats: ScanStats {
            files_scanned: (groups * 2) as u64,
            files_skipped_error: 0,
            duplicate_groups: groups as u64,
            total_wasted_bytes: (groups as u64) * 4_000_000,
            partial: false,
            hard_link_groups: 0,
        },
        duplicate_groups,
        hard_link_groups: vec![],
        error_log: vec![],
        folder_summaries: vec![],
        cross_folder_conflicts: vec![],
    }
}

/// Runs the results view headlessly and prints frame-time percentiles.
/// The first frames are dropped (font and texture warm-up).
fn bench_ui(groups: usize, frames: usize) {
    let mut app = SortwiseApp::default();
    app.mode = app::AppMode::Dedupe;
    app.view = app::AppView::Results;
    app.set_last_result(Some(synthetic_result(groups)));
    app.results_selected_group = Some(0);
    app.results_selected_file_in_group = 0;

    let ctx = egui::Context::default();
    crate::ui::style::configure_app_style(&ctx);

    let warmup = 10.min(frames);
    let mut times: Vec<Duration> = Vec::with_capacity(frames);
    for _ in 0..frames {
        let t = Instant::now();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::SidePanel::left("sidebar")
                .default_width(300.0)
                .show(ctx, |ui| crate::ui::results_dup_list_panel(&mut app, ui, ctx));
            egui::CentralPanel::default().show(ctx, |ui| crate::ui::results_main_panel(&mut app, ui, ctx));
        });
        times.push(t.elapsed());
    }

    let mut measured: Vec<f64> = times[warmup..].iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    measured.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| measured[((measured.len() - 1) as f64 * p).round() as usize];
    println!("groups={groups} frames={} (warmup dropped: {warmup})", measured.len());
    println!("frame_ms p50={:.2} p95={:.2} max={:.2}", pct(0.5), pct(0.95), pct(1.0));
}

#[cfg(test)]
mod gui_tests {
    //! Headless UI tests: real egui frames with injected key events (no window needed).
    use super::*;
    use crate::app::{AppMode, AppView, PendingAction};
    use crate::ui::{confirm_pending_action, results_dup_list_panel, results_main_panel};
    use eframe::egui::{Event, Key, Modifiers, RawInput};

    fn new_app() -> SortwiseApp {
        let mut app = SortwiseApp::default();
        app.mode = AppMode::Dedupe;
        app.view = AppView::Results;
        app.set_last_result(Some(synthetic_result(3)));
        app.results_selected_group = Some(0);
        app.results_selected_file_in_group = 0;
        app
    }

    /// Run one egui frame with the results layout and the confirm dialog.
    fn frame(app: &mut SortwiseApp, ctx: &egui::Context, events: Vec<Event>, modifiers: Modifiers) {
        let input = RawInput { events, modifiers, ..Default::default() };
        let _ = ctx.run(input, |ctx| {
            egui::SidePanel::left("sidebar")
                .default_width(300.0)
                .show(ctx, |ui| results_dup_list_panel(app, ui, ctx));
            egui::CentralPanel::default().show(ctx, |ui| results_main_panel(app, ui, ctx));
            confirm_pending_action(app, ctx);
        });
    }

    #[test]
    fn extract_banner_stays_in_its_folder_and_dismisses_on_navigation() {
        use crate::app::ExtractBanner;
        let base = std::env::temp_dir().join(format!("sfm_banner_{}", std::process::id()));
        let here = base.join("here");
        let there = base.join("there");
        std::fs::create_dir_all(&here).unwrap();
        std::fs::create_dir_all(&there).unwrap();

        let mut app = SortwiseApp::default();
        app.mode = AppMode::Explorer;
        app.explorer_dir = here.clone();
        app.extract_banner = Some(ExtractBanner {
            zip: here.join("a.zip"),
            dest: here.join("a"),
            dir: here.clone(),
        });

        let ctx = egui::Context::default();
        let run = |app: &mut SortwiseApp| {
            let _ = ctx.run(RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| crate::ui::explorer_main_panel(app, ui));
            });
        };
        run(&mut app);
        assert!(app.extract_banner.is_some(), "banner stays while the folder is unchanged");

        app.explorer_navigate(there.clone());
        run(&mut app);
        assert!(app.extract_banner.is_none(), "navigating away dismisses the banner");

        let _ = std::fs::remove_dir_all(&base);
    }

    fn key(k: Key, modifiers: Modifiers) -> Event {
        Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    fn ctrl() -> Modifiers {
        Modifiers { ctrl: true, command: true, ..Default::default() }
    }

    #[test]
    fn delete_key_stages_checked_copies_for_confirmation() {
        let mut app = new_app();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![], Modifiers::default());

        let copy = PathBuf::from("/data/photos/2019/IMG_00000000_1.jpg");
        app.results_selected_files.insert(copy.clone());
        frame(&mut app, &ctx, vec![key(Key::Delete, Modifiers::default())], Modifiers::default());

        match &app.pending_action {
            Some(PendingAction::Trash { plan, .. }) => assert_eq!(plan.paths, vec![copy]),
            other => panic!("expected a staged trash, got {other:?}"),
        }
    }

    #[test]
    fn escape_cancels_staged_action_without_changing_anything() {
        let mut app = new_app();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![], Modifiers::default());
        app.results_selected_files.insert(PathBuf::from("/data/photos/2019/IMG_00000000_1.jpg"));
        frame(&mut app, &ctx, vec![key(Key::Delete, Modifiers::default())], Modifiers::default());
        assert!(app.pending_action.is_some());

        frame(&mut app, &ctx, vec![key(Key::Escape, Modifiers::default())], Modifiers::default());
        assert!(app.pending_action.is_none(), "Esc discards the plan");
        assert_eq!(app.last_result.as_ref().map(|r| r.duplicate_groups.len()), Some(3), "nothing was removed");
    }

    #[test]
    fn ctrl_a_checks_every_non_keep_copy_in_the_group() {
        let mut app = new_app();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![], Modifiers::default());
        frame(&mut app, &ctx, vec![key(Key::A, ctrl())], ctrl());

        let group = &app.last_result.as_ref().unwrap().duplicate_groups[0];
        let expected: std::collections::HashSet<PathBuf> =
            group.files.iter().enumerate().filter(|(i, _)| *i != group.keep_index).map(|(_, f)| f.path.clone()).collect();
        assert_eq!(app.results_selected_files, expected);
        assert!(!app.results_selected_files.contains(&group.files[group.keep_index].path));
    }

    #[test]
    fn question_mark_toggles_shortcut_list() {
        let mut app = new_app();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![], Modifiers::default());
        frame(&mut app, &ctx, vec![Event::Text("?".into())], Modifiers::default());
        assert!(app.shortcuts_open);
        frame(&mut app, &ctx, vec![Event::Text("?".into())], Modifiers::default());
        assert!(!app.shortcuts_open);
    }

    #[test]
    fn delete_with_only_keep_checked_stages_nothing() {
        let mut app = new_app();
        let ctx = egui::Context::default();
        frame(&mut app, &ctx, vec![], Modifiers::default());
        app.results_selected_files.insert(PathBuf::from("/data/photos/2019/IMG_00000000_0.jpg"));
        frame(&mut app, &ctx, vec![key(Key::Delete, Modifiers::default())], Modifiers::default());
        assert!(app.pending_action.is_none(), "the keep file alone is never staged");
        assert!(app.apply_message.is_some());
    }
}
