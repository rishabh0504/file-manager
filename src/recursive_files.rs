//! Recursive flat file listing for the Explorer "Files" view.
//!
//! Walks a folder tree and returns every regular file except hidden entries
//! (name starts with `.`), known system files, system/VCS/cache directories
//! (`scanner::is_system_dir`, when `skip_system_dirs` is set), and plain-text
//! and source-code files (see `TEXT_EXTS`). Symlinks are never followed.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use walkdir::WalkDir;

/// Upper bound on listed files. Walk stops once reached and `Listing::truncated` is set.
pub const DEFAULT_CAP: usize = 50_000;

/// Extensions treated as text / source code and excluded from the listing.
const TEXT_EXTS: &[&str] = &[
    "txt", "md", "markdown", "rtf", "csv", "tsv", "log", "json", "xml", "yaml", "yml", "toml",
    "ini", "cfg", "conf", "html", "htm", "css", "js", "ts", "tsx", "jsx", "rs", "py", "sh", "c",
    "h", "cpp", "hpp", "java", "go", "swift", "kt", "rb", "php",
];

/// System files that are not dot-prefixed.
const SYSTEM_FILE_NAMES: &[&str] = &["thumbs.db", "desktop.ini", "ehthumbs.db"];

#[derive(Debug, Clone)]
pub struct RecursiveFile {
    pub path: PathBuf,
    /// Path relative to the listing root, for display and search.
    pub rel: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Default)]
pub struct Listing {
    pub files: Vec<RecursiveFile>,
    pub truncated: bool,
    pub cancelled: bool,
    /// Entries that could not be read (permission denied etc.); skipped, not fatal.
    pub skipped: usize,
}

fn ext_lower(path: &Path) -> Option<String> {
    path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase())
}

/// True for plain-text and source-code files.
pub fn is_text_file(path: &Path) -> bool {
    ext_lower(path).is_some_and(|e| TEXT_EXTS.contains(&e.as_str()))
}

/// True for names that must never be listed (dot-files and known system files).
pub fn is_hidden_or_system_file(name: &str) -> bool {
    name.starts_with('.') || SYSTEM_FILE_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

/// Walk `root` recursively and return the filtered files, unsorted.
/// The caller decides ordering; the default UI order is size descending.
pub fn list_files(root: &Path, skip_system_dirs: bool, cap: usize, cancel: &AtomicBool) -> Listing {
    let mut out = Listing::default();

    let walker = WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| {
        if e.depth() == 0 {
            return true;
        }
        let name = e.file_name().to_string_lossy();
        if name.starts_with('.') {
            return false;
        }
        if e.file_type().is_dir() {
            return !(skip_system_dirs && crate::scanner::is_system_dir_path(e.path()));
        }
        true
    });

    for entry in walker {
        if cancel.load(Ordering::Relaxed) {
            out.cancelled = true;
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                out.skipped += 1;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if is_hidden_or_system_file(&name) || is_text_file(entry.path()) {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                out.skipped += 1;
                continue;
            }
        };
        if out.files.len() >= cap {
            out.truncated = true;
            break;
        }
        let path = entry.path().to_path_buf();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
        out.files.push(RecursiveFile {
            path,
            rel,
            size: meta.len(),
            modified: meta.modified().ok(),
        });
    }

    out
}

/// Sort in place: size descending by default. Ties broken by relative path for stable order.
pub fn sort_by_size_desc(files: &mut [RecursiveFile]) {
    files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.rel.cmp(&b.rel)));
}
