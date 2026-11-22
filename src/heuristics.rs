use crate::types::FileEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDisposition {
    HashNormally,
    SkipHeuristic,
}

/// OS metadata, partial downloads and temp/lock files. These are never user
/// content and only create false duplicates. Matched case-insensitively.
fn is_junk(name: &str, ext: Option<&str>) -> bool {
    const JUNK_NAMES: &[&str] = &[".ds_store", "thumbs.db", "desktop.ini", ".localized"];
    const JUNK_EXTS: &[&str] = &["tmp", "temp", "part", "crdownload", "download", "lock", "swp"];
    JUNK_NAMES.contains(&name) || ext.is_some_and(|e| JUNK_EXTS.contains(&e))
}

/// Classify a file for deduplication.
///
/// Every regular file is hashed except junk (see `is_junk`). Size and mtime are
/// never used to decide duplicates; content hashing does that. The
/// `skip_logs_and_locks` argument is retained for API compatibility; log/lock
/// files are covered by `is_junk`.
pub fn classify(entry: &FileEntry, _skip_logs_and_locks: bool) -> FileDisposition {
    let name = entry
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    let ext = entry.path.extension().and_then(|e| e.to_str()).map(str::to_lowercase);
    if is_junk(&name, ext.as_deref()) {
        FileDisposition::SkipHeuristic
    } else {
        FileDisposition::HashNormally
    }
}

/// Coarse file kind used for result filters (U11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    Archive,
    Other,
}

impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Image => "Images",
            MediaKind::Video => "Videos",
            MediaKind::Audio => "Audio",
            MediaKind::Archive => "Archives",
            MediaKind::Other => "Other",
        }
    }
}

/// Classify a path by extension (case-insensitive).
pub fn media_kind(path: &std::path::Path) -> MediaKind {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_ascii_lowercase(),
        None => return MediaKind::Other,
    };
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "tif" | "webp" | "heic" | "heif" | "raw"
        | "cr2" | "nef" | "arw" | "dng" | "svg" | "ico" | "psd" | "ai" => MediaKind::Image,
        "mp4" | "mov" | "avi" | "mkv" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg" | "3gp"
        | "ts" | "mts" | "m2ts" | "vob" | "rmvb" | "divx" => MediaKind::Video,
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma" | "aiff" | "alac" | "opus" => MediaKind::Audio,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "zst" | "lz4" | "cab" | "iso"
        | "dmg" | "pkg" => MediaKind::Archive,
        _ => MediaKind::Other,
    }
}
