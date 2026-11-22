//! Preview modal, folder modal and result bookkeeping.
use super::*;

impl SortwiseApp {
    /// Open the shared preview from a Files-tab card. Arrow keys then step through the Files list.
    pub fn files_open_preview(&mut self, path: PathBuf) {
        self.explorer_open_preview(path);
        self.files_preview_active = true;
    }

    pub fn explorer_open_preview(&mut self, path: PathBuf) {
        self.files_preview_active = false;
        self.clear_preview_cache();
        if let Some(q_idx) = self.explorer_find_queue_for_path(&path) {
            self.explorer_preview_queue_pick = Some(q_idx);
        }
        self.explorer_preview_path = Some(path);
        self.explorer_preview_open = true;
        self.explorer_preview_rotation = 0;
    }

    pub fn folder_modal_open_dir(&mut self, folder: PathBuf) {
        self.folder_modal_path = Some(folder);
        self.folder_modal_open = true;
        self.folder_modal_filter.clear();
        self.folder_modal_selected = None;
        self.folder_modal_reload();
    }

    pub fn folder_modal_close(&mut self) {
        self.folder_modal_open = false;
        self.folder_modal_path = None;
        self.folder_modal_entries.clear();
        self.folder_modal_loaded_path = None;
        self.folder_modal_filter.clear();
        self.folder_modal_selected = None;
        self.close_explorer_preview();
    }

    pub fn folder_modal_reload(&mut self) {
        let Some(path) = self.folder_modal_path.clone() else { return };
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&path) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let meta = e.metadata().ok();
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                if is_system_entry(&name, is_dir) {
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
        entries.sort_by(|a, b| {
            if a.is_dir != b.is_dir {
                b.is_dir.cmp(&a.is_dir)
            } else {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            }
        });
        self.folder_modal_entries = entries;
        self.folder_modal_loaded_path = Some(path);
    }

    pub fn folder_modal_prev(&mut self) {
        let Some(cur_path) = self.folder_modal_path.clone() else { return };
        let folders: Vec<PathBuf> = self
            .explorer_entries
            .iter()
            .filter(|e| e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        if let Some(pos) = folders.iter().position(|p| p == &cur_path) {
            if pos > 0 {
                self.folder_modal_open_dir(folders[pos - 1].clone());
            }
        }
    }

    pub fn folder_modal_next(&mut self) {
        let Some(cur_path) = self.folder_modal_path.clone() else { return };
        let folders: Vec<PathBuf> = self
            .explorer_entries
            .iter()
            .filter(|e| e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        if let Some(pos) = folders.iter().position(|p| p == &cur_path) {
            if pos + 1 < folders.len() {
                self.folder_modal_open_dir(folders[pos + 1].clone());
            }
        }
    }

    pub fn folder_modal_open_in_grid(&mut self) {
        if let Some(path) = self.folder_modal_path.clone() {
            self.folder_modal_close();
            self.explorer_navigate(path);
        }
    }

    /// Replace the scan result. Bumps the results epoch and drops any pending
    /// bulk action, since its plan was computed against the old result.
    pub fn set_last_result(&mut self, result: Option<ScanResult>) {
        self.last_result = result.map(Arc::new);
        self.results_epoch = self.results_epoch.wrapping_add(1);
        self.pending_action = None;
    }

    pub fn clear_preview_cache(&mut self) {
        self.preview_path = None;
        self.preview_rotation = 0;
        self.preview_texture = None;
        self.preview_scroll_text = None;
        self.preview_subtitle = None;
        self.inline_video = None;
        #[cfg(feature = "mpv")]
        {
            if let Some(p) = self.mpv.as_ref() {
                p.stop();
            }
        }
        self.mpv_loaded_path = None;
        // Do not touch explorer_preview_open / explorer_preview_path —
        // those close the modal; callers that want dismiss set them explicitly.
    }

    pub fn close_explorer_preview(&mut self) {
        self.files_preview_active = false;
        self.explorer_preview_open = false;
        self.explorer_preview_path = None;
        self.explorer_preview_rotation = 0;
        self.clear_preview_cache();
    }
}
