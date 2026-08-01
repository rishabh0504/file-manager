use crate::cancel::CancellationToken;
use crate::excludes::{compile_patterns, path_matches_any};
use crate::platform::file_id;
use crate::types::{ScanConfig, FileEntry};
use glob::Pattern;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn should_skip_dir(path: &Path, config: &ScanConfig, exclude_patterns: &[Pattern]) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if config.skip_hidden && name.starts_with('.') && name != "." && name != ".." {
        return true;
    }
    if config.skip_system_dirs {
        let lower = name.to_lowercase();
        if matches!(
            lower.as_str(),
            "node_modules"
                | ".git"
                | "__pycache__"
                | ".trash"
                | "$recycle.bin"
                | "proc"
                | "sys"
                | "dev"
        ) {
            return true;
        }
    }
    path_matches_any(path, exclude_patterns)
}

fn is_under_excluded_subdir(path: &Path, config: &ScanConfig) -> bool {
    for fc in &config.folder_configs {
        if !path.starts_with(&fc.root) {
            continue;
        }
        for ex in &fc.excluded_subdirs {
            if path.starts_with(ex) {
                return true;
            }
        }
    }
    false
}

/// Build a [`FileEntry`] for a single path (used when re-hashing known duplicate paths).
/// When `respect_min_size` is false, small files are still included so refresh matches prior findings.
pub fn file_entry_for_path(
    path: &Path,
    config: &ScanConfig,
    respect_min_size: bool,
) -> Option<FileEntry> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    if respect_min_size && meta.len() < config.min_file_size {
        return None;
    }
    let root_index = config
        .roots
        .iter()
        .enumerate()
        .filter(|(_, r)| path.starts_with(r))
        .max_by_key(|(_, r)| r.as_os_str().len())
        .map(|(i, _)| i)
        .unwrap_or(0);
    make_entry(path, &meta, root_index)
}

fn make_entry(path: &Path, meta: &std::fs::Metadata, root_index: usize) -> Option<FileEntry> {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let file_id = file_id(&path).ok()?;
    let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let atime_secs = atime_seconds(meta);
    #[cfg(unix)]
    let nlink = {
        use std::os::unix::fs::MetadataExt;
        meta.nlink() as u32
    };
    #[cfg(not(unix))]
    let nlink = 1u32;

    Some(FileEntry {
        path,
        size: meta.len(),
        mtime,
        file_id,
        nlink,
        root_index,
        atime_secs,
    })
}

#[cfg(unix)]
fn atime_seconds(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.atime().max(0) as u64
}

#[cfg(not(unix))]
fn atime_seconds(_meta: &std::fs::Metadata) -> u64 {
    0
}

/// Walk selected roots; skips directories per config. Progress: (files_found_delta, last_path).
pub fn parallel_scan_roots(
    roots: &[PathBuf],
    config: &ScanConfig,
    token: &CancellationToken,
    on_progress: impl Fn(u64, PathBuf),
) -> Vec<FileEntry> {
    let exclude_patterns = compile_patterns(&config.exclude_globs);
    let mut all = Vec::new();
    let mut count: u64 = 0;

    for (root_index, root) in roots.iter().enumerate() {
        if token.is_cancelled() {
            break;
        }
        if !root.is_dir() {
            continue;
        }

        let walker = WalkDir::new(root).follow_links(false).into_iter();
        for entry in walker.filter_entry(|e| {
            let p = e.path();
            if p == root.as_path() {
                return true;
            }
            if should_skip_dir(p, config, &exclude_patterns) {
                return false;
            }
            if is_under_excluded_subdir(p, config) {
                return false;
            }
            true
        }) {
            if token.is_cancelled() {
                break;
            }
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if is_under_excluded_subdir(path, config) {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.len() < config.min_file_size {
                continue;
            }
            if let Some(fe) = make_entry(path, &meta, root_index) {
                count += 1;
                on_progress(count, fe.path.clone());
                all.push(fe);
            }
        }
    }

    all
}

fn is_dir_empty_rec(path: &Path, token: &CancellationToken) -> bool {
    if token.is_cancelled() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();
        
        // Ignore system/hidden files starting with "." (e.g. .DS_Store, .git, etc)
        // and Thumbs.db
        if file_name_str.starts_with('.') || file_name_str == "Thumbs.db" {
            continue;
        }

        if file_type.is_file() || file_type.is_symlink() {
            return false;
        }
        if file_type.is_dir() {
            if !is_dir_empty_rec(&entry.path(), token) {
                return false;
            }
        }
    }
    true
}

pub fn find_empty_folders(
    roots: &[PathBuf],
    token: &CancellationToken,
    on_progress: impl Fn(u64, PathBuf),
) -> Vec<PathBuf> {
    let mut empty_dirs = Vec::new();
    let mut count = 0;
    for root in roots {
        if token.is_cancelled() {
            break;
        }
        if !root.is_dir() {
            continue;
        }
        let walker = WalkDir::new(root).contents_first(true).into_iter();
        for entry in walker {
            if token.is_cancelled() {
                break;
            }
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path == root.as_path() {
                continue;
            }
            if entry.file_type().is_dir() {
                if is_dir_empty_rec(path, token) {
                    count += 1;
                    on_progress(count, path.to_path_buf());
                    empty_dirs.push(path.to_path_buf());
                }
            }
        }
    }
    empty_dirs
}

pub fn find_archive_files(
    roots: &[PathBuf],
    token: &CancellationToken,
    on_progress: impl Fn(u64, PathBuf),
) -> Vec<PathBuf> {
    let mut archives = Vec::new();
    let mut count = 0;
    
    let extensions = ["zip", "rar", "7z", "tar", "gz", "xz", "bz2", "tgz"];

    for root in roots {
        if token.is_cancelled() {
            break;
        }
        if !root.is_dir() {
            continue;
        }
        let walker = WalkDir::new(root).into_iter();
        for entry in walker {
            if token.is_cancelled() {
                break;
            }
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if entry.file_type().is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if extensions.iter().any(|&x| x == ext_lower) {
                        count += 1;
                        on_progress(count, path.to_path_buf());
                        archives.push(path.to_path_buf());
                    }
                }
            }
        }
    }
    archives
}


