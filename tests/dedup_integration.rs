use sortwise::cancel::CancellationToken;
use sortwise::hasher::compute_chunk_size;
use sortwise::pipeline::run_dedup;
use sortwise::scanner::parallel_scan_roots;
use sortwise::types::{FolderConfig, ScanConfig, ScanProgress};
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
        min_file_size: 1, // fixtures are small; the engine default is 4 KB
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


fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "sfm_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn scan_cfg(roots: Vec<std::path::PathBuf>, skip_system_dirs: bool) -> ScanConfig {
    ScanConfig {
        folder_configs: roots
            .iter()
            .map(|r| FolderConfig { root: r.clone(), excluded_subdirs: Vec::new() })
            .collect(),
        root_order: roots.clone(),
        roots,
        skip_hidden: false,
        skip_system_dirs,
        use_hash_cache: false,
        ..Default::default()
    }
}

fn dup_paths(cfg: &ScanConfig) -> Vec<Vec<std::path::PathBuf>> {
    let token = CancellationToken::new();
    let entries = parallel_scan_roots(&cfg.roots, cfg, &token, |_, _| {});
    let res = run_dedup(entries, cfg, &token, Arc::new(Mutex::new(ScanProgress::default())), None);
    res.duplicate_groups
        .iter()
        .map(|g| {
            let mut v: Vec<_> = g.files.iter().map(|f| f.path.clone()).collect();
            v.sort();
            v
        })
        .collect()
}

#[test]
fn real_copy_of_a_hard_linked_video_is_still_a_duplicate() {
    let dir = scratch("hardlink_plus_copy");
    let payload: Vec<u8> = (0u8..=250).cycle().take(50_000).collect();
    let a = dir.join("clip.mp4");
    std::fs::write(&a, &payload).unwrap();
    std::fs::hard_link(&a, dir.join("clip_link.mp4")).unwrap();
    let copy = dir.join("clip copy.mp4");
    std::fs::write(&copy, &payload).unwrap();

    let groups = dup_paths(&scan_cfg(vec![dir.clone()], false));
    assert_eq!(groups.len(), 1, "the separate copy must be found: {groups:?}");
    assert!(groups[0].contains(&copy));
    assert_eq!(groups[0].len(), 2, "one name per hard-link set: {groups:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn overlapping_scan_roots_do_not_hide_duplicates() {
    let dir = scratch("overlap_roots");
    let inner = dir.join("2020");
    std::fs::create_dir_all(&inner).unwrap();
    let payload: Vec<u8> = (0u8..=200).cycle().take(40_000).collect();
    let a = inner.join("trip.mov");
    let b = dir.join("trip_backup.mov");
    std::fs::write(&a, &payload).unwrap();
    std::fs::write(&b, &payload).unwrap();

    // Parent and child both added as roots: trip.mov is enumerated twice.
    let groups = dup_paths(&scan_cfg(vec![dir.clone(), inner.clone()], false));
    assert_eq!(groups, vec![vec![b.clone(), a.clone()].into_iter().collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_folders_named_like_linux_system_dirs_are_scanned() {
    let dir = scratch("dev_folder");
    let payload: Vec<u8> = (0u8..=180).cycle().take(30_000).collect();
    for sub in ["Dev", "run"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        std::fs::write(dir.join(sub).join("video.mp4"), &payload).unwrap();
    }
    let groups = dup_paths(&scan_cfg(vec![dir.clone()], true));
    assert_eq!(groups.len(), 1, "videos inside Dev/ and run/ must be scanned: {groups:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
