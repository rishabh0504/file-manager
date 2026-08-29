use crate::cache::HashCache;
use crate::cancel::CancellationToken;
use crate::errors::FileError;
use crate::hasher::{compute_chunk_size, full_hash, partial_hash_from_reads};
use crate::heuristics::{classify, FileDisposition};
use crate::platform::FileId;
use crate::resolver::{build_duplicate_group, detect_cross_folder_conflicts};
use crate::types::{
    log_error, DuplicateGroup, FileEntry, FolderSummary, HardLinkGroup, KeepPolicy, ProgressPhase,
    ScanConfig, ScanProgress, ScanResult, ScanStats,
};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

/// Maximum hashing threads. BLAKE3 is faster than any single SSD, so the
/// bottleneck is I/O — more than 6 concurrent readers adds heat without
/// throughput gains on typical consumer hardware.
fn hash_thread_count() -> usize {
    // Half the logical cores, kept between 2 and 6.
    (num_cpus::get() / 2).max(2).min(6)
}

// MAX_GROUP_SIZE intentionally removed: chunking files of the same size into
// sub-groups caused silent cross-chunk duplicate blindspots. All files sharing
// a size now form one group and are processed together.

fn partition_hardlinks(entries: Vec<FileEntry>) -> (Vec<FileEntry>, Vec<HardLinkGroup>) {
    let mut map: HashMap<FileId, Vec<FileEntry>> = HashMap::new();
    for e in entries {
        map.entry(e.file_id).or_default().push(e);
    }
    let mut unique = Vec::new();
    let mut hard = Vec::new();
    for (_, mut g) in map {
        if g.len() == 1 {
            unique.push(g.pop().expect("one"));
        } else {
            hard.push(HardLinkGroup { files: g });
        }
    }
    (unique, hard)
}

fn apply_heuristics(entries: Vec<FileEntry>, config: &ScanConfig) -> Vec<FileEntry> {
    entries
        .into_iter()
        .filter(|e| {
            matches!(
                classify(e, config.skip_logs_and_locks),
                FileDisposition::HashNormally
            )
        })
        .collect()
}

fn bucket_by_size(entries: Vec<FileEntry>) -> (Vec<Vec<FileEntry>>, u64) {
    let mut map: HashMap<u64, Vec<FileEntry>> = HashMap::new();
    for e in entries {
        map.entry(e.size).or_default().push(e);
    }
    let mut bytes_in_hashing: u64 = 0;
    let mut out: Vec<Vec<FileEntry>> = Vec::new();
    for g in map.into_values() {
        if g.len() <= 1 {
            continue;
        }
        bytes_in_hashing += g.iter().map(|e| e.size).sum::<u64>();
        out.push(g);   // single group — no chunking, no duplicate blindspots
    }
    (out, bytes_in_hashing)
}

/// Per-group output collected during the parallel phase.
#[allow(dead_code)] // bytes_hashed/files_hashed reserved for future per-group progress
struct GroupOutcome {
    dup_groups: Vec<DuplicateGroup>,
    errors: Vec<(PathBuf, String)>,
    bytes_hashed: u64,
    files_hashed: u64,
}

