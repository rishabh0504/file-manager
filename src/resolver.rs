use crate::errors::{FileError, FileResult};
use crate::types::{
    ActionMode, CrossFolderConflict, DuplicateGroup, FileEntry, FileEntrySnapshot, KeepPolicy,
    ScanConfig,
};
use crate::platform::file_id;
use std::path::{Path, PathBuf};

const TMP_EXT: &str = "sfm_tmp";

pub fn score_entry(entry: &FileEntry, config: &ScanConfig) -> i64 {
    let mut score: i64 = 0;

    let depth = entry.path.components().count() as i64;
    score -= depth * 10;

    score += (entry.atime_secs as i64) / 1_000_000;

    for preferred in &config.preferred_dirs {
        if entry.path.starts_with(preferred) {
            score += 1000;
        }
    }

    for (rank, folder) in config.root_order.iter().enumerate() {
        if entry.path.starts_with(folder) {
            score += (config.root_order.len().saturating_sub(rank)) as i64 * 50;
            break;
        }
    }

    let lossy = entry.path.to_string_lossy();
    for junk in ["/Downloads", "/Trash", "/.Trash", "/tmp", "/Temp", "\\Downloads"] {
        if lossy.contains(junk) {
            score -= 500;
        }
    }

    score
}

/// Explain the positive score terms that made `entry` the keep file (U10).
/// Mirrors `score_entry`: preferred folders, root order, and path depth.
pub fn keep_reason(entry: &FileEntry, config: &ScanConfig) -> String {
    let mut reasons: Vec<String> = Vec::new();
    if config.preferred_dirs.iter().any(|p| entry.path.starts_with(p)) {
        reasons.push("in a preferred folder".into());
    }
    if let Some(rank) = config.root_order.iter().position(|f| entry.path.starts_with(f)) {
        if rank == 0 {
            reasons.push("in your first-ranked folder".into());
        } else {
            reasons.push(format!("in folder #{} of your order", rank + 1));
        }
    }
    if reasons.is_empty() {
        reasons.push("shallowest path and most recently used".into());
    }
    reasons.join(", ")
}

pub fn pick_keep_index(files: &[FileEntry], policy: KeepPolicy, config: &ScanConfig) -> usize {
    match policy {
        KeepPolicy::Manual => 0,
        KeepPolicy::ShortestPath => files
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| e.path.as_os_str().len())
            .map(|(i, _)| i)
            .unwrap_or(0),
        KeepPolicy::Oldest => files
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.mtime.cmp(&b.mtime))
            .map(|(i, _)| i)
            .unwrap_or(0),
        KeepPolicy::SmartScore => files
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                let sa = score_entry(a, config);
                let sb = score_entry(b, config);
                sa.cmp(&sb)
            })
            .map(|(i, _)| i)
            .unwrap_or(0),
    }
}

fn hash_hex(h: &[u8; 32]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn build_duplicate_group(
    hash: [u8; 32],
    files: Vec<FileEntry>,
    keep_policy: KeepPolicy,
    config: &ScanConfig,
) -> DuplicateGroup {
    // In asymmetric mode, roots 0..split_idx are Source (A), and split_idx..N are Destination (B).
    // If enabled, we pick exactly one file from A as the Reference (keep), and all files from B as duplicates (trash).
    // Any other files from A are omitted from the duplicate group so they aren't shown or deleted.
    let (files_to_process, forced_keep_index) = if let Some(split_idx) = config.asymmetric_split_index {
        let (mut a_files, b_files): (Vec<FileEntry>, Vec<FileEntry>) = files
            .into_iter()
            .partition(|f| f.root_index < split_idx);

        if !a_files.is_empty() && !b_files.is_empty() {
            // Pick best file in A as reference
            let best_a_idx = pick_keep_index(&a_files, keep_policy, config);
            let ref_a = a_files.swap_remove(best_a_idx);
            
            let mut list = Vec::with_capacity(1 + b_files.len());
            list.push(ref_a); // Index 0 is always the Reference from A
            list.extend(b_files);
            (list, Some(0))
        } else {
            // If it doesn't span both A and B, we shouldn't get here because of pipeline filtering,
            // but fallback safely to empty or original list.
            (Vec::new(), None)
        }
    } else {
        (files, None)
    };

    if files_to_process.is_empty() {
        return DuplicateGroup {
            keep_reason: String::new(),
            hash: hash_hex(&hash),
            files: Vec::new(),
            total_wasted_bytes: 0,
            keep_index: 0,
        };
    }

    let keep_index = forced_keep_index.unwrap_or_else(|| pick_keep_index(&files_to_process, keep_policy, config));
    let total_wasted_bytes = files_to_process.iter().map(|f| f.size).sum::<u64>()
        - files_to_process.get(keep_index).map(|f| f.size).unwrap_or(0);

    let snapshots: Vec<FileEntrySnapshot> = files_to_process
        .iter()
        .enumerate()
        .map(|(i, e)| FileEntrySnapshot {
            path: e.path.clone(),
            size: e.size,
            score: score_entry(e, config),
            is_keep: i == keep_index,
            root_index: e.root_index,
        })
        .collect();

    let keep_reason = keep_reason(&files_to_process[keep_index], config);
    DuplicateGroup {
        hash: hash_hex(&hash),
        files: snapshots,
        total_wasted_bytes,
        keep_index,
        keep_reason,
    }
}

pub fn detect_cross_folder_conflicts(groups: &[DuplicateGroup]) -> Vec<CrossFolderConflict> {
    let mut out = Vec::new();
    for g in groups {
        let root_idxs: std::collections::HashSet<_> =
            g.files.iter().map(|f| f.root_index).collect();
        if root_idxs.len() <= 1 {
            continue;
        }
        let keep_index = g.keep_index;
        out.push(CrossFolderConflict {
            hash_hex: g.hash.clone(),
            paths: g.files.iter().map(|f| f.path.clone()).collect(),
            keep_index,
        });
    }
    out
}

pub fn atomic_hardlink(keep_path: &Path, target_path: &Path) -> FileResult<()> {
    let temp_path = target_path.with_extension(format!(
        "{}_{}",
        TMP_EXT,
        std::process::id()
    ));
    std::fs::hard_link(keep_path, &temp_path).map_err(|e| FileError::Io(e.to_string()))?;
    match std::fs::rename(&temp_path, target_path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temp_path);
            Err(FileError::Io(e.to_string()))
        }
    }
}

