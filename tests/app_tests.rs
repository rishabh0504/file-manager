use sortwise::app::SortwiseApp;
use sortwise::types::ExplorerEntry;
use std::path::PathBuf;

#[test]
fn test_system_entry_exclusions() {
    assert!(sortwise::app::is_system_entry(".git", true));
    assert!(sortwise::app::is_system_entry("node_modules", true));
    assert!(sortwise::app::is_system_entry("__pycache__", true));
    assert!(sortwise::app::is_system_entry("$recycle.bin", true));
    assert!(sortwise::app::is_system_entry(".DS_Store", false));
    assert!(sortwise::app::is_system_entry(".ds_store", false));
    assert!(sortwise::app::is_system_entry("._photo.jpg", false));
    assert!(sortwise::app::is_system_entry("thumbs.db", false));

    assert!(!sortwise::app::is_system_entry("Documents", true));
    assert!(!sortwise::app::is_system_entry(".config", true));
    assert!(!sortwise::app::is_system_entry(".secret_archive", true));
    assert!(!sortwise::app::is_system_entry(".hidden_photos", true));
    assert!(!sortwise::app::is_system_entry("vacation_2024", true));
    assert!(!sortwise::app::is_system_entry("photo.jpg", false));
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

    std::fs::write(temp_dir.join(".DS_Store"), b"junk").unwrap();
    std::fs::write(temp_dir.join("root_note.txt"), b"hello").unwrap();

    let mut app = SortwiseApp::default();
    app.explorer_dir = temp_dir.clone();
    app.explorer_reload();

    let entry_names: std::collections::HashSet<_> = app
        .explorer_entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();

    assert!(
        entry_names.contains("Albums"),
        "Normal folder should appear"
    );
    assert!(
        entry_names.contains("Invoices"),
        "Normal folder should appear"
    );
    assert!(
        entry_names.contains(".custom_hidden_folder"),
        "Custom hidden folder should appear (not filtered)"
    );
    assert!(
        !entry_names.contains(".git"),
        "System folder .git should be filtered"
    );
    assert!(
        !entry_names.contains("node_modules"),
        "System folder node_modules should be filtered"
    );
    assert!(
        !entry_names.contains(".DS_Store"),
        "System file .DS_Store should be filtered"
    );
    assert!(
        entry_names.contains("root_note.txt"),
        "Regular file should appear"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_duplicate_file_rename_on_collision() {
    let temp = std::env::temp_dir().join("test_dup_rename");
    std::fs::create_dir_all(&temp).unwrap();

    let file1 = temp.join("photo.jpg");
    std::fs::write(&file1, b"content1").unwrap();

    let mut app = SortwiseApp::default();
    app.explorer_dir = temp.clone();

    let dest = sortwise::app::unique_file_dest(&temp, &file1);
    assert_ne!(
        dest.file_name().unwrap(),
        "photo.jpg",
        "Should generate unique name"
    );
    assert!(dest.file_name().unwrap().to_string_lossy().starts_with("photo_"));

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn test_collapse_search_results_removes_duplicates() {
    let path1 = PathBuf::from("/tmp/file1.jpg");
    let path2 = PathBuf::from("/tmp/file2.jpg");
    let path1_dup = PathBuf::from("/tmp/file1.jpg");

    let entries = vec![
        ExplorerEntry {
            name: "file1.jpg".to_string(),
            path: path1,
            is_dir: false,
            size: 100,
            modified: None,
        },
        ExplorerEntry {
            name: "file2.jpg".to_string(),
            path: path2,
            is_dir: false,
            size: 200,
            modified: None,
        },
        ExplorerEntry {
            name: "file1.jpg".to_string(),
            path: path1_dup,
            is_dir: false,
            size: 100,
            modified: None,
        },
    ];

    let collapsed = sortwise::app::collapse_search_results(entries);
    assert_eq!(collapsed.len(), 2, "Duplicates should be removed");
}

#[test]
fn test_path_is_strict_descendant() {
    let parent = PathBuf::from("/home/user/photos");
    let child = PathBuf::from("/home/user/photos/2024");
    let same = PathBuf::from("/home/user/photos");
    let sibling = PathBuf::from("/home/user/videos");

    assert!(sortwise::app::path_is_strict_descendant(&child, &parent));
    assert!(!sortwise::app::path_is_strict_descendant(&same, &parent));
    assert!(!sortwise::app::path_is_strict_descendant(&sibling, &parent));
}

#[test]
fn test_collect_media_files_under() {
    let temp = std::env::temp_dir().join("test_media_collect");
    std::fs::create_dir_all(&temp).unwrap();

    std::fs::write(temp.join("photo.jpg"), b"jpg").unwrap();
    std::fs::write(temp.join("video.mp4"), b"mp4").unwrap();
    std::fs::write(temp.join("archive.zip"), b"zip").unwrap();
    std::fs::write(temp.join("other.txt"), b"txt").unwrap();

    let files = sortwise::app::collect_media_files_under(&temp, false);
    let names: Vec<_> = files
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect();

    assert!(names.contains(&"photo.jpg".to_string()));
    assert!(names.contains(&"video.mp4".to_string()));
    assert!(names.contains(&"archive.zip".to_string()));
    assert!(!names.contains(&"other.txt".to_string()));

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn test_movable_queue_operations() {
    let temp = std::env::temp_dir().join("test_movable");
    std::fs::create_dir_all(&temp).unwrap();

    let vid1 = temp.join("clip1.mp4");
    let vid2 = temp.join("clip2.mp4");
    let vid3 = temp.join("clip3.mp4");
    std::fs::write(&vid1, b"video1").unwrap();
    std::fs::write(&vid2, b"video2").unwrap();
    std::fs::write(&vid3, b"video3").unwrap();

    let dest_a = temp.join("Person A");
    let dest_b = temp.join("Person B");
    std::fs::create_dir_all(&dest_a).unwrap();
    std::fs::create_dir_all(&dest_b).unwrap();

    let mut app = SortwiseApp::default();
    app.explorer_dir = temp.clone();

    sortwise::operations::stage_paths(&mut app, &[vid1.clone(), vid2.clone(), vid3.clone()]);
    assert_eq!(app.movable_items.len(), 3);

    app.movable_items[0].target_queue = Some("Person A".to_string());
    app.explorer_queues.push(sortwise::types::FileQueue {
        name: "Person A".to_string(),
        target: dest_a.clone(),
        items: vec![],
    });
    app.explorer_queues.push(sortwise::types::FileQueue {
        name: "Person B".to_string(),
        target: dest_b.clone(),
        items: vec![],
    });

    let moved = sortwise::operations::move_single_movable_item(&mut app, 0);
    assert!(moved);
    assert_eq!(app.movable_items.len(), 2);
    assert!(!vid1.exists(), "Original file should be moved");
    assert!(dest_a.join("clip1.mp4").exists(), "File should exist at destination");

    app.movable_items[0].target_queue = Some("Person A".to_string());
    app.movable_items[1].target_queue = Some("Person B".to_string());
    let (success, failed) = sortwise::operations::move_all_movable_items(&mut app);
    assert_eq!(success, 2);
    assert_eq!(failed, 0);
    assert_eq!(app.movable_items.len(), 0);
    assert!(dest_b.join("clip2.mp4").exists());
    assert!(dest_b.join("clip3.mp4").exists());

    let _ = std::fs::remove_dir_all(&temp);
}
