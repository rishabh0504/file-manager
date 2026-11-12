//! Pure planning for bulk actions (Implementation-Plan Phase 1, findings U1–U3).
//!
//! Decides which files an action would touch, without doing any I/O or UI work,
//! so the confirm dialog and the executor always agree on the same set.
//!
//! Invariant: a plan never contains a group's keep file (`DuplicateGroup::keep_index`).

use crate::types::{ActionMode, DuplicateGroup, FileEntrySnapshot, ScanResult};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Number of example paths shown in the confirm dialog.
pub const SAMPLE_SIZE: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionPlan {
    /// Every path the action will touch, in group order.
    pub paths: Vec<PathBuf>,
    pub total_bytes: u64,
    /// Number of distinct duplicate groups touched.
    pub group_count: usize,
    /// First `SAMPLE_SIZE` paths, for display.
    pub sample: Vec<PathBuf>,
}

impl ActionPlan {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Build a plan from `(group_index, file)` pairs. Keep files must already be filtered out.
    fn build<'a>(items: impl IntoIterator<Item = (usize, &'a FileEntrySnapshot)>) -> Self {
        let mut plan = ActionPlan::default();
        let mut groups = HashSet::new();
        for (gi, f) in items {
            groups.insert(gi);
            plan.total_bytes += f.size;
            if plan.sample.len() < SAMPLE_SIZE {
                plan.sample.push(f.path.clone());
            }
            plan.paths.push(f.path.clone());
        }
        plan.group_count = groups.len();
        plan
    }
}

/// Plan for trashing items chosen in the Explorer: `(path, size)` pairs, deduplicated, in order.
/// No duplicate-group rules apply, so `group_count` stays 0.
pub fn for_paths(items: &[(PathBuf, u64)]) -> ActionPlan {
    let mut plan = ActionPlan::default();
    let mut seen = HashSet::new();
    for (path, size) in items {
        if !seen.insert(path.clone()) {
            continue;
        }
        plan.total_bytes += size;
        if plan.sample.len() < SAMPLE_SIZE {
            plan.sample.push(path.clone());
        }
        plan.paths.push(path.clone());
    }
    plan
}

/// Plan for trashing empty folders: the listed folders that are also checked, in listed order.
/// Folders carry no size and no duplicate-group rules, so `group_count` stays 0.
pub fn for_empty_folders(listed: &[PathBuf], selected: &HashSet<PathBuf>) -> ActionPlan {
    let items: Vec<(PathBuf, u64)> = listed
        .iter()
        .filter(|p| selected.contains(*p))
        .map(|p| (p.clone(), 0))
        .collect();
    for_paths(&items)
}

/// Non-keep files of one group, tagged with the group index.
fn non_keep(gi: usize, g: &DuplicateGroup) -> impl Iterator<Item = (usize, &FileEntrySnapshot)> {
    g.files
        .iter()
        .enumerate()
        .filter(move |(i, _)| *i != g.keep_index)
        .map(move |(_, f)| (gi, f))
}

/// Bulk action over every group (Apply actions). `DryRun` plans nothing.
pub fn for_apply(result: &ScanResult, mode: ActionMode) -> ActionPlan {
    match mode {
        ActionMode::DryRun => ActionPlan::default(),
        ActionMode::Delete | ActionMode::HardLink => ActionPlan::build(
            result
                .duplicate_groups
                .iter()
                .enumerate()
                .flat_map(|(gi, g)| non_keep(gi, g)),
        ),
    }
}

/// Selected copies inside one group (checkbox selection). The keep file is skipped even if selected.
pub fn for_selected(gi: usize, g: &DuplicateGroup, selected: &HashSet<PathBuf>) -> ActionPlan {
    ActionPlan::build(non_keep(gi, g).filter(|(_, f)| selected.contains(&f.path)))
}

/// Every non-keep copy in one group ("Delete duplicates in group").
pub fn for_group(gi: usize, g: &DuplicateGroup) -> ActionPlan {
    ActionPlan::build(non_keep(gi, g))
}

/// Non-keep copies of one group that live directly in `folder` ("Delete all here").
pub fn for_folder(gi: usize, g: &DuplicateGroup, folder: &Path) -> ActionPlan {
    ActionPlan::build(
        non_keep(gi, g).filter(|(_, f)| f.path.parent() == Some(folder)),
    )
}

/// "Clean same-folder duplicates": inside each group and folder, keep one copy
/// (the keep file if it is there, otherwise the shortest name) and plan the rest.
pub fn for_same_folder(result: &ScanResult) -> ActionPlan {
    let mut items: Vec<(usize, &FileEntrySnapshot)> = Vec::new();
    for (gi, g) in result.duplicate_groups.iter().enumerate() {
        let mut by_folder: std::collections::HashMap<&Path, Vec<(usize, &FileEntrySnapshot)>> =
            std::collections::HashMap::new();
        for (i, f) in g.files.iter().enumerate() {
            if let Some(parent) = f.path.parent() {
                by_folder.entry(parent).or_default().push((i, f));
            }
        }
        for (_, mut files) in by_folder {
            if files.len() < 2 {
                continue;
            }
            files.sort_by_key(|(i, f)| {
                let keep_rank = if *i == g.keep_index { 0 } else { 1 };
                let name_len = f.path.file_name().map(|n| n.len()).unwrap_or(usize::MAX);
                (keep_rank, name_len)
            });
            items.extend(files.into_iter().skip(1).map(|(_, f)| (gi, f)));
        }
    }
    ActionPlan::build(items)
}
