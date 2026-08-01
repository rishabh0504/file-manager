//! Headless duplicate scan (`cargo run --features cli --bin sfm-cli`).
use smart_file_manager::cache::HashCache;
use smart_file_manager::cancel::CancellationToken;
use smart_file_manager::pipeline::run_dedup;
use smart_file_manager::scanner::parallel_scan_roots;
use smart_file_manager::types::{FolderConfig, ScanConfig};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: sfm-cli <dir> [more dirs...]");
        eprintln!("Outputs JSON report to stdout.");
        std::process::exit(1);
    }

    let roots: Vec<PathBuf> = args.into_iter().map(PathBuf::from).collect();
    let folder_configs: Vec<FolderConfig> = roots
        .iter()
        .map(|r| FolderConfig {
            root: r.clone(),
            excluded_subdirs: Vec::new(),
        })
        .collect();

    let cfg = ScanConfig {
        roots: roots.clone(),
        folder_configs,
        root_order: roots.clone(),
        use_hash_cache: std::env::var("SFM_NO_CACHE").is_err(),
        ..Default::default()
    };

    let token = CancellationToken::new();
    let progress = Arc::new(Mutex::new(smart_file_manager::types::ScanProgress::default()));

    let entries = parallel_scan_roots(&roots, &cfg, &token, |_, _| {});
    let cache = if cfg.use_hash_cache {
        Some(HashCache::load())
    } else {
        None
    };
    let res = run_dedup(entries, &cfg, &token, Arc::clone(&progress), cache);

    let errors: Vec<_> = res
        .error_log
        .iter()
        .take(200)
        .map(|(p, e)| serde_json::json!({ "path": p.display().to_string(), "error": e }))
        .collect();

    let out = serde_json::json!({
        "stats": res.stats,
        "duplicate_groups": res.duplicate_groups,
        "folder_summaries": res.folder_summaries,
        "hard_link_group_count": res.hard_link_groups.len(),
        "errors": errors,
    });

    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
}
