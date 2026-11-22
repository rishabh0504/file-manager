//! Explorer: navigation, history, search, listing, sorting and folder actions.
use super::*;

impl SortwiseApp {
    pub fn explorer_navigate(&mut self, path: PathBuf) {
        if path == self.explorer_dir {
            return;
        }
        self.explorer_cancel_search();
        self.explorer_close_preview();
        self.explorer_back.push(self.explorer_dir.clone());
        self.explorer_fwd.clear();
        self.explorer_dir = path;
        self.explorer_selected = None;
        self.explorer_multi.clear();
        self.explorer_anchor = None;
        self.explorer_filter.clear();
        self.explorer_last_filter.clear();
        self.explorer_loaded_dir = None;
    }

    pub fn explorer_go_back(&mut self) {
        if let Some(prev) = self.explorer_back.pop() {
            self.explorer_cancel_search();
            self.explorer_close_preview();
            self.explorer_fwd.push(self.explorer_dir.clone());
            self.explorer_dir = prev;
            self.explorer_selected = None;
        self.explorer_multi.clear();
        self.explorer_anchor = None;
            self.explorer_filter.clear();
            self.explorer_last_filter.clear();
            self.explorer_loaded_dir = None;
        }
    }

    pub fn explorer_go_forward(&mut self) {
        if let Some(next) = self.explorer_fwd.pop() {
            self.explorer_cancel_search();
            self.explorer_close_preview();
            self.explorer_back.push(self.explorer_dir.clone());
            self.explorer_dir = next;
            self.explorer_selected = None;
        self.explorer_multi.clear();
        self.explorer_anchor = None;
            self.explorer_filter.clear();
            self.explorer_last_filter.clear();
            self.explorer_loaded_dir = None;
        }
    }

    pub fn explorer_go_up(&mut self) {
        if let Some(parent) = self.explorer_dir.parent().map(|p| p.to_path_buf()) {
            self.explorer_navigate(parent);
        }
    }

    /// Stop any open preview/player cleanly without dropping the MPV render context.
    pub fn explorer_close_preview(&mut self) {
        self.files_preview_active = false;
        self.explorer_preview_open = false;
        self.explorer_preview_path = None;
        self.preview_path = None;
        self.preview_texture = None;
        self.preview_scroll_text = None;
        self.preview_subtitle = None;
        self.inline_video = None;
        #[cfg(feature = "mpv")]
        {
            if let Some(p) = self.mpv.as_ref() {
                p.stop();
            }
            self.mpv_loaded_path = None;
        }
    }

    /// Cancel any in-flight recursive search and leave search mode.
    pub fn explorer_cancel_search(&mut self) {
        self.explorer_abort_in_flight_search();
        self.explorer_is_recursive_search = false;
        self.explorer_search_pending = false;
        self.explorer_search_typed_at = None;
        self.explorer_search_checked.clear();
    }

    /// Stop a running search worker without clearing displayed recursive results.
    pub(crate) fn explorer_abort_in_flight_search(&mut self) {
        if let Some(token) = self.explorer_search_token.take() {
            token.cancel();
        }
        self.explorer_search_rx = None;
        self.explorer_searching = false;
    }

    /// Check every collapsed search result (Select All).
    pub fn explorer_search_select_all(&mut self) {
        self.explorer_search_checked = self
            .explorer_entries
            .iter()
            .map(|e| e.path.clone())
            .collect();
    }

    pub fn explorer_search_clear_checks(&mut self) {
        self.explorer_search_checked.clear();
    }

    /// Stage all checked search units into queue[idx]. Files are staged as-is;
    /// folders are expanded to media/archive files under them (submit is file-only).
    pub fn explorer_add_checked_to_queue(&mut self, idx: usize) {
        if self.explorer_queues.get(idx).is_none() {
            return;
        }
        let checked: Vec<PathBuf> = self.explorer_search_checked.iter().cloned().collect();
        if checked.is_empty() {
            self.explorer_status = Some("Nothing selected to add.".to_string());
            return;
        }

        let show_hidden = self.explorer_show_hidden;
        let mut to_stage: Vec<PathBuf> = Vec::new();
        let mut folders_expanded = 0usize;
        for path in checked {
            let is_dir = self
                .explorer_entries
                .iter()
                .find(|e| e.path == path)
                .map(|e| e.is_dir)
                .unwrap_or_else(|| path.is_dir());
            if is_dir {
                folders_expanded += 1;
                to_stage.extend(collect_media_files_under(&path, show_hidden));
            } else {
                to_stage.push(path);
            }
        }

        if idx >= self.explorer_queues.len() {
            return;
        }
        let qname = self.explorer_queues[idx].name.clone();
        let mut added = 0usize;
        let mut skipped = 0usize;
        for path in to_stage {
            if self.explorer_queues[idx].items.contains(&path) {
                skipped += 1;
            } else {
                self.unstage_elsewhere(idx, &path);
                self.explorer_queues[idx].items.push(path);
                added += 1;
            }
        }

        self.explorer_status = Some(if folders_expanded > 0 {
            format!(
                "Added {added} file(s) to “{qname}” (from {folders_expanded} folder(s){}).",
                if skipped > 0 {
                    format!(", {skipped} already queued")
                } else {
                    String::new()
                }
            )
        } else {
            format!(
                "Added {added} file(s) to “{qname}”{}.",
                if skipped > 0 {
                    format!(" ({skipped} already queued)")
                } else {
                    String::new()
                }
            )
        });
    }

