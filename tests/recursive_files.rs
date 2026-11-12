use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use sortwise::recursive_files::{is_text_file, list_files, sort_by_size_desc, DEFAULT_CAP};

/// Unique temp tree, removed on drop.
struct TempTree(PathBuf);

impl TempTree {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("sortwise_rf_{tag}_{nanos}"));
        fs::create_dir_all(&root).unwrap();
        TempTree(root)
    }

    fn write(&self, rel: &str, bytes: usize) {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![b'x'; bytes]).unwrap();
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn rels(files: &[sortwise::recursive_files::RecursiveFile]) -> Vec<String> {
    let mut v: Vec<String> = files.iter().map(|f| f.rel.replace('\\', "/")).collect();
    v.sort();
    v
}

#[test]
fn lists_nested_files_and_excludes_text_hidden_and_system() {
    let t = TempTree::new("filter");
    t.write("a/movie.mp4", 100);
    t.write("a/b/deep.zip", 50);
    t.write("a/b/photo.jpg", 10);
    t.write("notes.txt", 5); // text: excluded
    t.write("code/main.rs", 5); // source: excluded
    t.write("report.md", 5); // text: excluded
    t.write(".hidden.mp4", 5); // dot-file: excluded
    t.write(".config/x.bin", 5); // inside hidden dir: excluded
    t.write("Thumbs.db", 5); // system file: excluded
    t.write("desktop.ini", 5); // system file: excluded
    t.write("node_modules/pkg/index.bin", 5); // system dir: excluded when skipping
    t.write("keep/archive.7z", 7);

    let cancel = AtomicBool::new(false);
    let listing = list_files(t.path(), true, DEFAULT_CAP, &cancel);

    assert_eq!(
        rels(&listing.files),
        vec!["a/b/deep.zip", "a/b/photo.jpg", "a/movie.mp4", "keep/archive.7z"]
    );
    assert!(!listing.truncated);
    assert!(!listing.cancelled);
}

#[test]
fn system_dirs_kept_when_skip_disabled() {
    let t = TempTree::new("nosys");
    t.write("node_modules/pkg/lib.zip", 5);
    t.write("visible.zip", 5);

    let cancel = AtomicBool::new(false);
    let listing = list_files(t.path(), false, DEFAULT_CAP, &cancel);
    assert_eq!(rels(&listing.files), vec!["node_modules/pkg/lib.zip", "visible.zip"]);
}

#[test]
fn sorts_by_size_descending_with_stable_tiebreak() {
    let t = TempTree::new("sort");
    t.write("small.zip", 1);
    t.write("big.mp4", 300);
    t.write("b_mid.zip", 50);
    t.write("a_mid.zip", 50);

    let cancel = AtomicBool::new(false);
    let mut listing = list_files(t.path(), true, DEFAULT_CAP, &cancel);
    sort_by_size_desc(&mut listing.files);
    let order: Vec<&str> = listing.files.iter().map(|f| f.rel.as_str()).collect();
    assert_eq!(order, vec!["big.mp4", "a_mid.zip", "b_mid.zip", "small.zip"]);
}

#[test]
fn cap_sets_truncated() {
    let t = TempTree::new("cap");
    for i in 0..5 {
        t.write(&format!("f{i}.zip"), 1);
    }
    let cancel = AtomicBool::new(false);
    let listing = list_files(t.path(), true, 3, &cancel);
    assert_eq!(listing.files.len(), 3);
    assert!(listing.truncated);
}

#[test]
fn cancel_stops_walk() {
    let t = TempTree::new("cancel");
    t.write("a.zip", 1);
    let cancel = AtomicBool::new(true);
    let listing = list_files(t.path(), true, DEFAULT_CAP, &cancel);
    assert!(listing.cancelled);
    assert!(listing.files.is_empty());
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    let t = TempTree::new("symlink");
    t.write("real/a.zip", 4);
    std::os::unix::fs::symlink(t.path(), t.path().join("real/loop")).unwrap();
    let cancel = AtomicBool::new(false);
    let listing = list_files(t.path(), true, DEFAULT_CAP, &cancel);
    assert_eq!(rels(&listing.files), vec!["real/a.zip"]);
}

#[test]
fn text_detection_is_case_insensitive() {
    assert!(is_text_file(Path::new("X.TXT")));
    assert!(is_text_file(Path::new("data.Json")));
    assert!(!is_text_file(Path::new("clip.MP4")));
    assert!(!is_text_file(Path::new("noext")));
}
