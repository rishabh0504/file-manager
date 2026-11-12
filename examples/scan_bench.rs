//! Scan benchmark (Implementation-Plan Phase 0 and 3).
//!
//! Usage:
//!   cargo run --release --example scan_bench -- <root> [root...]
//!
//! Cold run: hash cache disabled. The real user cache is never touched.
//! Warm run (optional): set `SFM_BENCH_CACHE=<path to a .sqlite file>`. That file is a
//! scratch cache for the benchmark; the same run is done twice (cold, then warm).
//!
//! Prints enumeration and hashing time, time to first groups (streaming), groups,
//! reclaimable bytes, peak RSS, and the drive facts the thread policy uses.

use sortwise::cache::HashCache;
use sortwise::cancel::CancellationToken;
use sortwise::pipeline::run_dedup_streaming;
use sortwise::platform::volume_info;
use sortwise::scanner::parallel_scan_roots;
use sortwise::types::{DuplicateGroup, ScanConfig, ScanProgress};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

fn peak_rss_bytes() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage writes into the provided, zeroed rusage struct.
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return 0;
    }
    // SAFETY: getrusage succeeded, so the struct is initialized.
    let max = unsafe { usage.assume_init() }.ru_maxrss as u64;
    // macOS reports bytes; Linux reports kilobytes.
    if cfg!(target_os = "macos") { max } else { max * 1024 }
}

fn run(roots: &[PathBuf], use_cache: Option<&PathBuf>, label: &str) {
    let cfg = ScanConfig {
        roots: roots.to_vec(),
        min_file_size: 4 * 1024,
        skip_system_dirs: false,
        use_hash_cache: use_cache.is_some(),
        ..ScanConfig::default()
    };
    let token = CancellationToken::new();
    let progress = Arc::new(Mutex::new(ScanProgress::default()));

    let t_scan = Instant::now();
    let entries = parallel_scan_roots(roots, &cfg, &token, |_, _| {});
    let scan_s = t_scan.elapsed().as_secs_f64();
    let files = entries.len();

    let cache = match use_cache {
        Some(path) => {
            let t = Instant::now();
            let c = HashCache::open(path, vec![]).expect("open bench cache");
            println!("{label}.cache_load_s={:.3}", t.elapsed().as_secs_f64());
            Some(c)
        }
        None => None,
    };

    let (live_tx, live_rx) = mpsc::channel::<Vec<DuplicateGroup>>();
    let t_dedup = Instant::now();
    let worker = std::thread::spawn(move || {
        
        run_dedup_streaming(entries, &cfg, &token, progress, cache, live_tx)
    });
    let mut first_groups_s: Option<f64> = None;
    for batch in live_rx.iter() {
        if first_groups_s.is_none() && !batch.is_empty() {
            first_groups_s = Some(t_dedup.elapsed().as_secs_f64());
        }
    }
    let result = worker.join().expect("dedup thread");
    let dedup_s = t_dedup.elapsed().as_secs_f64();

    println!("{label}.files_enumerated={files}");
    println!("{label}.enumerate_s={scan_s:.2}");
    println!("{label}.hash_and_group_s={dedup_s:.2}");
    println!("{label}.total_s={:.2}", scan_s + dedup_s);
    println!(
        "{label}.first_groups_s={}",
        first_groups_s.map(|s| format!("{s:.2}")).unwrap_or_else(|| "none".into())
    );
    println!("{label}.groups={}", result.duplicate_groups.len());
    println!("{label}.reclaimable_bytes={}", result.stats.total_wasted_bytes);
    println!("{label}.peak_rss_mb={:.1}", peak_rss_bytes() as f64 / (1024.0 * 1024.0));
}

fn main() {
    let roots: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: scan_bench <root> [root...]");
        std::process::exit(2);
    }
    for r in &roots {
        let v = volume_info(r);
        println!("volume[{}].solid_state={:?} removable={}", r.display(), v.solid_state, v.removable);
    }

    run(&roots, None, "cold");
    if let Ok(path) = std::env::var("SFM_BENCH_CACHE") {
        let path = PathBuf::from(path);
        let _ = std::fs::remove_file(&path);
        run(&roots, Some(&path), "cache_first");
        run(&roots, Some(&path), "cache_warm");
    }
}
