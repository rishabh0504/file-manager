use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub size: u64,
    pub mtime_secs: u64,
    pub partial_hash: [u8; 32],
    pub full_hash: Option<[u8; 32]>,
}

#[derive(Debug, Default)]
pub struct HashCache {
    map: HashMap<String, CacheEntry>,
    path: PathBuf,
    dirty: bool,
}

fn mtime_secs(meta: &std::fs::Metadata) -> u64 {
    meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl HashCache {
    pub fn load() -> Self {
        let path = cache_path();
        let map = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            map,
            path,
            dirty: false,
        }
    }

    pub fn save(&mut self) -> io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(&self.map)?;
        fs::write(&self.path, s)?;
        self.dirty = false;
        Ok(())
    }

    pub fn get(&self, path: &Path, size: u64, mtime_secs: u64) -> Option<&CacheEntry> {
        let key = path.to_string_lossy().to_string();
        self.map.get(&key).filter(|e| e.size == size && e.mtime_secs == mtime_secs)
    }

    pub fn insert_partial(&mut self, path: &Path, size: u64, mtime_secs: u64, hash: [u8; 32]) {
        let key = path.to_string_lossy().to_string();
        self.map.insert(
            key,
            CacheEntry {
                size,
                mtime_secs,
                partial_hash: hash,
                full_hash: None,
            },
        );
        self.dirty = true;
    }

    pub fn set_full(&mut self, path: &Path, size: u64, mtime_secs: u64, full: [u8; 32]) {
        let key = path.to_string_lossy().to_string();
        if let Some(e) = self.map.get_mut(&key) {
            if e.size == size && e.mtime_secs == mtime_secs {
                e.full_hash = Some(full);
                self.dirty = true;
            }
        }
    }
}

pub fn entry_mtime_secs(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    Some(mtime_secs(&meta))
}

fn cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("smart-file-manager")
        .join("hash_cache.json")
}