/// Process one size-bucket end-to-end: partial hash → filter → full hash → group.
///
/// # Performance notes
/// - Runs concurrently with all other size-buckets via `into_par_iter()` in `run_dedup`.
/// - Inner iterations use `par_iter()` to parallelize massive size buckets.
///   Rayon's work-stealing safely prevents oversubscription inside the bounded pool.
/// - `mtime_secs` is read from `FileEntry` (pre-computed at scan time) — no extra `stat()`.
/// - `verify_group` is omitted: BLAKE3-256 full-hash collision probability is `2^-256`.
/// - Progress Mutex is locked once per group, not once per file.
fn process_size_group(
    group: Vec<FileEntry>,
    config: &ScanConfig,
    token: &CancellationToken,
    cache_shared: &Option<Arc<RwLock<HashCache>>>,
    progress: &Arc<Mutex<ScanProgress>>,
) -> GroupOutcome {
    let mut errors: Vec<(PathBuf, String)> = Vec::new();
    let mut dup_groups: Vec<DuplicateGroup> = Vec::new();
    let mut bytes_hashed: u64 = 0;
    let mut files_hashed: u64 = 0;

    // ── Stage A: Partial hash ────────────────────────────────────────────────
    let partial_results: Vec<(FileEntry, Result<[u8; 32], FileError>)> = group
        .par_iter()
        .map(|e| {
            if token.is_cancelled() {
                return (e.clone(), Err(FileError::Cancelled));
            }
            // mtime_secs pre-computed at scan time — no stat() syscall here.
            if let Some(cm) = cache_shared {
                if let Ok(g) = cm.read() {
                    if let Some(ent) = g.get(&e.path, e.size, e.mtime_secs) {
                        return (e.clone(), Ok(ent.partial_hash));
                    }
                }
            }
            let chunk = compute_chunk_size(e.size);
            let h = partial_hash_from_reads(&e.path, e.size, chunk, token);
            (e.clone(), h)
        })
        .collect();

    // One write lock for the whole group's partial-hash results.
    if let Some(cm) = cache_shared {
        let mut c = cm.write().unwrap();
        for (e, r) in &partial_results {
            if let Ok(hash) = r {
                if c.get(&e.path, e.size, e.mtime_secs).is_none() {
                    c.insert_partial(&e.path, e.size, e.mtime_secs, *hash);
                }
            }
        }
    }

    for (e, r) in &partial_results {
        if let Err(err) = r {
            log_error(&mut errors, e.path.clone(), err.clone());
        }
    }

    let mut by_partial: HashMap<[u8; 32], Vec<FileEntry>> = HashMap::new();
    for (e, r) in partial_results {
        if let Ok(h) = r {
            by_partial.entry(h).or_default().push(e);
        }
    }
    let mut full_candidates: Vec<FileEntry> = Vec::new();
    for (_, mut files) in by_partial {
        if files.len() <= 1 { continue; }
        full_candidates.append(&mut files);
    }

    if token.is_cancelled() || full_candidates.is_empty() {
        return GroupOutcome { dup_groups, errors, bytes_hashed, files_hashed };
    }

    // ── Stage B: Full hash ───────────────────────────────────────────────────
    let full_results: Vec<(FileEntry, Result<[u8; 32], FileError>)> = full_candidates
        .par_iter()
        .map(|e| {
            if token.is_cancelled() {
                return (e.clone(), Err(FileError::Cancelled));
            }
            if let Some(cm) = cache_shared {
                if let Ok(g) = cm.read() {
                    if let Some(ent) = g.get(&e.path, e.size, e.mtime_secs) {
                        if let Some(fh) = ent.full_hash {
                            return (e.clone(), Ok(fh));
                        }
                    }
                }
            }
            (e.clone(), full_hash(&e.path, e.size, token))
        })
        .collect();

    if let Some(cm) = cache_shared {
        let mut c = cm.write().unwrap();
        for (e, r) in &full_results {
            if let Ok(hash) = r {
                c.set_full(&e.path, e.size, e.mtime_secs, *hash);
            }
        }
    }

    // Accumulate locally, then lock progress once per group.
    let mut last_path: Option<PathBuf> = None;
    for (e, r) in &full_results {
        match r {
            Ok(_) => {
                bytes_hashed = bytes_hashed.saturating_add(e.size);
                files_hashed = files_hashed.saturating_add(1);
                last_path = Some(e.path.clone());
            }
            Err(err) => { log_error(&mut errors, e.path.clone(), err.clone()); }
        }
    }
    if files_hashed > 0 {
        let mut p = progress.lock().unwrap();
        p.bytes_processed = p.bytes_processed.saturating_add(bytes_hashed);
        p.files_hashed = p.files_hashed.saturating_add(files_hashed);
        if let Some(path) = last_path {
            p.current_path = path;
        }
    }

    // ── Stage C: Build duplicate groups ──────────────────────────────────────
    // verify_group omitted — BLAKE3-256 full hash is collision-proof (2^-256).
    let mut by_full: HashMap<[u8; 32], Vec<FileEntry>> = HashMap::new();
    for (e, r) in full_results {
        if let Ok(h) = r {
            by_full.entry(h).or_default().push(e);
        }
    }
    for (hash, files) in by_full {
        if files.len() < 2 || token.is_cancelled() { continue; }
        let dg = build_duplicate_group(hash, files, KeepPolicy::SmartScore, config);
        if dg.files.len() >= 2 {
            dup_groups.push(dg);
        }
    }

    GroupOutcome { dup_groups, errors, bytes_hashed, files_hashed }
}

