use smart_file_manager::cancel::CancellationToken;
use smart_file_manager::hasher::compute_chunk_size;
use smart_file_manager::pipeline::run_dedup;
use smart_file_manager::scanner::parallel_scan_roots;
use smart_file_manager::types::{FolderConfig, ScanConfig, ScanProgress};
use std::sync::{Arc, Mutex};

#[test]
fn chunk_size_caps_at_64k() {
    assert_eq!(compute_chunk_size(1024 * 1024), 65536);
    assert_eq!(compute_chunk_size(16), 4);
}

#[test]
fn finds_identical_files_in_folder() {
    let dir = std::env::temp_dir().join(format!(
        "sfm_dedup_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.png");
    let b = dir.join("b.png");
    let payload: Vec<u8> = (0u8..=255).cycle().take(20_000).collect();
    std::fs::write(&a, &payload).unwrap();
    std::fs::write(&b, &payload).unwrap();

    let roots = vec![dir.clone()];
    let folder_configs: Vec<FolderConfig> = vec![FolderConfig {
        root: dir.clone(),
        excluded_subdirs: Vec::new(),
    }];
    let cfg = ScanConfig {
        roots: roots.clone(),
        folder_configs,
        root_order: roots.clone(),
        skip_hidden: false,
        skip_system_dirs: false,
        use_hash_cache: false,
        ..Default::default()
    };

    let token = CancellationToken::new();
    let entries = parallel_scan_roots(&roots, &cfg, &token, |_, _| {});
    assert_eq!(entries.len(), 2, "expected two files scanned");

    let progress = Arc::new(Mutex::new(ScanProgress::default()));
    let res = run_dedup(entries, &cfg, &token, Arc::clone(&progress), None);

    assert!(
        !res.duplicate_groups.is_empty(),
        "expected at least one duplicate group"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_asymmetric_a_to_b_mode() {
    let base = std::env::temp_dir().join(format!(
        "sfm_asym_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let dir_a = base.join("source_a");
    let dir_b = base.join("dest_b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let payload1: Vec<u8> = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let payload2: Vec<u8> = vec![9, 9, 9, 9, 9, 9, 9, 9];

    // File 1 is in both A and B (duplicate across A and B)
    std::fs::write(dir_a.join("f1_a.jpg"), &payload1).unwrap();
    std::fs::write(dir_b.join("f1_b.jpg"), &payload1).unwrap();

    // File 2 is ONLY in B, twice (internal duplicates in B)
    std::fs::write(dir_b.join("f2_b1.jpg"), &payload2).unwrap();
    std::fs::write(dir_b.join("f2_b2.jpg"), &payload2).unwrap();

    let roots = vec![dir_a.clone(), dir_b.clone()];
    let cfg = ScanConfig {
        roots: roots.clone(),
        root_order: roots.clone(),
        asymmetric_split_index: Some(1), // index 0 is A, index 1 is B
        ..Default::default()
    };

    let token = CancellationToken::new();
    let entries = parallel_scan_roots(&roots, &cfg, &token, |_, _| {});
    assert_eq!(entries.len(), 4);

    let progress = Arc::new(Mutex::new(ScanProgress::default()));
    let res = run_dedup(entries, &cfg, &token, Arc::clone(&progress), None);

    // In asymmetric mode, only f1 should appear as a duplicate group. f2 (internal to B) must be completely omitted.
    assert_eq!(res.duplicate_groups.len(), 1, "Only files duplicating A in B should be reported");
    let g = &res.duplicate_groups[0];
    assert_eq!(g.files.len(), 2);
    assert!(g.files[0].is_keep, "Source A file must be marked as keep");
    assert_eq!(g.files[0].path, dir_a.join("f1_a.jpg"));
    assert!(!g.files[1].is_keep, "Destination B file must be marked as duplicate");
    assert_eq!(g.files[1].path, dir_b.join("f1_b.jpg"));

    let _ = std::fs::remove_dir_all(&base);
}

