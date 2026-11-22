//! Helper functions for app logic.

use crate::types::ExplorerEntry;
use crate::constants::MEDIA_EXTS;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn mode_switch(app: &mut crate::app::SortwiseApp, ui: &mut eframe::egui::Ui) {
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut app.mode,
            crate::types::AppMode::Explorer,
            "🗂 Explorer",
        );
        ui.selectable_value(&mut app.mode, crate::types::AppMode::Dedupe, "🔍 Dedupe");
        ui.selectable_value(
            &mut app.mode,
            crate::types::AppMode::CompareSourceDest,
            "⚖ Compare",
        );
        ui.selectable_value(
            &mut app.mode,
            crate::types::AppMode::EmptyFolders,
            "📂 Empty",
        );
        ui.selectable_value(
            &mut app.mode,
            crate::types::AppMode::ArchiveFiles,
            "📦 Archives",
        );
    });
}

pub fn collapse_search_results(entries: Vec<ExplorerEntry>) -> Vec<ExplorerEntry> {
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter(|e| seen.insert(e.path.clone()))
        .collect()
}

pub fn is_system_entry(name: &str, is_dir: bool) -> bool {
    if is_dir {
        matches!(
            name,
            ".git"
                | "node_modules"
                | "__pycache__"
                | ".pytest_cache"
                | "build"
                | "dist"
                | "target"
                | "vendor"
                | ".venv"
                | "venv"
                | ".idea"
                | ".vscode"
                | "Coverage"
                | ".cargo"
                | ".tox"
                | "$recycle.bin"
                | "System Volume Information"
                | "$RECYCLE.BIN"
        )
    } else {
        matches!(
            name,
            ".DS_Store"
                | ".ds_store"
                | "thumbs.db"
                | "Thumbs.db"
                | "desktop.ini"
                | ".localized"
                | "Icon"
        ) || name.starts_with("._")
    }
}

pub fn path_is_strict_descendant(child: &Path, ancestor: &Path) -> bool {
    child.starts_with(ancestor) && child != ancestor
}

pub fn collect_media_files_under(root: &Path, show_hidden: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root)
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

pub fn unique_file_dest(target_dir: &Path, file_path: &Path) -> PathBuf {
    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let original_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    let mut dest = target_dir.join(original_name);
    let mut counter = 1u32;
    while dest.exists() || dest.symlink_metadata().is_ok() {
        let new_name = if ext.is_empty() {
            format!("{stem}_{counter}")
        } else {
            format!("{stem}_{counter}.{ext}")
        };
        dest = target_dir.join(&new_name);
        counter += 1;
    }
    dest
}
