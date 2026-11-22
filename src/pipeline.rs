use crate::cache::HashCache;
use crate::cancel::CancellationToken;
use crate::errors::FileError;
use crate::hasher::{compute_chunk_size, full_hash, partial_hash_from_reads};
use crate::heuristics::{classify, FileDisposition};
use crate::platform::FileId;
use crate::resolver::{build_duplicate_group, detect_cross_folder_conflicts};
use crate::types::{
    log_error, DuplicateGroup, FileEntry, FolderSummary, HardLinkGroup, KeepPolicy, ProgressPhase,
    IoProfile, ScanConfig, ScanProgress, ScanResult, ScanStats,
};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

/// Maximum hashing threads. BLAKE3 is faster than any single SSD, so the
/// bottleneck is I/O — more than 6 concurrent readers adds heat without
/// throughput gains on typical consumer hardware.
/// Reader threads for hashing (P-8, U7). `Auto` uses 2–6 only when every root is on
/// solid-state storage; anything else (spinning, network, unknown) gets 2 readers.
fn hash_thread_count(config: &ScanConfig) -> usize {
    let ssd_max = (num_cpus::get() / 2).clamp(2, 6);
    match config.io_profile {
        IoProfile::HddSafe => 2,
        IoProfile::SsdFast => ssd_max,
        IoProfile::Auto => {
            let all_ssd = !config.roots.is_empty()
                && config
                    .roots
                    .iter()
                    .all(|r| crate::platform::volume_info(r).solid_state == Some(true));
            if all_ssd { ssd_max } else { 2 }
        }
    }
}


// MAX_GROUP_SIZE intentionally removed: chunking files of the same size into
// sub-groups caused silent cross-chunk duplicate blindspots. All files sharing
// a size now form one group and are processed together.

