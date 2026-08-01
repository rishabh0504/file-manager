use crate::cache::{entry_mtime_secs, HashCache};
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
use crate::verifier::verify_group;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

const MAX_GROUP_SIZE: usize = 10_000;

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
        for chunk in g.chunks(MAX_GROUP_SIZE) {
            out.push(chunk.to_vec());
        }
    }
    (out, bytes_in_hashing)
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
            stage_label: "Partial hash".into(),
            stage: 1,
            stages_total: 4,
        };
    }

    let mut duplicate_groups: Vec<DuplicateGroup> = Vec::new();
    let mut groups_found: u64 = 0;

    for group in size_groups {
        if token.is_cancelled() {
            break;
        }

        let partial_results: Vec<(FileEntry, Result<[u8; 32], FileError>)> = group
            .par_iter()
            .map(|e| {
                if token.is_cancelled() {
                    return (e.clone(), Err(FileError::Cancelled));
                }
                let mtime = entry_mtime_secs(&e.path).unwrap_or(0);
                if let Some(cm) = &cache_shared {
                    if let Ok(g) = cm.read() {
                        if let Some(ent) = g.get(&e.path, e.size, mtime) {
                            return (e.clone(), Ok(ent.partial_hash));
                        }
                    }
                }
                let chunk = compute_chunk_size(e.size);
                let h = partial_hash_from_reads(&e.path, e.size, chunk, token);
                (e.clone(), h)
            })
            .collect();

        if let Some(cm) = &cache_shared {
            let mut c = cm.write().unwrap();
            for (e, r) in &partial_results {
                if let Ok(hash) = r {
                    let mtime = entry_mtime_secs(&e.path).unwrap_or(0);
                    if c.get(&e.path, e.size, mtime).is_none() {
                        c.insert_partial(&e.path, e.size, mtime, *hash);
                    }
                }
            }
        }

        for (e, r) in &partial_results {
            if let Err(err) = r {
                log_error(&mut error_log, e.path.clone(), err.clone());
            }
        }

        let mut by_partial: HashMap<[u8; 32], Vec<FileEntry>> = HashMap::new();
        for (e, r) in partial_results {
            if let Ok(h) = r {
                by_partial.entry(h).or_default().push(e);
            }
        }

        let mut full_candidates: Vec<FileEntry> = Vec::new();
        for (_ph, mut files) in by_partial {
            if files.len() <= 1 {
                continue;
            }
            full_candidates.append(&mut files);
        }

        if token.is_cancelled() {
            break;
        }

        {
            let mut p = progress.lock().unwrap();
            p.phase = ProgressPhase::Processing {
                stage_label: "Full hash".into(),
                stage: 2,
                stages_total: 4,
            };
        }

        let full_results: Vec<(FileEntry, Result<[u8; 32], FileError>)> = full_candidates
            .par_iter()
            .map(|e| {
                if token.is_cancelled() {
                    return (e.clone(), Err(FileError::Cancelled));
                }
                let mtime = entry_mtime_secs(&e.path).unwrap_or(0);
                if let Some(cm) = &cache_shared {
                    if let Ok(g) = cm.read() {
                        if let Some(ent) = g.get(&e.path, e.size, mtime) {
                            if let Some(fh) = ent.full_hash {
                                return (e.clone(), Ok(fh));
                            }
                        }
                    }
                }
                (e.clone(), full_hash(&e.path, e.size, token))
            })
            .collect();

        if let Some(cm) = &cache_shared {
            let mut c = cm.write().unwrap();
            for (e, r) in &full_results {
                if let Ok(hash) = r {
                    let mtime = entry_mtime_secs(&e.path).unwrap_or(0);
                    c.set_full(&e.path, e.size, mtime, *hash);
                }
            }
        }

        for (e, r) in &full_results {
            match r {
                Ok(_) => {
                    let mut p = progress.lock().unwrap();
                    p.bytes_processed = p.bytes_processed.saturating_add(e.size);
                    p.files_hashed = p.files_hashed.saturating_add(1);
                    p.current_path = e.path.clone();
                }
                Err(err) => {
                    log_error(&mut error_log, e.path.clone(), err.clone());
                }
            }
        }

        let mut by_full: HashMap<[u8; 32], Vec<FileEntry>> = HashMap::new();
        for (e, r) in full_results {
            if let Ok(h) = r {
                by_full.entry(h).or_default().push(e);
            }
        }

        {
            let mut p = progress.lock().unwrap();
            p.phase = ProgressPhase::Processing {
                stage_label: "Verify & resolve".into(),
                stage: 3,
                stages_total: 4,
            };
        }

        for (full_hash, files) in by_full {
            if files.len() < 2 {
                continue;
            }
            if token.is_cancelled() {
                break;
            }
            let verified = verify_group(&files);
            if verified.len() < 2 {
                continue;
            }
            let dg = build_duplicate_group(full_hash, verified, KeepPolicy::SmartScore, config);
            groups_found += 1;
            duplicate_groups.push(dg);
            let mut p = progress.lock().unwrap();
            p.groups_found = groups_found;
        }
    }

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

    let mut merged = refresh_errors;
    merged.append(&mut new.error_log);
    const MAX_ERR: usize = 1000;
    if merged.len() > MAX_ERR {
        merged.truncate(MAX_ERR);
    }

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
