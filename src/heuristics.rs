use crate::types::FileEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDisposition {
    HashNormally,
    SkipHeuristic,
}

/// Extensions worth deduplicating — large binary files where duplicates
/// cause real disk waste. Text files, code, and config files are excluded
/// entirely; the user selects a directory and wants media/archive duplicates.
fn is_dedup_candidate(ext: &str) -> bool {
    matches!(
        ext,
        // ── Images ──────────────────────────────────────────────────────
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "tif"
            | "webp" | "heic" | "heif" | "raw" | "cr2" | "nef" | "arw"
            | "dng" | "svg" | "ico" | "psd" | "ai"
        // ── Videos ──────────────────────────────────────────────────────
            | "mp4" | "mov" | "avi" | "mkv" | "wmv" | "flv" | "webm"
            | "m4v" | "mpg" | "mpeg" | "3gp" | "ts" | "mts" | "m2ts"
            | "vob" | "rmvb" | "divx"
        // ── Archives / compressed ────────────────────────────────────────
            | "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz"
            | "tgz" | "zst" | "lz4" | "cab" | "iso" | "dmg" | "pkg"
        // ── Audio (also large) ───────────────────────────────────────────
            | "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "wma"
            | "aiff" | "alac" | "opus"
    )
}

/// Classify a file for deduplication.
///
/// Only files whose extension appears in the media/archive allowlist are
/// hashed. Everything else — text files, code, configs, dotfiles — is
/// skipped. The `skip_logs_and_locks` argument is retained for API
/// compatibility but has no effect; log/lock files are already outside
/// the allowlist.
pub fn classify(entry: &FileEntry, _skip_logs_and_locks: bool) -> FileDisposition {
    match entry.path.extension().and_then(|e| e.to_str()) {
        Some(ext) if is_dedup_candidate(&ext.to_lowercase()) => FileDisposition::HashNormally,
        // No extension or non-media extension → not worth hashing
        _ => FileDisposition::SkipHeuristic,
    }
}
