#![allow(clippy::field_reassign_with_default)] // tests build an app, then set only what they need
use super::*;

#[test]
fn test_system_entry_exclusions() {
    assert!(is_system_entry(".git", true));
    assert!(is_system_entry("node_modules", true));
    assert!(is_system_entry("__pycache__", true));
    assert!(is_system_entry("$recycle.bin", true));
    assert!(is_system_entry(".DS_Store", false));
    assert!(is_system_entry(".ds_store", false));
    assert!(is_system_entry("._photo.jpg", false));
    assert!(is_system_entry("thumbs.db", false));

    // Normal and custom hidden folders should NOT be considered system junk
    assert!(!is_system_entry("Documents", true));
    assert!(!is_system_entry(".config", true));
    assert!(!is_system_entry(".secret_archive", true));
    assert!(!is_system_entry(".hidden_photos", true));
    assert!(!is_system_entry("vacation_2024", true));
    assert!(!is_system_entry("photo.jpg", false));
}

#[test]
fn test_folder_explorer_listing_and_system_filtering() {
    let temp_dir = std::env::temp_dir().join(format!(
        "sfm_folder_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Create normal folders, hidden folders, and system junk folders
    let normal_dir1 = temp_dir.join("Albums");
    let normal_dir2 = temp_dir.join("Invoices");
    let hidden_dir = temp_dir.join(".custom_hidden_folder");
    let sys_git = temp_dir.join(".git");
    let sys_node = temp_dir.join("node_modules");

    std::fs::create_dir_all(&normal_dir1).unwrap();
    std::fs::create_dir_all(&normal_dir2).unwrap();
    std::fs::create_dir_all(&hidden_dir).unwrap();
    std::fs::create_dir_all(&sys_git).unwrap();
    std::fs::create_dir_all(&sys_node).unwrap();

    // Add dummy files
    std::fs::write(temp_dir.join(".DS_Store"), b"junk").unwrap();
    std::fs::write(temp_dir.join("root_note.txt"), b"hello").unwrap();

    let mut app = SortwiseApp::default();
    app.explorer_dir = temp_dir.clone();
    app.explorer_view_mode = ExplorerViewMode::Folders;
    app.explorer_show_hidden = true;
    app.explorer_reload();

    let dir_names: Vec<String> = app
        .explorer_entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| e.name.clone())
        .collect();

    // Must include normal and hidden folders
    assert!(dir_names.contains(&"Albums".to_string()));
    assert!(dir_names.contains(&"Invoices".to_string()));
    assert!(dir_names.contains(&".custom_hidden_folder".to_string()));

    // Must NOT include system junk folders or system files
    assert!(!dir_names.contains(&".git".to_string()));
    assert!(!dir_names.contains(&"node_modules".to_string()));
    assert!(!dir_names.contains(&".DS_Store".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_folder_modal_and_carousel_navigation() {
    let temp_dir = std::env::temp_dir().join(format!(
        "sfm_carousel_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let f1 = temp_dir.join("Folder_A");
    let f2 = temp_dir.join("Folder_B");
    let f3 = temp_dir.join("Folder_C");
    std::fs::create_dir_all(&f1).unwrap();
    std::fs::create_dir_all(&f2).unwrap();
    std::fs::create_dir_all(&f3).unwrap();

    // Put files in Folder_A
    std::fs::write(f1.join("image1.png"), b"img").unwrap();
    std::fs::write(f1.join("doc1.pdf"), b"pdf").unwrap();

    let mut app = SortwiseApp::default();
    app.explorer_dir = temp_dir.clone();
    app.explorer_reload();

    // 1. Open Folder_A in modal
    app.folder_modal_open_dir(f1.clone());
    assert!(app.folder_modal_open);
    assert_eq!(app.folder_modal_path, Some(f1.clone()));
    assert_eq!(app.folder_modal_entries.len(), 2);

    // 2. Navigate Next -> Folder_B
    app.folder_modal_next();
    assert_eq!(app.folder_modal_path, Some(f2.clone()));

    // 3. Navigate Next -> Folder_C
    app.folder_modal_next();
    assert_eq!(app.folder_modal_path, Some(f3.clone()));

    // 4. Navigate Prev -> Folder_B
    app.folder_modal_prev();
    assert_eq!(app.folder_modal_path, Some(f2.clone()));

    // 5. Open in Grid -> updates explorer_dir and closes modal
    app.folder_modal_open_in_grid();
    assert!(!app.folder_modal_open);
    assert_eq!(app.explorer_dir, f2);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_queue_and_dequeue_features() {
    let mut app = SortwiseApp::default();
    let target_dir = std::env::temp_dir().join("sfm_queue_target");
    app.explorer_queues.push(FileQueue {
        name: "Sorted Photos".to_string(),
        target: target_dir,
        items: Vec::new(),
    });

    let test_folder = PathBuf::from("/Users/test/UnmanagedFolder");
    let test_file = PathBuf::from("/Users/test/UnmanagedFolder/photo.jpg");

    // 1. Test staging folder
    assert!(!app.explorer_is_in_queue(0, &test_folder));
    app.explorer_toggle_queue(0, test_folder.clone());
    assert!(app.explorer_is_in_queue(0, &test_folder));
    assert_eq!(app.explorer_queues[0].items.len(), 1);

    // 2. Test dequeuing folder
    app.explorer_toggle_queue(0, test_folder.clone());
    assert!(!app.explorer_is_in_queue(0, &test_folder));
    assert_eq!(app.explorer_queues[0].items.len(), 0);

    // 3. Test staging file
    assert!(!app.explorer_is_in_queue(0, &test_file));
    app.explorer_toggle_queue(0, test_file.clone());
    assert!(app.explorer_is_in_queue(0, &test_file));
    assert_eq!(app.explorer_queues[0].items.len(), 1);

    // 3b. Test finding queue for path & auto-selection in preview
    assert_eq!(app.explorer_find_queue_for_path(&test_file), Some(0));
    assert_eq!(app.explorer_find_queue_for_path(&PathBuf::from("/non/existent")), None);
    app.explorer_preview_queue_pick = None;
    app.explorer_open_preview(test_file.clone());
    assert_eq!(app.explorer_preview_queue_pick, Some(0));

    // 4. Test explicit remove
    app.explorer_remove_from_queue(0, test_file.clone());
    assert!(!app.explorer_is_in_queue(0, &test_file));
    assert_eq!(app.explorer_queues[0].items.len(), 0);
}

#[test]
fn test_explorer_performance_and_epoch_invariants() {
    let mut app = SortwiseApp::default();
    let initial_epoch = app.navigation_epoch.load(std::sync::atomic::Ordering::SeqCst);
    assert!(initial_epoch >= 1);

    // Reload bumps epoch
    app.explorer_reload();
    let new_epoch = app.navigation_epoch.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(new_epoch, initial_epoch + 1);

    // Sorting is case-insensitive and zero-alloc
    app.explorer_entries = vec![
        ExplorerEntry {
            name: "b_file.txt".to_string(),
            path: PathBuf::from("/test/b_file.txt"),
            is_dir: false,
            size: 10,
            modified: None,
        },
        ExplorerEntry {
            name: "A_folder".to_string(),
            path: PathBuf::from("/test/A_folder"),
            is_dir: true,
            size: 0,
            modified: None,
        },
        ExplorerEntry {
            name: "a_file.txt".to_string(),
            path: PathBuf::from("/test/a_file.txt"),
            is_dir: false,
            size: 20,
            modified: None,
        },
    ];
    app.explorer_sort_key = ExplorerSortKey::Name;
    app.explorer_sort_asc = true;
    app.explorer_sort_entries();

    // Folders first:
    assert!(app.explorer_entries[0].is_dir);
    assert_eq!(app.explorer_entries[0].name, "A_folder");
    // Then files sorted case-insensitively:
    assert_eq!(app.explorer_entries[1].name, "a_file.txt");
    assert_eq!(app.explorer_entries[2].name, "b_file.txt");
}

#[test]
fn test_unique_destinations_prevent_overwrites() {
    let temp = std::env::temp_dir().join(format!(
        "sfm_collision_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp).unwrap();

    // 1. Test unique_file_dest:
    let f1 = temp.join("report.pdf");
    std::fs::write(&f1, b"original").unwrap();

    let src_file = PathBuf::from("/anywhere/report.pdf");
    let dest1 = unique_file_dest(&temp, &src_file);
    assert_eq!(dest1, temp.join("report_1.pdf"));

    // If report_1.pdf also exists:
    std::fs::write(&dest1, b"first collision").unwrap();
    let dest2 = unique_file_dest(&temp, &src_file);
    assert_eq!(dest2, temp.join("report_2.pdf"));

    // 2. Test unique_dir_dest:
    let d1 = temp.join("Photos");
    std::fs::create_dir_all(&d1).unwrap();

    let dest_d1 = unique_dir_dest(&temp, "Photos");
    assert_eq!(dest_d1, temp.join("Photos_2"));

    // If Photos_2 also exists:
    std::fs::create_dir_all(&dest_d1).unwrap();
    let dest_d2 = unique_dir_dest(&temp, "Photos");
    assert_eq!(dest_d2, temp.join("Photos_3"));

    // 3. Test fast_move safety guard:
    let dummy_src = temp.join("dummy.txt");
    std::fs::write(&dummy_src, b"test").unwrap();
    let err = fast_move(&dummy_src, &f1);
    assert!(err.is_err());
    assert_eq!(err.unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);

    // 4. Test fast_move_dir safety guard:
    let dummy_dir = temp.join("dummy_dir");
    std::fs::create_dir_all(&dummy_dir).unwrap();
    let err_dir = fast_move_dir(&dummy_dir, &d1);
    assert!(err_dir.is_err());
    assert_eq!(err_dir.unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn test_movable_queue_individual_and_bulk_move_with_renaming() {
    let temp = std::env::temp_dir().join(format!(
        "sfm_movable_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let src_dir = temp.join("source");
    let dest_a = temp.join("Person_A");
    let dest_b = temp.join("Person_B");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dest_a).unwrap();
    std::fs::create_dir_all(&dest_b).unwrap();

    // Populate source video files
    let vid1 = src_dir.join("clip1.mp4");
    let vid2 = src_dir.join("clip2.mp4");
    let vid3 = src_dir.join("clip3.mp4");
    std::fs::write(&vid1, b"video 1 content").unwrap();
    std::fs::write(&vid2, b"video 2 content").unwrap();
    std::fs::write(&vid3, b"video 3 content").unwrap();

    // Create an existing file in dest_a with the exact same name clip1.mp4
    std::fs::write(dest_a.join("clip1.mp4"), b"existing file").unwrap();

    let mut app = SortwiseApp::default();
    app.explorer_dir = src_dir.clone();

    // Register destination queues
    app.explorer_queues.push(FileQueue {
        name: "Person A".to_string(),
        target: dest_a.clone(),
        items: Vec::new(),
    });
    app.explorer_queues.push(FileQueue {
        name: "Person B".to_string(),
        target: dest_b.clone(),
        items: Vec::new(),
    });

    // 1. Stage items into movable queue
    app.toggle_movable(vid1.clone());
    app.toggle_movable(vid2.clone());
    app.toggle_movable(vid3.clone());

    assert_eq!(app.movable_items.len(), 3);
    assert!(app.is_in_movable(&vid1));
    assert!(app.is_in_movable(&vid2));
    assert!(app.is_in_movable(&vid3));

    // 2. Assign queues
    app.movable_items[0].target_queue = Some("Person A".to_string());
    app.movable_items[1].target_queue = Some("Person B".to_string());
    // item 2 left unassigned for now

    // 3. Move individual item 0 (clip1.mp4 to Person A)
    app.start_movable_move(Some(0));
    wait_for_movable_move(&mut app);
    assert_eq!(app.movable_items.len(), 2);
    assert!(!vid1.exists(), "Original file should be moved");
    // Check automatic non-destructive renaming
    assert!(dest_a.join("clip1.mp4").exists(), "Existing destination file must remain intact");
    assert!(dest_a.join("clip1_1.mp4").exists(), "Colliding file should be automatically renamed to clip1_1.mp4");

    // 4. Assign queue for remaining item and move bulk
    app.movable_items[1].target_queue = Some("Person B".to_string());
    app.start_movable_move(None);
    wait_for_movable_move(&mut app);
    assert_eq!(app.explorer_status.as_deref(), Some("Moved 2 item(s)."));
    assert_eq!(app.movable_items.len(), 0);
    assert!(dest_b.join("clip2.mp4").exists());
    assert!(dest_b.join("clip3.mp4").exists());

    let _ = std::fs::remove_dir_all(&temp);
}

fn files_tab_rec(path: &str) -> sortwise::recursive_files::RecursiveFile {
    sortwise::recursive_files::RecursiveFile {
        path: PathBuf::from(path),
        rel: path.to_string(),
        size: 1,
        modified: None,
    }
}

#[test]
fn files_tab_stage_goes_to_folder_queue_or_items_never_both() {
    let mut app = SortwiseApp::default();
    app.explorer_queues.push(FileQueue { name: "A".into(), target: PathBuf::from("/dest/a"), items: vec![] });
    app.explorer_queues.push(FileQueue { name: "B".into(), target: PathBuf::from("/dest/b"), items: vec![] });
    let f = PathBuf::from("/x/a.mp4");

    // Picking a queue lists the file in that folder queue (Folder queues tab).
    assert_eq!(app.explorer_files_stage(std::slice::from_ref(&f), Some("A".into())), 1);
    assert_eq!(app.explorer_queues[0].items, vec![f.clone()]);
    assert!(app.movable_items.is_empty());
    assert_eq!(app.explorer_files_stage_of(&f), FilesStage::Queue("A".into()));

    // Re-picking another queue moves it there; it is never in two queues.
    app.explorer_files_stage(std::slice::from_ref(&f), Some("B".into()));
    assert!(app.explorer_queues[0].items.is_empty());
    assert_eq!(app.explorer_queues[1].items, vec![f.clone()]);

    // No queue: goes to Items only.
    app.explorer_files_stage(std::slice::from_ref(&f), None);
    assert!(app.explorer_queues.iter().all(|q| q.items.is_empty()));
    assert_eq!(app.explorer_files_stage_of(&f), FilesStage::Item(None));

    // Unknown queue changes nothing.
    assert_eq!(app.explorer_files_stage(std::slice::from_ref(&f), Some("Nope".into())), 0);
    assert_eq!(app.explorer_files_stage_of(&f), FilesStage::Item(None));

    // Added from the preview modal (folder queue path) is seen by the Files view too.
    app.explorer_files_unstage(&f);
    assert_eq!(app.explorer_files_stage_of(&f), FilesStage::None);
    let before = app.explorer_files_staged_fingerprint();
    app.explorer_add_to_queue(0, f.clone());
    assert!(app.explorer_files_staged_set().contains(&f));
    assert_ne!(before, app.explorer_files_staged_fingerprint());
}

#[test]
fn files_tab_moved_file_leaves_list_and_folder_move_removes_children() {
    let mut app = SortwiseApp::default();
    app.explorer_files = vec![
        files_tab_rec("/x/a.mp4"),
        files_tab_rec("/x/dir/b.mp4"),
        files_tab_rec("/x/dir/c.zip"),
        files_tab_rec("/x/d.zip"),
    ];
    app.explorer_files_checked.insert(PathBuf::from("/x/a.mp4"));

    app.explorer_files_forget_path(Path::new("/x/a.mp4"), false);
    assert_eq!(app.explorer_files.len(), 3);
    assert!(!app.explorer_files_checked.contains(Path::new("/x/a.mp4")));

    // A moved folder takes every file under it with it, but not similar prefixes.
    app.explorer_files_forget_path(Path::new("/x/dir"), true);
    let left: Vec<&str> = app.explorer_files.iter().map(|f| f.rel.as_str()).collect();
    assert_eq!(left, vec!["/x/d.zip"]);
}

#[test]
fn files_tab_prune_drops_missing_and_refresh_sets_rescan() {
    let temp = std::env::temp_dir().join(format!(
        "sfm_files_prune_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp).unwrap();
    let keep = temp.join("keep.zip");
    std::fs::write(&keep, b"k").unwrap();
    let mut app = SortwiseApp::default();
    app.explorer_files = vec![
        files_tab_rec(&keep.to_string_lossy()),
        files_tab_rec(&temp.join("gone.zip").to_string_lossy()),
    ];
    app.explorer_files_checked.insert(temp.join("gone.zip"));

    app.explorer_files_prune_missing();
    assert_eq!(app.explorer_files.len(), 1);
    assert!(app.explorer_files_checked.is_empty());

    app.explorer_files_refresh();
    assert!(app.explorer_files_rescan);
    let _ = std::fs::remove_dir_all(&temp);
}

fn scratch_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "sfm_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn moving_a_folder_into_itself_is_refused_and_nothing_is_lost() {
    let root = scratch_dir("move_into_self");
    let src = root.join("Album");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    std::fs::write(src.join("a.jpg"), b"photo").unwrap();

    let err = fast_move_dir(&src, &src.join("sub").join("Album")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(std::fs::read(src.join("a.jpg")).unwrap(), b"photo");
    assert!(!src.join("sub").join("Album").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn moving_onto_an_existing_name_keeps_both_files() {
    let root = scratch_dir("keep_both");
    let src_dir = root.join("from");
    let dst_dir = root.join("to");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dst_dir).unwrap();
    std::fs::write(dst_dir.join("clip.mp4"), b"already here").unwrap();
    let src = src_dir.join("clip.mp4");
    std::fs::write(&src, b"incoming").unwrap();

    let dest = unique_file_dest(&dst_dir, &src);
    assert_eq!(dest, dst_dir.join("clip_1.mp4"));
    fast_move(&src, &dest).unwrap();
    assert_eq!(std::fs::read(dst_dir.join("clip.mp4")).unwrap(), b"already here");
    assert_eq!(std::fs::read(dst_dir.join("clip_1.mp4")).unwrap(), b"incoming");
    assert!(!src.exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn unique_destinations_keep_non_utf8_names() {
    use std::os::unix::ffi::OsStrExt;
    let target = std::path::Path::new("/nonexistent_sfm_target");
    let name = std::ffi::OsStr::from_bytes(b"caf\xe9.jpg");
    let dest = unique_file_dest(target, &std::path::Path::new("/x").join(name));
    assert_eq!(dest.file_name().unwrap(), name);
    let folder = std::ffi::OsStr::from_bytes(b"f\xe9te");
    assert_eq!(unique_dir_dest(target, folder).file_name().unwrap(), folder);
}

/// Drive the background Items move to completion, as the frame loop would.
fn wait_for_movable_move(app: &mut SortwiseApp) {
    let start = std::time::Instant::now();
    while app.movable_moving {
        assert!(start.elapsed() < std::time::Duration::from_secs(10), "move did not finish");
        std::thread::sleep(std::time::Duration::from_millis(5));
        app.poll_movable_move();
    }
}

#[test]
fn a_failed_items_move_stays_staged() {
    let root = scratch_dir("items_fail");
    let target = root.join("dest");
    std::fs::create_dir_all(&target).unwrap();
    let mut app = SortwiseApp::default();
    app.explorer_dir = root.clone();
    app.explorer_queues.push(FileQueue { name: "Q".into(), target: target.clone(), items: Vec::new() });
    let missing = root.join("gone.mp4");
    app.movable_items.push(MovableItem { path: missing.clone(), target_queue: Some("Q".into()) });

    app.start_movable_move(None);
    wait_for_movable_move(&mut app);
    assert_eq!(app.movable_items.len(), 1, "failed item must stay staged");
    assert!(app.explorer_status.as_deref().unwrap_or("").contains("1 failed"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_file_is_staged_in_only_one_place() {
    let mut app = SortwiseApp::default();
    for name in ["A", "B"] {
        app.explorer_queues.push(FileQueue { name: name.into(), target: PathBuf::from(format!("/t/{name}")), items: Vec::new() });
    }
    let f = PathBuf::from("/src/clip.mp4");
    app.toggle_movable(f.clone());
    app.explorer_add_to_queue(0, f.clone());
    assert!(app.movable_items.is_empty(), "leaves Items when queued");
    app.explorer_add_to_queue(1, f.clone());
    assert!(app.explorer_queues[0].items.is_empty(), "moved out of queue A");
    assert_eq!(app.explorer_queues[1].items, vec![f.clone()]);
    assert_eq!(app.explorer_status.as_deref(), Some("Moved “clip.mp4” from “A” to “B”."));
}

fn wait_for_extract(app: &mut SortwiseApp) {
    let start = std::time::Instant::now();
    while app.extract_rx.is_some() {
        assert!(start.elapsed() < std::time::Duration::from_secs(10), "extract did not finish");
        std::thread::sleep(std::time::Duration::from_millis(5));
        app.poll_extract();
    }
}

#[test]
fn extracting_a_zip_never_touches_an_existing_folder_and_cleans_up_on_failure() {
    let root = scratch_dir("unzip");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src").join("a.txt"), b"hello").unwrap();
    let zip = root.join("photos.zip");
    let ok = std::process::Command::new("/usr/bin/zip")
        .current_dir(root.join("src"))
        .arg("-q")
        .arg(&zip)
        .arg("a.txt")
        .status()
        .unwrap();
    assert!(ok.success());
    // A folder with the archive's name already exists: it must stay untouched.
    std::fs::create_dir_all(root.join("photos")).unwrap();
    std::fs::write(root.join("photos").join("mine.txt"), b"keep").unwrap();

    let mut app = SortwiseApp::default();
    app.explorer_dir = root.clone();
    app.explorer_extract_zip(&zip);
    wait_for_extract(&mut app);
    let banner = app.extract_banner.clone().expect("extract succeeded");
    assert_eq!(banner.dest, root.join("photos_2"));
    assert_eq!(std::fs::read(root.join("photos_2").join("a.txt")).unwrap(), b"hello");
    assert_eq!(std::fs::read(root.join("photos").join("mine.txt")).unwrap(), b"keep");

    // A broken archive fails and leaves no half-made folder behind.
    let bad = root.join("broken.zip");
    std::fs::write(&bad, b"not a zip").unwrap();
    app.extract_banner = None;
    app.explorer_extract_zip(&bad);
    wait_for_extract(&mut app);
    assert!(app.extract_banner.is_none());
    assert!(!root.join("broken").exists());
    assert!(bad.exists(), "the archive itself is never touched");
    let _ = std::fs::remove_dir_all(&root);
}

/// Real cross-volume moves onto a FAT32 disk image (rename fails, copy fallback runs).
/// macOS only; needs `hdiutil`. Run: cargo test --bin sortwise cross_volume -- --ignored
#[cfg(target_os = "macos")]
#[test]
#[ignore]
fn cross_volume_moves_copy_then_delete_and_keep_both_on_collision() {
    let root = scratch_dir("xvol");
    let dmg = root.join("vol.dmg");
    let mnt = root.join("mnt");
    std::fs::create_dir_all(&mnt).unwrap();
    let run = |args: &[&str]| {
        let ok = std::process::Command::new("hdiutil").args(args).status().unwrap();
        assert!(ok.success(), "hdiutil {args:?}");
    };
    run(&["create", "-quiet", "-size", "40m", "-fs", "MS-DOS FAT32", "-volname", "SFMX", dmg.to_str().unwrap()]);
    run(&["attach", "-quiet", "-nobrowse", "-mountpoint", mnt.to_str().unwrap(), dmg.to_str().unwrap()]);

    let result = std::panic::catch_unwind(|| {
        let src = root.join("src");
        std::fs::create_dir_all(src.join("Trip/day1")).unwrap();
        let data: Vec<u8> = (0u8..=255).cycle().take(3 * 1024 * 1024).collect();
        std::fs::write(src.join("clip.mp4"), &data).unwrap();
        std::fs::write(src.join("Trip/day1/a.jpg"), b"photo-a").unwrap();
        std::fs::write(mnt.join("clip.mp4"), b"already on the drive").unwrap();

        // File: lands as clip_1.mp4, existing file untouched, source removed.
        assert!(move_into_dir(&src.join("clip.mp4"), &mnt));
        assert_eq!(std::fs::read(mnt.join("clip.mp4")).unwrap(), b"already on the drive");
        assert_eq!(std::fs::read(mnt.join("clip_1.mp4")).unwrap(), data);
        assert!(!src.join("clip.mp4").exists());

        // Folder: copied recursively, then the source is removed.
        assert!(move_into_dir(&src.join("Trip"), &mnt));
        assert_eq!(std::fs::read(mnt.join("Trip/day1/a.jpg")).unwrap(), b"photo-a");
        assert!(!src.join("Trip").exists());
    });

    run(&["detach", "-quiet", mnt.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&root);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
