//! Streaming must not change the answer (Implementation-Plan Phase 3, P-6).
//! `run_dedup_streaming` and `run_dedup` must return the same duplicate groups, and the
//! streamed batches must add up to the same set.

use sortwise::cancel::CancellationToken;
use sortwise::pipeline::{run_dedup, run_dedup_streaming};
use sortwise::scanner::parallel_scan_roots;
use sortwise::types::{DuplicateGroup, ScanConfig, ScanProgress};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};

fn make_tree(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sfm_parity_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    // Three duplicate sets of different sizes, plus unique files of the same sizes.
    for set in 0..3u8 {
        let payload: Vec<u8> = (0..(20_000 + set as usize * 7_000)).map(|i| (i as u8).wrapping_add(set)).collect();
        for copy in 0..3 {
            std::fs::write(dir.join(format!("set{set}_{copy}.jpg")), &payload).unwrap();
        }
        let unique: Vec<u8> = payload.iter().map(|b| b.wrapping_add(1)).collect();
        std::fs::write(dir.join("sub").join(format!("unique{set}.jpg")), unique).unwrap();
    }
    dir
}

fn signature(groups: &[DuplicateGroup]) -> BTreeSet<(String, Vec<PathBuf>, usize)> {
    groups
        .iter()
        .map(|g| {
            let mut paths: Vec<PathBuf> = g.files.iter().map(|f| f.path.clone()).collect();
            paths.sort();
            (g.hash.clone(), paths, g.files.len())
        })
        .collect()
}

#[test]
fn streaming_and_batch_modes_agree() {
    let dir = make_tree("agree");
    let cfg = ScanConfig {
        roots: vec![dir.clone()],
        min_file_size: 1,
        skip_system_dirs: false,
        use_hash_cache: false,
        ..ScanConfig::default()
    };
    let token = CancellationToken::new();

    let batch = run_dedup(
        parallel_scan_roots(&cfg.roots, &cfg, &token, |_, _| {}),
        &cfg,
        &token,
        Arc::new(Mutex::new(ScanProgress::default())),
        None,
    );

    let (tx, rx) = mpsc::channel();
    let streamed = run_dedup_streaming(
        parallel_scan_roots(&cfg.roots, &cfg, &token, |_, _| {}),
        &cfg,
        &token,
        Arc::new(Mutex::new(ScanProgress::default())),
        None,
        tx,
    );
    let live: Vec<DuplicateGroup> = rx.try_iter().flatten().collect();

    assert_eq!(batch.duplicate_groups.len(), 3);
    assert_eq!(signature(&batch.duplicate_groups), signature(&streamed.duplicate_groups));
    assert_eq!(signature(&batch.duplicate_groups), signature(&live));
    assert_eq!(batch.stats.total_wasted_bytes, streamed.stats.total_wasted_bytes);

    let _ = std::fs::remove_dir_all(&dir);
}
