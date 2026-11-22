//! All files tab: recursive listing and staging of individual files.
use super::*;

impl SortwiseApp {
    /// Re-read the current directory if it changed (cheap no-op otherwise).
    /// Files view: start a background recursive listing when the folder changes, and
    /// collect a finished listing. Call once per frame while the Files view is shown.
    pub fn explorer_files_ensure(&mut self) {
        use std::sync::mpsc::TryRecvError;
        if let Some(rx) = &self.explorer_files_rx {
            match rx.try_recv() {
                Ok(listing) => {
                    self.explorer_files = listing.files;
                    self.explorer_files_truncated = listing.truncated;
                    self.explorer_files_loading = false;
                    self.explorer_files_rx = None;
                    self.explorer_files_gen += 1;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.explorer_files_loading = false;
                    self.explorer_files_rx = None;
                }
            }
        }
        let Some(dir) = self.files_view_dir.clone() else {
            return;
        };
        let same_dir = self.explorer_files_root.as_ref() == Some(&dir);
        if same_dir && !self.explorer_files_rescan {
            return;
        }
        // Refresh of the same folder keeps the checked files and the old list until the new one arrives.
        let keep_state = same_dir;
        self.explorer_files_rescan = false;
        if let Some(old) = self.explorer_files_cancel.take() {
            old.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let root = dir.clone();
        let skip_system = self.skip_system_dirs;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut listing = sortwise::recursive_files::list_files(
                &root,
                skip_system,
                sortwise::recursive_files::DEFAULT_CAP,
                &worker_cancel,
            );
            sortwise::recursive_files::sort_by_size_desc(&mut listing.files);
            let _ = tx.send(listing);
        });
        self.explorer_files_root = Some(dir);
        self.explorer_files_cancel = Some(cancel);
        self.explorer_files_rx = Some(rx);
        self.explorer_files_loading = true;
        if !keep_state {
            self.explorer_files.clear();
            self.explorer_files_truncated = false;
            self.explorer_files_checked.clear();
        }
        self.explorer_files_gen += 1;
    }

    /// Files tab Refresh: re-walk the current folder and pick up new, renamed or deleted files.
    pub fn explorer_files_refresh(&mut self) {
        self.explorer_files_rescan = true;
        self.explorer_files_gen += 1;
    }

    /// Drop one moved path (and everything under it, for a moved folder) from the Files list.
    /// Called after a successful move, so moved items disappear from the list.
    pub fn explorer_files_forget_path(&mut self, path: &Path, was_dir: bool) {
        let gone = |p: &Path| p == path || (was_dir && p.starts_with(path));
        let before = self.explorer_files.len();
        self.explorer_files.retain(|f| !gone(&f.path));
        self.explorer_files_checked.retain(|p| !gone(p));
        if self.explorer_files.len() != before {
            self.explorer_files_gen += 1;
        }
    }

    /// Drop every listed file that no longer exists on disk. Called after folder-queue submits,
    /// which move many files at once.
    pub fn explorer_files_prune_missing(&mut self) {
        let before = self.explorer_files.len();
        self.explorer_files.retain(|f| f.path.exists());
        let remaining: HashSet<&PathBuf> = self.explorer_files.iter().map(|f| &f.path).collect();
        self.explorer_files_checked.retain(|p| remaining.contains(p));
        if self.explorer_files.len() != before {
            self.explorer_files_gen += 1;
        }
    }

    /// Files view: stage files. `Some(name)` adds them to that folder queue's items (Folder
    /// queues tab, moved on Submit). `None` adds them to Items (loose staging, no queue).
    /// A file ends up in exactly one place, so it can never be moved twice. Moves nothing on disk.
    /// Returns how many files were newly placed.
    pub fn explorer_files_stage(&mut self, paths: &[PathBuf], queue: Option<String>) -> usize {
        let idx = match &queue {
            Some(name) => match self.explorer_queues.iter().position(|q| &q.name == name) {
                Some(i) => Some(i),
                None => {
                    self.explorer_status = Some(format!("Queue “{name}” not found."));
                    return 0;
                }
            },
            None => None,
        };
        let mut added = 0;
        for path in paths {
            match idx {
                Some(i) => {
                    if self.explorer_queues[i].items.contains(path) {
                        continue;
                    }
                    self.explorer_files_unstage(path);
                    self.explorer_queues[i].items.push(path.clone());
                    added += 1;
                }
                None => {
                    if self.movable_items.iter().any(|m| &m.path == path) {
                        continue;
                    }
                    self.explorer_files_unstage(path);
                    self.movable_items.push(MovableItem { path: path.clone(), target_queue: None });
                    added += 1;
                }
            }
        }
        self.explorer_status = Some(match &queue {
            Some(q) => format!("Added {added} file(s) to queue “{q}”."),
            None => format!("Added {added} file(s) to Items."),
        });
        added
    }

    /// Files view: remove a file from every folder queue and from Items. Never touches the file.
    pub fn explorer_files_unstage(&mut self, path: &Path) {
        for q in &mut self.explorer_queues {
            q.items.retain(|p| p != path);
        }
        self.movable_items.retain(|m| m.path != path);
    }

    /// Files view: where a file is staged. A folder queue wins over Items.
    pub fn explorer_files_stage_of(&self, path: &Path) -> FilesStage {
        if let Some(q) = self.explorer_queues.iter().find(|q| q.items.iter().any(|p| p == path)) {
            return FilesStage::Queue(q.name.clone());
        }
        match self.movable_items.iter().find(|m| m.path == path) {
            Some(m) => FilesStage::Item(m.target_queue.clone()),
            None => FilesStage::None,
        }
    }

    /// Files view: every staged path, from folder queues and Items.
    pub fn explorer_files_staged_set(&self) -> std::collections::HashSet<PathBuf> {
        self.explorer_queues
            .iter()
            .flat_map(|q| q.items.iter().cloned())
            .chain(self.movable_items.iter().map(|m| m.path.clone()))
            .collect()
    }

    /// Files view: order-independent fingerprint of the staged set, so the list is refiltered
    /// whenever staging changes anywhere (cards, preview modal, staging panel).
    pub fn explorer_files_staged_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let set = self.explorer_files_staged_set();
        let mut acc = set.len() as u64;
        for p in &set {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            p.hash(&mut h);
            acc = acc.wrapping_add(h.finish());
        }
        acc
    }
}
