use crate::errors::{FileError, FileResult};
use crate::types::{
    ActionMode, CrossFolderConflict, DuplicateGroup, FileEntry, FileEntrySnapshot, KeepPolicy,
    ScanConfig,
};
use crate::platform::file_id;
use std::path::Path;

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
    let keep_index = pick_keep_index(&files, keep_policy, config);
    let total_wasted_bytes = files.iter().map(|f| f.size).sum::<u64>()
        - files.get(keep_index).map(|f| f.size).unwrap_or(0);

    let snapshots: Vec<FileEntrySnapshot> = files
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

    DuplicateGroup {
        hash: hash_hex(&hash),
        files: snapshots,
        total_wasted_bytes,
        keep_index,
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

pub fn apply_delete_group(group: &DuplicateGroup, mode: ActionMode) -> Vec<FileResult<()>> {
    if mode != ActionMode::Delete {
        return Vec::new();
    }
    group
        .files
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != group.keep_index)
        .map(|(_, snap)| trash::delete(&snap.path).map_err(|e| FileError::Io(e.to_string())))
        .collect()
}
