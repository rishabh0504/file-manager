//! Append-only record of bulk actions (Implementation-Plan Phase 1, finding R1).
//!
//! One JSON file per action under `<data_dir>/sortwise/actions/`.
//! Written after every bulk trash or hard-link, including partial failures.

use serde::Serialize;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct ActionLogEntry<'a> {
    pub timestamp: String,
    pub app_version: &'static str,
    /// Human-readable action name, e.g. "trash_selected", "apply_delete".
    pub action: &'a str,
    /// Every path the plan targeted.
    pub planned: &'a [PathBuf],
    /// Paths the action finished on (moved to Trash, or replaced by a hard link).
    pub completed: &'a [PathBuf],
    /// Planned paths that did not finish.
    pub not_completed: Vec<PathBuf>,
    /// Free-text outcome, e.g. the batch error or "replaced 12 of 14".
    pub note: Option<&'a str>,
}

impl<'a> ActionLogEntry<'a> {
    pub fn new(action: &'a str, planned: &'a [PathBuf], completed: &'a [PathBuf], note: Option<&'a str>) -> Self {
        let done: std::collections::HashSet<&PathBuf> = completed.iter().collect();
        let not_completed = planned
            .iter()
            .filter(|p| !done.contains(p))
            .cloned()
            .collect();
        Self {
            timestamp: chrono::Local::now().to_rfc3339(),
            app_version: env!("CARGO_PKG_VERSION"),
            action,
            planned,
            completed,
            not_completed,
            note,
        }
    }
}

/// Directory that holds action logs, if the platform has a data directory.
pub fn log_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("sortwise").join("actions"))
}

/// Write one entry to `dir`, returning the file path. `dir` is created if missing.
pub fn write_to(dir: &Path, entry: &ActionLogEntry) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f");
    let path = dir.join(format!("{stamp}.json"));
    let json = serde_json::to_string_pretty(entry)
        .map_err(io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Write to the default log directory.
pub fn write(entry: &ActionLogEntry) -> io::Result<PathBuf> {
    let dir = log_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no data directory"))?;
    write_to(&dir, entry)
}