pub fn run_dedup(
    entries: Vec<FileEntry>,
    config: &ScanConfig,
    token: &CancellationToken,
    progress: Arc<Mutex<ScanProgress>>,
    cache: Option<HashCache>,
) -> ScanResult {
    let cache_shared: Option<Arc<RwLock<HashCache>>> =
        if config.use_hash_cache { cache.map(|c| Arc::new(RwLock::new(c))) } else { None };

    let total_input = entries.len() as u64;
    let mut error_log: Vec<(std::path::PathBuf, String)> = Vec::new();

    let (unique_entries, hard_link_groups) = partition_hardlinks(entries);
    let hard_link_group_count = hard_link_groups.len() as u64;
    let to_hash = apply_heuristics(unique_entries, config);
    let (size_groups, bytes_total) = bucket_by_size(to_hash);

    {
        let mut p = progress.lock().unwrap();
        p.bytes_total = bytes_total;
        p.files_to_hash_total = size_groups.iter().map(|g| g.len() as u64).sum();
        p.phase = ProgressPhase::Processing {
            stage_label: "Hashing".into(),
            stage: 1,
            stages_total: 2,
        };
    }

    // Build a bounded thread pool so we don't peg every core at 100%.
    // BLAKE3 is I/O-bound on SSDs: 2-6 concurrent readers is the sweet spot.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(hash_thread_count())
        .thread_name(|i| format!("sfm-hash-{i}"))
        .build()
        .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build().unwrap());

    // All size-groups processed in parallel inside the bounded pool.
    let outcomes: Vec<GroupOutcome> = pool.install(|| {
        size_groups
            .into_par_iter()
            .filter(|_| !token.is_cancelled())
            .map(|group| process_size_group(group, config, token, &cache_shared, &progress))
            .collect()
    });

    let mut duplicate_groups: Vec<DuplicateGroup> = Vec::new();
    for outcome in outcomes {
        duplicate_groups.extend(outcome.dup_groups);
        error_log.extend(outcome.errors);
    }
    let groups_found = duplicate_groups.len() as u64;

    {
        let mut p = progress.lock().unwrap();
        p.phase = ProgressPhase::Finishing;
    }

    if let Some(cm) = &cache_shared {
        let _ = cm.write().unwrap().save();
    }

    let cross = detect_cross_folder_conflicts(&duplicate_groups);
    let total_wasted: u64 = duplicate_groups.iter().map(|g| g.total_wasted_bytes).sum();
    let folder_summaries = summarize_folders(&duplicate_groups, &config.roots);

    ScanResult {
        duplicate_groups,
        hard_link_groups,
        stats: ScanStats {
            files_scanned: total_input,
            files_skipped_error: error_log.len() as u64,
            duplicate_groups: groups_found,
            total_wasted_bytes: total_wasted,
            partial: token.is_cancelled(),
            hard_link_groups: hard_link_group_count,
        },
        error_log,
        folder_summaries,
        cross_folder_conflicts: cross,
    }
}

/// Re-hash only paths that appeared in the last duplicate report (same roots/config as the original scan).
/// Reflects Finder moves, deletes, and edits without walking whole trees again.
pub fn refresh_duplicates_from_result(
    previous: &ScanResult,
    config: &ScanConfig,
    token: &CancellationToken,
    progress: Arc<Mutex<ScanProgress>>,
    cache: Option<HashCache>,
) -> ScanResult {
    let mut seen = HashSet::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    for g in &previous.duplicate_groups {
        for f in &g.files {
            if seen.insert(f.path.clone()) {
                paths.push(f.path.clone());
            }
        }
    }

    let mut refresh_errors: Vec<(PathBuf, String)> = Vec::new();
    let mut entries: Vec<FileEntry> = Vec::new();
    for p in &paths {
        match crate::scanner::file_entry_for_path(p, config, false) {
            Some(e) => entries.push(e),
            None => {
                let detail = if !p.exists() {
                    "missing or moved"
                } else {
                    "not a regular file"
                };
                log_error(
                    &mut refresh_errors,
                    p.clone(),
                    FileError::Io(format!("Refresh: {detail}")),
                );
            }
        }
    }

    let mut new = run_dedup(entries, config, token, progress, cache);

    // All errors are preserved — no truncation — so the user sees every
    // file that moved or became inaccessible since the last scan.
    let mut merged = refresh_errors;
    merged.append(&mut new.error_log);

    ScanResult {
        duplicate_groups: new.duplicate_groups,
        hard_link_groups: previous.hard_link_groups.clone(),
        stats: ScanStats {
            files_scanned: paths.len() as u64,
            files_skipped_error: merged.len() as u64,
            duplicate_groups: new.stats.duplicate_groups,
            total_wasted_bytes: new.stats.total_wasted_bytes,
            partial: new.stats.partial,
            hard_link_groups: previous.stats.hard_link_groups,
        },
        error_log: merged,
        folder_summaries: new.folder_summaries,
        cross_folder_conflicts: new.cross_folder_conflicts,
    }
}

pub fn summarize_folders(groups: &[DuplicateGroup], roots: &[std::path::PathBuf]) -> Vec<FolderSummary> {
    roots
        .iter()
        .enumerate()
        .map(|(i, root)| {
            let mut dup_groups = 0u32;
            let mut wasted = 0u64;
            for g in groups {
                if !g.files.iter().any(|f| f.root_index == i) {
                    continue;
                }
                dup_groups += 1;
                for f in &g.files {
                    if f.root_index == i && !f.is_keep {
                        wasted += f.size;
                    }
                }
            }
            FolderSummary {
                root: root.clone(),
                files_scanned: 0,
                duplicate_groups: dup_groups,
                total_wasted_bytes: wasted,
                error_count: 0,
                external_hardlinks: 0,
            }
        })
        .collect()
}
