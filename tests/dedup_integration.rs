use smart_file_manager::cancel::CancellationToken;
use smart_file_manager::hasher::compute_chunk_size;
use smart_file_manager::pipeline::run_dedup;
use smart_file_manager::scanner::parallel_scan_roots;
use smart_file_manager::types::{FolderConfig, ScanConfig, ScanProgress};
use std::sync::{Arc, Mutex};

#[test]
fn chunk_size_caps_at_8k() {
    assert_eq!(compute_chunk_size(1024 * 1024), 8192);
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
    let a = dir.join("a.bin");
    let b = dir.join("b.bin");
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
