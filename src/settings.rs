//! Scan preferences that survive restarts (Implementation-Plan U16).
//!
//! Stored as versioned JSON in the platform config directory. Writes go to a
//! temporary file and are renamed into place, so a crash cannot leave a half-written file.
//!
//! Deliberately not persisted: the action mode (always starts as Dry run, so a
//! destructive mode is never re-armed silently) and the folder list.

use crate::types::IoProfile;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub skip_system_dirs: bool,
    pub skip_hidden: bool,
    pub min_file_size: u64,
    pub use_hash_cache: bool,
    pub skip_logs_and_locks: bool,
    pub io_profile: IoProfile,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            skip_system_dirs: true,
            skip_hidden: false,
            min_file_size: 0,
            use_hash_cache: true,
            skip_logs_and_locks: false,
            io_profile: IoProfile::Auto,
        }
    }
}

/// `<config_dir>/sortwise/settings.json`.
pub fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("sortwise").join("settings.json"))
}

/// Read settings. A missing file is not an error (first run returns defaults).
/// A corrupt file is an error, so the caller can tell the user.
pub fn load_from(path: &Path) -> io::Result<Settings> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(e),
    }
}

/// Write settings atomically: temp file in the same directory, then rename.
pub fn save_to(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sfm_settings_{}_{name}", std::process::id())).join("settings.json")
    }

    #[test]
    fn round_trip_preserves_values() {
        let path = temp_file("roundtrip");
        let s = Settings { min_file_size: 10 * 1024 * 1024, io_profile: IoProfile::HddSafe, ..Settings::default() };
        save_to(&path, &s).unwrap();
        assert_eq!(load_from(&path).unwrap(), s);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_file_gives_defaults() {
        let path = temp_file("missing");
        assert_eq!(load_from(&path).unwrap(), Settings::default());
    }

    #[test]
    fn corrupt_file_is_reported_not_ignored() {
        let path = temp_file("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load_from(&path).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn unknown_or_missing_fields_fall_back_to_defaults() {
        let path = temp_file("partial");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"skip_hidden": true}"#).unwrap();
        let s = load_from(&path).unwrap();
        assert!(s.skip_hidden);
        assert_eq!(s.min_file_size, 0);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
