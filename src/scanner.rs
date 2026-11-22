use crate::cancel::CancellationToken;
use crate::excludes::{compile_patterns, path_matches_any};
use crate::platform::file_id;
use crate::types::{ScanConfig, FileEntry};
use glob::Pattern;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

pub fn is_system_dir(lower_name: &str) -> bool {
    matches!(
        lower_name,
        // VCS
        ".git" | ".svn" | ".hg" | ".bzr"
        // JS / Node
            | "node_modules"
        // Python
            | "__pycache__" | ".tox" | ".pytest_cache" | ".mypy_cache" | ".ruff_cache"
        // macOS system dirs
            | ".trashes" | ".spotlight-v100" | ".fseventsd"
            | ".documentrevisions-v100" | ".temporaryitems" | ".mobilebackups"
            | ".vol" | ".apdisk" | "__macosx"
        // Windows system dirs (lowercased)
            | "$recycle.bin" | "system volume information" | "$sysreset"
            | "$winreagent"
        // Build / cache tools
            | ".trash" | ".gradle" | ".m2" | ".cargo"
    )
}

/// Names that are system folders only at the top of a drive (`/dev`, `/proc`, `C:\\Recovery`).
/// Anywhere else they are ordinary user folders (a "Dev" or "Run" album) and must be scanned.
fn is_root_only_system_dir(lower_name: &str) -> bool {
    matches!(
        lower_name,
        "proc" | "sys" | "dev" | "run" | "snap" | "lost+found" | "recovery" | "msocache"
    )
}

/// System folder test with the folder's location taken into account.
pub fn is_system_dir_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_lowercase();
    if is_system_dir(&lower) {
        return true;
    }
    let at_drive_root = path.parent().is_some_and(|p| p.parent().is_none());
    at_drive_root && is_root_only_system_dir(&lower)
}

fn should_skip_dir(path: &Path, config: &ScanConfig, exclude_patterns: &[Pattern]) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if config.skip_hidden && name.starts_with('.') && name != "." && name != ".." {
        return true;
    }
    if config.skip_system_dirs && is_system_dir_path(path) {
        return true;
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
    // canonicalize() is skipped: follow_links=false means the path from WalkDir is
    // already the real path — calling realpath() on every file is an unnecessary syscall.
    let path = path.to_path_buf();
    let file_id = file_id(&path).ok()?;
    let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    // Pre-compute mtime_secs once here so the pipeline never needs to stat() again.
    let mtime_secs = mtime
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let ctime_ns = ctime_nanos(meta);
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
        mtime_secs,
        ctime_ns,
        file_id,
        nlink,
        root_index,
        atime_secs,
    })
}

/// Inode change time in ns. On non-Unix targets there is no ctime, so mtime is used as the
/// closest stand-in (weaker against mtime-restoring tools).
#[cfg(unix)]
fn ctime_nanos(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let secs = meta.ctime().max(0) as u64;
    secs.saturating_mul(1_000_000_000).saturating_add(meta.ctime_nsec().max(0) as u64)
}

#[cfg(not(unix))]
fn ctime_nanos(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
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

/// Walk selected roots in parallel using `ignore::WalkBuilder`.
/// Each root is walked with up to 8 threads for fast SSD enumeration.
/// Applies all configured filters: system dirs, hidden, min size, glob excludes.
pub fn parallel_scan_roots(
    roots: &[PathBuf],
    config: &ScanConfig,
    token: &CancellationToken,
    on_progress: impl Fn(u64, PathBuf) + Send + Sync + 'static,
) -> Vec<FileEntry> {
    let exclude_patterns = compile_patterns(&config.exclude_globs);
    let all: Arc<Mutex<Vec<FileEntry>>> = Arc::new(Mutex::new(Vec::new()));
    let count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    // Wrap callback in Arc so it can be shared across multiple roots + threads.
    let on_progress = Arc::new(on_progress);

    // Walking is I/O-bound: saturating a single drive with more than 4 concurrent
    // readdir() threads produces no throughput gain and wastes CPU heat.
    // Using half the cores keeps the system responsive and cool.
    let thread_count = (num_cpus::get() / 2).clamp(2, 4);

    for (root_index, root) in roots.iter().enumerate() {
        if token.is_cancelled() {
            break;
        }
        if !root.is_dir() {
            continue;
        }

        let all = Arc::clone(&all);
        let count = Arc::clone(&count);
        let token = token.clone_token();
        let on_progress = Arc::clone(&on_progress);
        let config_ref = config;
        let excl_ref = &exclude_patterns;
        let root_clone = root.clone();

        WalkBuilder::new(root)
            .threads(thread_count)
            .follow_links(false)
            // Disable ignore-file processing — we do our own filtering.
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .hidden(false)  // we handle hidden ourselves via skip_hidden
            .build_parallel()
            .run(|| {
                let all = Arc::clone(&all);
                let count = Arc::clone(&count);
                let token = token.clone_token();
                let on_progress = Arc::clone(&on_progress);
                let root_for_thread = root_clone.clone();
                Box::new(move |result| {
                    use ignore::WalkState;
                    if token.is_cancelled() {
                        return WalkState::Quit;
                    }
                    let entry = match result {
                        Ok(e) => e,
                        Err(_) => return WalkState::Continue,
                    };
                    let path = entry.path();

                    // Directory filtering (controls descent).
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                        && path != root_for_thread.as_path()
                    {
                        if should_skip_dir(path, config_ref, excl_ref) {
                            return WalkState::Skip;
                        }
                        return WalkState::Continue;
                    }

                    // File filtering.
                    if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                        return WalkState::Continue;
                    }
                    if should_skip_dir(path, config_ref, excl_ref) {
                        return WalkState::Continue;
                    }
                    if is_under_excluded_subdir(path, config_ref) {
                        return WalkState::Continue;
                    }
                    let meta = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => return WalkState::Continue,
                    };
                    if meta.len() < config_ref.min_file_size {
                        return WalkState::Continue;
                    }
                    if let Some(fe) = make_entry(path, &meta, root_index) {
                        let n = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        on_progress(n, fe.path.clone());
                        all.lock().unwrap().push(fe);
                    }
                    WalkState::Continue
                })
            });
    }

    Arc::try_unwrap(all).unwrap().into_inner().unwrap()
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
        if file_type.is_dir()
            && !is_dir_empty_rec(&entry.path(), token) {
                return false;
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
            let Ok(entry): Result<walkdir::DirEntry, _> = entry else { continue };
            let path = entry.path();
            if path == root.as_path() {
                continue;
            }
            if entry.file_type().is_dir()
                && is_dir_empty_rec(path, token) {
                    count += 1;
                    on_progress(count, path.to_path_buf());
                    empty_dirs.push(path.to_path_buf());
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
            let Ok(entry): Result<walkdir::DirEntry, _> = entry else { continue };
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


