//! Safety tests for bulk-action planning (Implementation-Plan Phase 1).
//! These check which files an action would touch. They never call the OS trash.

use sortwise::action_log::{ActionLogEntry, write_to};
use sortwise::action_plan::{prune_groups, for_apply, for_empty_folders, for_folder, for_group, for_paths, for_same_folder, for_selected, SAMPLE_SIZE};
use sortwise::types::{ActionMode, DuplicateGroup, FileEntrySnapshot, ScanResult, ScanStats};
use std::collections::HashSet;
use std::path::PathBuf;

fn file(path: &str, size: u64, root: usize) -> FileEntrySnapshot {
    FileEntrySnapshot {
        path: PathBuf::from(path),
        size,
        score: 0,
        is_keep: false,
        root_index: root,
    }
}

fn group(files: Vec<FileEntrySnapshot>, keep_index: usize) -> DuplicateGroup {
    let size = files.first().map(|f| f.size).unwrap_or(0);
    DuplicateGroup {
        keep_reason: String::new(),
        hash: format!("h{}", files.len()),
        total_wasted_bytes: size * (files.len() as u64 - 1),
        files,
        keep_index,
    }
}

fn result(groups: Vec<DuplicateGroup>) -> ScanResult {
    ScanResult {
        stats: ScanStats {
            files_scanned: 0,
            files_skipped_error: 0,
            duplicate_groups: groups.len() as u64,
            total_wasted_bytes: 0,
            partial: false,
            hard_link_groups: 0,
        },
        duplicate_groups: groups,
        hard_link_groups: vec![],
        error_log: vec![],
        folder_summaries: vec![],
        cross_folder_conflicts: vec![],
    }
}

/// Group 0: keep is index 1 (`/a/x.jpg`). Group 1: keep is index 0.
fn sample_result() -> ScanResult {
    result(vec![
        group(
            vec![
                file("/a/x.jpg", 100, 0),
                file("/a/keep.jpg", 100, 0),
                file("/b/x.jpg", 100, 1),
            ],
            1,
        ),
        group(vec![file("/c/y.mp4", 500, 0), file("/d/y.mp4", 500, 1)], 0),
    ])
}

#[test]
fn apply_delete_never_includes_keep_files() {
    let r = sample_result();
    let plan = for_apply(&r, ActionMode::Delete);
    assert_eq!(plan.paths.len(), 3);
    assert!(!plan.paths.contains(&PathBuf::from("/a/keep.jpg")));
    assert!(!plan.paths.contains(&PathBuf::from("/c/y.mp4")));
    assert_eq!(plan.group_count, 2);
    assert_eq!(plan.total_bytes, 100 + 100 + 500);
}

#[test]
fn apply_dry_run_plans_nothing() {
    let plan = for_apply(&sample_result(), ActionMode::DryRun);
    assert!(plan.is_empty());
    assert_eq!(plan.total_bytes, 0);
}

#[test]
fn apply_hardlink_uses_same_set_as_delete() {
    let r = sample_result();
    assert_eq!(for_apply(&r, ActionMode::HardLink), for_apply(&r, ActionMode::Delete));
}

#[test]
fn selected_keep_file_is_skipped_even_when_checked() {
    let r = sample_result();
    let g = &r.duplicate_groups[0];
    let selected: HashSet<PathBuf> = g.files.iter().map(|f| f.path.clone()).collect();
    let plan = for_selected(0, g, &selected);
    assert_eq!(plan.paths, vec![PathBuf::from("/a/x.jpg"), PathBuf::from("/b/x.jpg")]);
}

#[test]
fn group_delete_always_leaves_keep_file() {
    let r = sample_result();
    for (gi, g) in r.duplicate_groups.iter().enumerate() {
        let plan = for_group(gi, g);
        assert!(plan.paths.len() < g.files.len(), "group {gi} would lose every copy");
        assert!(!plan.paths.contains(&g.files[g.keep_index].path));
    }
}

#[test]
fn folder_delete_skips_keep_when_keep_is_in_that_folder() {
    let r = sample_result();
    let g = &r.duplicate_groups[0];
    // Keep lives in /a together with one duplicate.
    let plan = for_folder(0, g, std::path::Path::new("/a"));
    assert_eq!(plan.paths, vec![PathBuf::from("/a/x.jpg")]);
}

#[test]
fn same_folder_clean_keeps_one_copy_per_folder() {
    let r = result(vec![group(
        vec![
            file("/a/one.jpg", 10, 0),
            file("/a/longer-name.jpg", 10, 0),
            file("/b/one.jpg", 10, 1),
        ],
        2,
    )]);
    let plan = for_same_folder(&r);
    // Folder /a has two copies: the shortest name survives, the other is planned.
    assert_eq!(plan.paths, vec![PathBuf::from("/a/longer-name.jpg")]);
}