    /// Spawn a background thread that uses `rg` (with walkdir fallback) to
    /// recursively find images, videos, audio, archives and folders whose names
    /// contain the current filter query.  Results replace `explorer_entries`.
    pub fn explorer_search_recursive(&mut self) {
        self.explorer_abort_in_flight_search();
        self.explorer_search_pending = false;
        self.explorer_search_typed_at = None;

        let token = CancellationToken::new();
        self.explorer_search_token = Some(token.clone_token());
        self.explorer_searching = true;
        self.explorer_is_recursive_search = true;

        let root = self.explorer_dir.clone();
        let query = self.explorer_filter.trim().to_lowercase();
        let show_hidden = self.explorer_show_hidden;
        let (tx, rx): (Sender<Vec<ExplorerEntry>>, Receiver<Vec<ExplorerEntry>>) =
            std::sync::mpsc::channel();
        self.explorer_search_rx = Some(rx);

        std::thread::spawn(move || {
            // -----------------------------------------------------------------
            // rg glob — exhaustive union of every extension category() handles
            // -----------------------------------------------------------------
            const MEDIA_GLOB: &str = "*.{png,jpg,jpeg,gif,webp,bmp,tiff,heic,\
 mp4,mov,m4v,webm,mkv,avi,wmv,mpg,mpeg,\
 mp3,wav,flac,aac,ogg,m4a,\
 zip,rar,7z,tar,gz,bz2,xz}";

            let mut entries: Vec<ExplorerEntry> = Vec::new();

            // -----------------------------------------------------------------
            // 1. File search via rg --files
            // -----------------------------------------------------------------
            let rg_result = std::process::Command::new("rg")
                .arg("--files")
                .args(if show_hidden { vec!["--hidden"] } else { vec![] })
                .args(["--glob", MEDIA_GLOB, "--"])
                .arg(&root)
                .output();

            match rg_result {
                Ok(output) if output.status.success() || !output.stdout.is_empty() => {
                    for line in output.stdout.split(|&b| b == b'\n') {
                        if token.is_cancelled() {
                            return;
                        }
                        let path_str = match std::str::from_utf8(line) {
                            Ok(s) => s.trim(),
                            Err(_) => continue,
                        };
                        if path_str.is_empty() {
                            continue;
                        }
                        let path = std::path::PathBuf::from(path_str);
                        // Filter by query against file name only
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        if !query.is_empty() && !name.contains(&query) {
                            continue;
                        }
                        if let Ok(meta) = std::fs::metadata(&path) {
                            entries.push(ExplorerEntry {
                                name: path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default(),
                                path: path.clone(),
                                is_dir: false,
                                size: meta.len(),
                                modified: meta.modified().ok(),
                            });
                        }
                    }
                }
                _ => {
                    // -----------------------------------------------------------
                    // Fallback: walkdir with manual extension filter
                    // -----------------------------------------------------------
                    for entry in walkdir::WalkDir::new(&root)
                        .follow_links(false)
                        .into_iter()
                        .filter_entry(|e| {
                            if token.is_cancelled() {
                                return false;
                            }
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
                        if token.is_cancelled() {
                            return;
                        }
                        if !entry.file_type().is_file() {
                            continue;
                        }
                        let path = entry.path();
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|s| s.to_lowercase())
                            .unwrap_or_default();
                        if !MEDIA_EXTS.contains(&ext.as_str()) {
                            continue;
                        }
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        if !query.is_empty() && !name.contains(&query) {
                            continue;
                        }
                        if let Ok(meta) = entry.metadata() {
                            entries.push(ExplorerEntry {
                                name: path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default(),
                                path: path.to_path_buf(),
                                is_dir: false,
                                size: meta.len(),
                                modified: meta.modified().ok(),
                            });
                        }
                    }
                }
            }

            // -----------------------------------------------------------------
            // 2. Folder search via walkdir (rg --files skips directories)
            // -----------------------------------------------------------------
            for entry in walkdir::WalkDir::new(&root)
                .follow_links(false)
                .min_depth(1)
                .into_iter()
                .filter_entry(|e| {
                    if token.is_cancelled() {
                        return false;
                    }
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
                if token.is_cancelled() {
                    return;
                }
                if !entry.file_type().is_dir() {
                    continue;
                }
                let path = entry.path();
                let name_str = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !query.is_empty() && !name_str.contains(&query) {
                    continue;
                }
                if let Ok(meta) = entry.metadata() {
                    entries.push(ExplorerEntry {
                        name: path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        path: path.to_path_buf(),
                        is_dir: true,
                        size: 0,
                        modified: meta.modified().ok(),
                    });
                }
            }

            if !token.is_cancelled() {
                entries.retain(|e| !is_system_entry(&e.name, e.is_dir));
                let collapsed = collapse_search_results(entries);
                let _ = tx.send(collapsed);
            }
        });
    }

    pub fn explorer_ensure_loaded(&mut self) {
        if self.explorer_loaded_dir.as_ref() == Some(&self.explorer_dir) {
            return;
        }
        self.explorer_reload();
    }

    pub fn explorer_reload(&mut self) {
        self.navigation_epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.thumbnail_loading.clear();
        self.explorer_error = None;
        let mut entries = Vec::new();
        match std::fs::read_dir(&self.explorer_dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    let meta = e.metadata().ok();
                    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    // Always include hidden; never include known system junk.
                    if is_system_entry(&name, is_dir) {
                        continue;
                    }
                    // In Folders mode, always include hidden child folders; in other modes respect explorer_show_hidden.
                    if !self.explorer_show_hidden && self.explorer_view_mode != ExplorerViewMode::Folders && name.starts_with('.') {
                        continue;
                    }
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    let modified = meta.as_ref().and_then(|m| m.modified().ok());
                    entries.push(ExplorerEntry {
                        name,
                        path: e.path(),
                        is_dir,
                        size,
                        modified,
                    });
                }
            }
            Err(err) => {
                self.explorer_error = Some(err.to_string());
            }
        }
        self.explorer_entries = entries;
        self.explorer_loaded_dir = Some(self.explorer_dir.clone());
        self.explorer_sort_entries();
    }

    pub fn explorer_sort_entries(&mut self) {
        let key = self.explorer_sort_key;
        let asc = self.explorer_sort_asc;
        self.explorer_entries.sort_by(|a, b| {
            // Folders always group before files.
            if a.is_dir != b.is_dir {
                return b.is_dir.cmp(&a.is_dir);
            }
            let ord = match key {
                ExplorerSortKey::Name => a.name.chars().map(|c| c.to_ascii_lowercase()).cmp(b.name.chars().map(|c| c.to_ascii_lowercase())),
                ExplorerSortKey::Size => a.size.cmp(&b.size),
                ExplorerSortKey::Modified => a.modified.cmp(&b.modified),
                ExplorerSortKey::Kind => {
                    let ea = a.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let eb = b.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    ea.chars().map(|c| c.to_ascii_lowercase()).cmp(eb.chars().map(|c| c.to_ascii_lowercase()))
                }
            };
            if asc {
                ord
            } else {
                ord.reverse()
            }
        });
    }

    pub fn explorer_set_sort(&mut self, key: ExplorerSortKey) {
        if self.explorer_sort_key == key {
            self.explorer_sort_asc = !self.explorer_sort_asc;
        } else {
            self.explorer_sort_key = key;
            self.explorer_sort_asc = true;
        }
        self.explorer_sort_entries();
    }

    /// Create a uniquely-named new folder in the current directory.
    pub fn explorer_new_folder(&mut self) {
        let base = self.explorer_dir.clone();
        let mut target = base.join("New Folder");
        let mut n = 2;
        while target.exists() {
            target = base.join(format!("New Folder {n}"));
            n += 1;
        }
        match std::fs::create_dir(&target) {
            Ok(()) => {
                self.explorer_status = Some(format!(
                    "Created {}",
                    target.file_name().unwrap_or_default().to_string_lossy()
                ));
                self.explorer_reload();
            }
            Err(e) => self.explorer_status = Some(format!("Could not create folder: {e}")),
        }
    }

    /// Extract a `.zip` into a sibling folder. On success, queues a prompt to
    /// move the original archive to the Trash.
    pub fn explorer_extract_zip(&mut self, zip: &std::path::Path) {
        let parent = zip.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| self.explorer_dir.clone());
        let stem = zip
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "extracted".to_string());
        let mut dest = parent.join(&stem);
        let mut n = 2;
        while dest.exists() {
            dest = parent.join(format!("{stem} ({n})"));
            n += 1;
        }
        if let Err(e) = std::fs::create_dir_all(&dest) {
            self.explorer_status = Some(format!("Extract failed: {e}"));
            return;
        }

        #[cfg(windows)]
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                    zip.display(),
                    dest.display()
                ),
            ])
            .status();
        #[cfg(not(windows))]
        let status = std::process::Command::new("/usr/bin/unzip")
            .arg("-o")
            .arg(zip)
            .arg("-d")
            .arg(&dest)
            .status();

        match status {
            Ok(s) if s.success() => {
                self.explorer_status = None;
                self.extract_banner = Some(ExtractBanner {
                    zip: zip.to_path_buf(),
                    dest: dest.clone(),
                    dir: self.explorer_dir.clone(),
                });
                self.explorer_reload();
            }
            Ok(s) => {
                self.explorer_status = Some(format!("Extract failed (exit {:?}).", s.code()));
            }
            Err(e) => {
                self.explorer_status = Some(format!("Extract failed: {e}"));
            }
        }
    }
}