/// Split hard links out of the hashing set.
///
/// - The same path listed twice (overlapping scan roots) counts once.
/// - Only files with a link count above 1 can be hard links. Grouping every file by inode would
///   lump unrelated files together on filesystems that do not report real inode numbers.
/// - One member of each hard-link set stays in the hashing set, so a separate real copy of that
///   content is still found as a duplicate.
fn partition_hardlinks(entries: Vec<FileEntry>) -> (Vec<FileEntry>, Vec<HardLinkGroup>) {
    let mut seen_paths = std::collections::HashSet::new();
    let mut unique = Vec::new();
    let mut map: HashMap<FileId, Vec<FileEntry>> = HashMap::new();
    for e in entries {
        if !seen_paths.insert(e.path.clone()) {
            continue;
        }
        if e.nlink > 1 {
            map.entry(e.file_id).or_default().push(e);
        } else {
            unique.push(e);
        }
    }
    let mut hard = Vec::new();
    for (_, mut g) in map {
        g.sort_by(|a, b| a.path.cmp(&b.path));
        if g.len() == 1 {
            unique.push(g.pop().expect("one"));
        } else {
            unique.push(g[0].clone());
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
/// - `ctime_ns` is read from `FileEntry` (pre-computed at scan time) — no extra `stat()`.
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
            // ctime_ns pre-computed at scan time — no stat() syscall here.
            if let Some(cm) = cache_shared {
                if let Ok(g) = cm.read() {
                    if let Some(ent) = g.get(&e.path, e.size, e.ctime_ns) {
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
                if c.get(&e.path, e.size, e.ctime_ns).is_none() {
                    c.insert_partial(&e.path, e.size, e.ctime_ns, *hash);
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
                    if let Some(ent) = g.get(&e.path, e.size, e.ctime_ns) {
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
                c.set_full(&e.path, e.size, e.ctime_ns, *hash);
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
    run_dedup_inner(entries, config, token, progress, cache, None)
}

/// Same result as `run_dedup`, and also sends each size group's duplicate groups on
/// `live` as soon as that group is finished (P-6). The final result is identical; the
/// stream only lets the UI show groups before the scan ends.
pub fn run_dedup_streaming(
    entries: Vec<FileEntry>,
    config: &ScanConfig,
    token: &CancellationToken,
    progress: Arc<Mutex<ScanProgress>>,
    cache: Option<HashCache>,
    live: std::sync::mpsc::Sender<Vec<DuplicateGroup>>,
) -> ScanResult {
    run_dedup_inner(entries, config, token, progress, cache, Some(live))
}

fn run_dedup_inner(
    entries: Vec<FileEntry>,
    config: &ScanConfig,
    token: &CancellationToken,
    progress: Arc<Mutex<ScanProgress>>,
    cache: Option<HashCache>,
    live: Option<std::sync::mpsc::Sender<Vec<DuplicateGroup>>>,
) -> ScanResult {
    let cache_shared: Option<Arc<RwLock<HashCache>>> =
        if config.use_hash_cache { cache.map(|c| Arc::new(RwLock::new(c))) } else { None };

    let total_input = entries.len() as u64;
    let mut error_log: Vec<(std::path::PathBuf, String)> = Vec::new();

    let (unique_entries, hard_link_groups) = partition_hardlinks(entries);
    let hard_link_group_count = hard_link_groups.len() as u64;
    let to_hash = apply_heuristics(unique_entries, config);
    // The media pass needs every hashed entry by path (exact-group members may be any type).
    let media_entries: Option<HashMap<PathBuf, FileEntry>> = config
        .match_media_payload
        .then(|| to_hash.iter().map(|e| (e.path.clone(), e.clone())).collect());
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
        .num_threads(hash_thread_count(config))
        .thread_name(|i| format!("sfm-hash-{i}"))
        .build()
        .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build().unwrap());

    // All size-groups processed in parallel inside the bounded pool.
    let outcomes: Vec<GroupOutcome> = pool.install(|| {
        size_groups
            .into_par_iter()
            .filter(|_| !token.is_cancelled())
            .map(|group| {
                let outcome = process_size_group(group, config, token, &cache_shared, &progress);
                if let Some(tx) = &live {
                    if !outcome.dup_groups.is_empty() {
                        // A closed receiver means the UI stopped listening. The scan still
                        // finishes and returns its full result, so the send error is ignored.
                        let _ = tx.send(outcome.dup_groups.clone());
                    }
                }
                outcome
            })
            .collect()
    });

    let mut duplicate_groups: Vec<DuplicateGroup> = Vec::new();
    for outcome in outcomes {
        duplicate_groups.extend(outcome.dup_groups);
        error_log.extend(outcome.errors);
    }
    if let Some(entries) = media_entries {
        if !token.is_cancelled() {
            {
                let mut p = progress.lock().unwrap();
                p.phase = ProgressPhase::Processing {
                    stage_label: "Matching photos and videos with different metadata".into(),
                    stage: 2,
                    stages_total: 2,
                };
            }
            duplicate_groups = pool.install(|| {
                merge_media_payload_groups(duplicate_groups, &entries, config, token, &mut error_log)
            });
        }
    }
    let groups_found = duplicate_groups.len() as u64;

    {
        let mut p = progress.lock().unwrap();
        p.phase = ProgressPhase::Finishing;
    }

    if let Some(cm) = &cache_shared {
        // A failed save only costs the next scan some re-hashing, so report it and continue.
        if let Err(e) = cm.write().unwrap().save() {
            eprintln!("hash cache save failed: {e}");
        }
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

/// Marker in `DuplicateGroup::hash` for groups joined by media data rather than whole-file bytes.
pub const MEDIA_GROUP_PREFIX: &str = "media-";

/// Join photos/videos whose media data is identical but whose metadata differs.
///
/// Exact groups that share media data with other files (or other exact groups) are merged into one
/// group, so every file sits in exactly one group. Only files whose media byte ranges hash equal are
/// joined; see `crate::media_hash`.
fn merge_media_payload_groups(
    groups: Vec<DuplicateGroup>,
    entries: &HashMap<PathBuf, FileEntry>,
    config: &ScanConfig,
    token: &CancellationToken,
    errors: &mut Vec<(PathBuf, String)>,
) -> Vec<DuplicateGroup> {
    use crate::media_hash::{format_for, hash as media_hash, locate, Payload};

    // 1. Cheap header parse of every candidate.
    let candidates: Vec<&FileEntry> = entries.values().filter(|e| format_for(&e.path).is_some()).collect();
    let located: Vec<(&FileEntry, Payload)> = candidates
        .par_iter()
        .filter(|_| !token.is_cancelled())
        .filter_map(|e| locate(&e.path).ok().flatten().map(|p| (*e, p)))
        .collect();

    // 2. Only payloads whose size is shared by another file are worth hashing.
    let mut by_len: HashMap<(crate::media_hash::MediaFormat, u64), Vec<(&FileEntry, Payload)>> = HashMap::new();
    for (e, p) in located {
        by_len.entry((p.format, p.len())).or_default().push((e, p));
    }
    let to_hash: Vec<(&FileEntry, Payload)> = by_len.into_values().filter(|v| v.len() > 1).flatten().collect();
    let hashed: Vec<(&FileEntry, Result<[u8; 32], String>)> = to_hash
        .par_iter()
        .filter(|_| !token.is_cancelled())
        .map(|(e, p)| (*e, media_hash(&e.path, p).map_err(|err| err.to_string())))
        .collect();
    let mut by_hash: HashMap<[u8; 32], Vec<&FileEntry>> = HashMap::new();
    for (e, r) in hashed {
        match r {
            Ok(h) => by_hash.entry(h).or_default().push(e),
            Err(msg) => errors.push((e.path.clone(), format!("media match: {msg}"))),
        }
    }
    if token.is_cancelled() {
        return groups;
    }

    // 3. Union exact groups and single files that share media data.
    let mut node_of: HashMap<PathBuf, usize> = HashMap::new();
    for (gi, g) in groups.iter().enumerate() {
        for f in &g.files {
            node_of.insert(f.path.clone(), gi);
        }
    }
    let mut parent: Vec<usize> = (0..groups.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let mut media_hash_of_root: HashMap<usize, [u8; 32]> = HashMap::new();
    let mut joined: HashSet<usize> = HashSet::new();
    for (h, files) in by_hash {
        if files.len() < 2 {
            continue;
        }
        let mut nodes: Vec<usize> = Vec::new();
        for e in files {
            let n = *node_of.entry(e.path.clone()).or_insert_with(|| {
                parent.push(parent.len());
                parent.len() - 1
            });
            nodes.push(n);
        }
        let first = nodes[0];
        for &n in &nodes[1..] {
            let (a, b) = (find(&mut parent, first), find(&mut parent, n));
            if a != b {
                parent[b] = a;
                joined.insert(a);
            }
        }
        let root = find(&mut parent, first);
        media_hash_of_root.entry(root).or_insert(h);
    }
    if joined.is_empty() {
        return groups;
    }

    // 4. Rebuild: components that joined two or more nodes become one media group.
    let mut members: HashMap<usize, Vec<PathBuf>> = HashMap::new();
    for (path, n) in &node_of {
        let root = find(&mut parent, *n);
        members.entry(root).or_default().push(path.clone());
    }
    let mut out = Vec::with_capacity(groups.len());
    let mut emitted: HashSet<usize> = HashSet::new();
    for (gi, g) in groups.into_iter().enumerate() {
        let root = find(&mut parent, gi);
        let multi = members.get(&root).is_some_and(|m| {
            let nodes: HashSet<usize> = m.iter().map(|p| node_of[p]).collect();
            nodes.len() > 1
        });
        if !multi {
            out.push(g);
            continue;
        }
        if emitted.insert(root) {
            if let Some(dg) = build_media_group(root, &members, entries, &media_hash_of_root, config) {
                out.push(dg);
            } else {
                out.push(g); // could not rebuild safely: keep the exact group as it was
            }
        }
    }
    // Components made only of single files (no exact group involved).
    let roots: Vec<usize> = members.keys().copied().collect();
    for root in roots {
        if emitted.contains(&root) {
            continue;
        }
        let nodes: HashSet<usize> = members[&root].iter().map(|p| node_of[p]).collect();
        if nodes.len() > 1 {
            emitted.insert(root);
            if let Some(dg) = build_media_group(root, &members, entries, &media_hash_of_root, config) {
                out.push(dg);
            }
        }
    }
    out
}

fn build_media_group(
    root: usize,
    members: &HashMap<usize, Vec<PathBuf>>,
    entries: &HashMap<PathBuf, FileEntry>,
    media_hash_of_root: &HashMap<usize, [u8; 32]>,
    config: &ScanConfig,
) -> Option<DuplicateGroup> {
    let mut files: Vec<FileEntry> = Vec::new();
    for p in members.get(&root)? {
        files.push(entries.get(p)?.clone());
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let hash = media_hash_of_root.get(&root).copied().unwrap_or([0u8; 32]);
    let mut dg = build_duplicate_group(hash, files, KeepPolicy::SmartScore, config);
    if dg.files.len() < 2 {
        return None;
    }
    dg.hash = format!("{MEDIA_GROUP_PREFIX}{}", dg.hash);
    dg.keep_reason = if dg.keep_reason.is_empty() {
        "same media, different metadata".to_string()
    } else {
        format!("{} · same media, different metadata", dg.keep_reason)
    };
    Some(dg)
}
