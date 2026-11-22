//! Global constants.

use std::time::Duration;

pub const SEARCH_DEBOUNCE: Duration = Duration::from_secs(2);

pub const MEDIA_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "heic", "mp4", "mov", "m4v",
    "webm", "mkv", "avi", "wmv", "mpg", "mpeg", "mp3", "wav", "flac", "aac", "ogg",
    "m4a", "zip", "rar", "7z", "tar", "gz", "bz2", "xz",
];