#[test]
fn same_folder_clean_never_plans_keep_file() {
    let r = result(vec![group(
        vec![
            file("/a/long-keep-name.jpg", 10, 0),
            file("/a/x.jpg", 10, 0),
        ],
        0,
    )]);
    let plan = for_same_folder(&r);
    assert_eq!(plan.paths, vec![PathBuf::from("/a/x.jpg")]);
}

#[test]
fn plan_sample_is_capped() {
    let files: Vec<FileEntrySnapshot> = (0..25).map(|i| file(&format!("/d/f{i}.jpg"), 1, 0)).collect();
    let r = result(vec![group(files, 0)]);
    let plan = for_apply(&r, ActionMode::Delete);
    assert_eq!(plan.paths.len(), 24);
    assert_eq!(plan.sample.len(), sortwise::action_plan::SAMPLE_SIZE);
}

#[test]
fn action_log_reports_not_completed_paths() {
    let planned = vec![PathBuf::from("/a/1"), PathBuf::from("/a/2"), PathBuf::from("/a/3")];
    let completed = vec![PathBuf::from("/a/2")];
    let entry = ActionLogEntry::new("apply_delete", &planned, &completed, None);
    assert_eq!(entry.not_completed, vec![PathBuf::from("/a/1"), PathBuf::from("/a/3")]);

    let dir = std::env::temp_dir().join(format!("sfm_action_log_{}", std::process::id()));
    let path = write_to(&dir, &entry).expect("log written");
    let text = std::fs::read_to_string(&path).expect("log readable");
    assert!(text.contains("apply_delete"));
    assert!(text.contains("not_completed"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn explorer_trash_plan_is_exactly_the_given_paths() {
    let items = vec![
        (PathBuf::from("/a/one.jpg"), 10),
        (PathBuf::from("/a/two.jpg"), 20),
        (PathBuf::from("/a/one.jpg"), 10), // duplicate selection must not be trashed twice
    ];
    let plan = for_paths(&items);
    assert_eq!(plan.paths, vec![PathBuf::from("/a/one.jpg"), PathBuf::from("/a/two.jpg")]);
    assert_eq!(plan.total_bytes, 30);
    assert_eq!(plan.group_count, 0);
    assert_eq!(plan.sample, plan.paths);
}

#[test]
fn explorer_trash_plan_sample_is_capped_and_empty_plan_is_empty() {
    let items: Vec<(PathBuf, u64)> = (0..SAMPLE_SIZE + 5)
        .map(|i| (PathBuf::from(format!("/a/{i}.txt")), 1))
        .collect();
    let plan = for_paths(&items);
    assert_eq!(plan.paths.len(), SAMPLE_SIZE + 5);
    assert_eq!(plan.sample.len(), SAMPLE_SIZE);
    assert!(for_paths(&[]).is_empty());
}

#[test]
fn empty_folder_plan_is_exactly_the_checked_listed_folders() {
    let listed = vec![
        PathBuf::from("/lib/Photos/Trip"),
        PathBuf::from("/lib/Downloads/old"),
        PathBuf::from("/lib/Docs/Drafts"),
    ];
    // "/lib/Gone" is checked but not listed any more; it must not be planned.
    let selected: HashSet<PathBuf> = [
        PathBuf::from("/lib/Docs/Drafts"),
        PathBuf::from("/lib/Photos/Trip"),
        PathBuf::from("/lib/Gone"),
    ]
    .into_iter()
    .collect();
    let plan = for_empty_folders(&listed, &selected);
    assert_eq!(plan.paths, vec![PathBuf::from("/lib/Photos/Trip"), PathBuf::from("/lib/Docs/Drafts")]);
    assert_eq!(plan.group_count, 0);
    assert!(for_empty_folders(&listed, &HashSet::new()).is_empty());
}

#[test]
fn pruning_earlier_copies_keeps_the_same_keep_file() {
    // Keep is the middle file. Trashing the first one shifts indices; the keep must not move.
    let g = group(vec![file("/a/x.mp4", 10, 0), file("/b/x.mp4", 10, 1), file("/c/x.mp4", 10, 2)], 1);
    let removed: HashSet<PathBuf> = [PathBuf::from("/a/x.mp4")].into_iter().collect();
    let out = prune_groups(vec![g], &removed);
    assert_eq!(out.len(), 1);
    let kept = &out[0].files[out[0].keep_index];
    assert_eq!(kept.path, PathBuf::from("/b/x.mp4"));
    assert!(kept.is_keep);
    assert_eq!(out[0].files.iter().filter(|f| f.is_keep).count(), 1);
    assert_eq!(out[0].total_wasted_bytes, 10);
}

#[test]
fn pruning_drops_groups_left_with_one_file() {
    let g = group(vec![file("/a/y.jpg", 5, 0), file("/b/y.jpg", 5, 1)], 0);
    let removed: HashSet<PathBuf> = [PathBuf::from("/b/y.jpg")].into_iter().collect();
    assert!(prune_groups(vec![g], &removed).is_empty());
}