pub fn apply_hardlink_group(group: &DuplicateGroup, mode: ActionMode) -> Vec<FileResult<()>> {
    if mode != ActionMode::HardLink {
        return Vec::new();
    }
    let Some(keep_snap) = group.files.get(group.keep_index) else {
        return Vec::new();
    };
    let keep_path = &keep_snap.path;
    let keep_id = file_id(keep_path).ok();

    group
        .files
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != group.keep_index)
        .map(|(_, snap)| {
            let tid = file_id(&snap.path).ok();
            if let (Some(k), Some(t)) = (keep_id, tid) {
                if k.volume_id != t.volume_id {
                    return Err(FileError::CrossVolumeHardLink);
                }
            }
            atomic_hardlink(keep_path, &snap.path)
        })
        .collect()
}

/// Move a single file to the system trash (Recycle Bin / Trash).
pub fn trash_single_path(path: &Path) -> FileResult<()> {
    trash::delete(path).map_err(|e| FileError::Io(e.to_string()))
}

/// Move multiple files to the system trash in a batch operation.
/// If `trash::delete_all` encounters an issue, automatically falls back to per-file `trash::delete`
/// so that one problematic file does not prevent the rest from being moved to trash.
pub fn trash_batch<P: AsRef<Path>, I: IntoIterator<Item = P>>(paths: I) -> Result<Vec<PathBuf>, String> {
    let path_list: Vec<PathBuf> = paths.into_iter().map(|p| p.as_ref().to_path_buf()).collect();
    if path_list.is_empty() {
        return Ok(Vec::new());
    }

    // Attempt native batch delete first
    if trash::delete_all(&path_list).is_ok() {
        return Ok(path_list);
    }

    // Fallback: per-file deletion so valid files still succeed
    use rayon::prelude::*;
    let (successfully_trashed, errors): (Vec<_>, Vec<_>) = path_list
        .into_par_iter()
        .map(|p| {
            // A failed batch may have trashed some files already: gone means done.
            if p.symlink_metadata().is_err() {
                return Ok(p);
            }
            match trash::delete(&p) {
                Ok(()) => Ok(p),
                Err(err) => Err(format!("{}: {}", p.display(), err)),
            }
        })
        .partition(Result::is_ok);

    let successfully_trashed: Vec<PathBuf> = successfully_trashed.into_iter().map(Result::unwrap).collect();
    let errors: Vec<String> = errors.into_iter().map(Result::unwrap_err).collect();

    if successfully_trashed.is_empty() && !errors.is_empty() {
        Err(errors.join("; "))
    } else {
        Ok(successfully_trashed)
    }
}

pub fn apply_delete_group(group: &DuplicateGroup, mode: ActionMode) -> Vec<FileResult<()>> {
    if mode != ActionMode::Delete {
        return Vec::new();
    }
    let paths: Vec<&Path> = group
        .files
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != group.keep_index)
        .map(|(_, snap)| snap.path.as_path())
        .collect();

    if paths.is_empty() {
        return Vec::new();
    }

    match trash_batch(&paths) {
        Ok(trashed) => trashed.iter().map(|_| Ok(())).collect(),
        Err(e) => vec![Err(FileError::Io(e))],
    }
}


