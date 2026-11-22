//! File-system helpers: fast move, unique destinations, search collapsing, media walk.
use super::*;

/// Move a file as fast as possible:
/// 1. `rename(2)` — atomic, instant on same filesystem.
/// 2. `cp -c` — APFS copy-on-write clone (macOS), instant regardless of file size.
/// 3. `cp`    — regular copy for non-APFS volumes, then delete source.
pub(crate) fn fast_move(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    // Safety guard: NEVER overwrite an existing destination file, directory, or symlink!
    if dst.symlink_metadata().is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("Destination already exists: {}", dst.display()),
        ));
    }

    // Fastest path: atomic rename (same volume)
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }

    // macOS: try APFS clone via `cp -c` (copy-on-write, ~instant)
    #[cfg(target_os = "macos")]
    {
        let clone = std::process::Command::new("cp")
            .args(["-c", "--"])
            .arg(src)
            .arg(dst)
            .status();
        if clone.map(|s| s.success()).unwrap_or(false) {
            std::fs::remove_file(src)?;
            return Ok(());
        }
        // `dst` did not exist before, so anything there now is a partial clone.
        let _ = std::fs::remove_file(dst);
    }

    // Generic fallback: copy (keeps permissions) + delete. A failed copy must not leave a
    // partial file behind, and the source is only removed after the copy fully succeeded.
    if let Err(e) = std::fs::copy(src, dst) {
        let _ = std::fs::remove_file(dst);
        return Err(e);
    }
    std::fs::remove_file(src)?;
    Ok(())
}

/// Move a directory as a unit: rename when possible, else recursive copy + delete.
pub(crate) fn fast_move_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    // A folder can never move into itself: the copy fallback would recurse, and the final
    // remove of `src` would also delete the copy inside it.
    if dst.starts_with(src) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Cannot move {} into itself", src.display()),
        ));
    }
    // Safety guard: NEVER overwrite an existing destination directory, file, or symlink!
    if dst.symlink_metadata().is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("Destination directory already exists: {}", dst.display()),
        ));
    }

    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("cp")
            .args(["-R", "--"])
            .arg(src)
            .arg(dst)
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            std::fs::remove_dir_all(src)?;
            return Ok(());
        }
        // Clean partial dest if `cp` left anything behind.
        let _ = std::fs::remove_dir_all(dst);
    }

    if let Err(e) = copy_dir_recursive(src, dst) {
        let _ = std::fs::remove_dir_all(dst);
        return Err(e);
    }
    std::fs::remove_dir_all(src)?;
    Ok(())
}

pub(crate) fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src).follow_links(false) {
        let entry = entry?;
        let rel = match entry.path().strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest_path = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

/// Move one file or folder into `target_dir`, creating it if needed. A name that already
/// exists there is never overwritten: the incoming item gets a unique name (`name_1.ext`,
/// `Folder_2`). Returns whether the item left its source.
pub(crate) fn move_into_dir(path: &Path, target_dir: &Path) -> bool {
    if !target_dir.is_dir() && std::fs::create_dir_all(target_dir).is_err() {
        return false;
    }
    let Ok(meta) = path.symlink_metadata() else {
        return false;
    };
    if meta.is_dir() {
        let dest = unique_dir_dest(target_dir, path.file_name().unwrap_or_default());
        fast_move_dir(path, &dest).is_ok()
    } else if meta.is_file() {
        let dest = unique_file_dest(target_dir, path);
        fast_move(path, &dest).is_ok()
    } else {
        false
    }
}

/// Unique destination for a directory: `Name`, then `Name_2`, `Name_3`, …
/// Never overwrites an existing directory, file, or symlink.
pub fn unique_dir_dest(target_dir: &std::path::Path, folder_name: impl AsRef<std::ffi::OsStr>) -> PathBuf {
    let folder_name = folder_name.as_ref();
    let mut dest = target_dir.join(folder_name);
    let mut counter = 2u32;
    while dest.symlink_metadata().is_ok() {
        let mut name = folder_name.to_os_string();
        name.push(format!("_{counter}"));
        dest = target_dir.join(name);
        counter += 1;
    }
    dest
}

/// Unique non-colliding destination path for a file: `Name.ext`, then `Name_1.ext`, `Name_2.ext`, …
/// Never overwrites an existing file, directory, or symlink.
pub fn unique_file_dest(target_dir: &std::path::Path, file_path: &std::path::Path) -> PathBuf {
    // Work on OsStr so names that are not valid UTF-8 keep their real name.
    let original_name = file_path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| "file".into());
    let stem = file_path
        .file_stem()
        .map(|s| s.to_os_string())
        .unwrap_or_else(|| "file".into());
    let ext = file_path.extension().map(|e| e.to_os_string());

    let mut dest = target_dir.join(&original_name);
    let mut counter = 1u32;
    while dest.symlink_metadata().is_ok() {
        let mut new_name = stem.clone();
        new_name.push(format!("_{counter}"));
        if let Some(ext) = &ext {
            new_name.push(".");
            new_name.push(ext);
        }
        dest = target_dir.join(&new_name);
        counter += 1;
    }
    dest
}

/// Drop search hits that live under a matched folder (folder is the selectable unit).
pub(crate) fn collapse_search_results(entries: Vec<ExplorerEntry>) -> Vec<ExplorerEntry> {
    let mut dir_paths: Vec<PathBuf> = entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| e.path.clone())
        .collect();
    // Shallowest first so parents win over nested matching folders.
    dir_paths.sort_by_key(|p| p.components().count());

    let mut keep_dirs: Vec<PathBuf> = Vec::new();
    for d in dir_paths {
        let under_kept = keep_dirs.iter().any(|k| path_is_strict_descendant(&d, k));
        if !under_kept {
            keep_dirs.push(d);
        }
    }

    entries
        .into_iter()
        .filter(|e| {
            if e.is_dir {
                keep_dirs.iter().any(|k| k == &e.path)
            } else {
                !keep_dirs
                    .iter()
                    .any(|k| path_is_strict_descendant(&e.path, k))
            }
        })
        .collect()
}

/// True for known OS/dev junk that should never appear in Explorer listings.
pub fn is_system_entry(name: &str, is_dir: bool) -> bool {
    // AppleDouble / Finder metadata (`._photo.jpg`) — always hide, even when
    // showing other hidden files.
    if name.starts_with("._") {
        return true;
    }
    let lower = name.to_lowercase();
    if is_dir {
        matches!(
            lower.as_str(),
            "node_modules"
                | ".git"
                | "__pycache__"
                | ".trash"
                | "$recycle.bin"
                | "proc"
                | "sys"
                | "dev"
        )
    } else {
        matches!(
            lower.as_str(),
            ".ds_store" | "thumbs.db" | "desktop.ini" | ".localized"
        )
    }
}

pub(crate) fn path_is_strict_descendant(child: &Path, ancestor: &Path) -> bool {
    child.starts_with(ancestor) && child != ancestor
}

/// Walk `root` for media/archive files (used when staging a checked search folder).
pub(crate) fn collect_media_files_under(root: &Path, show_hidden: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            if is_system_entry(&n, e.file_type().is_dir()) {
                return false;
            }
            if !show_hidden && n.starts_with('.') {
                return false;
            }
            true
        })
        .flatten()
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if is_system_entry(&name, false) {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        if MEDIA_EXTS.contains(&ext.as_str()) {
            out.push(path.to_path_buf());
        }
    }
    out
}
