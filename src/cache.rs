//! Persistent hash cache (Implementation-Plan Phase 3, P-7).
//!
//! Stored in SQLite (WAL). Each scan loads the table once, then writes only the entries
//! it changed, in one transaction. A crash mid-save leaves the previous state intact.
//!
//! The old single-blob cache (`hash_cache.bin` / `.json`) is read once for migration and
//! left in place for one release as a fallback.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    /// Keys changed since the last save.
    dirty: HashSet<String>,
    db_path: PathBuf,
}

fn mtime_secs(meta: &std::fs::Metadata) -> u64 {
    meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS hashes (
    path TEXT PRIMARY KEY,
    size INTEGER NOT NULL,
    mtime_secs INTEGER NOT NULL,
    partial_hash BLOB NOT NULL,
    full_hash BLOB
);";

fn open_db(path: &Path) -> io::Result<Connection> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let conn = Connection::open(path).map_err(io_err)?;
    conn.pragma_update(None, "journal_mode", "WAL").map_err(io_err)?;
    conn.pragma_update(None, "synchronous", "NORMAL").map_err(io_err)?;
    conn.execute_batch(SCHEMA).map_err(io_err)?;
    Ok(conn)
}

fn to_32(bytes: &[u8]) -> Option<[u8; 32]> {
    bytes.try_into().ok()
}

impl HashCache {
    /// Open the default cache. On failure the scan runs without a cache and the error is
    /// printed; the scan itself does not fail.
    pub fn load() -> Self {
        let Some(db) = cache_db_path() else {
            return Self::empty(PathBuf::new());
        };
        match Self::open(&db, cache_legacy_paths()) {
            Ok(cache) => cache,
            Err(e) => {
                eprintln!("hash cache unavailable, scanning without it: {e}");
                Self::empty(PathBuf::new())
            }
        }
    }

    fn empty(db_path: PathBuf) -> Self {
        Self { map: HashMap::new(), dirty: HashSet::new(), db_path }
    }

    /// Load every row from `db_path`. If the database does not exist yet, import the
    /// legacy blob (if any) and mark its rows dirty so the first save writes them.
    pub fn open(db_path: &Path, legacy: Vec<PathBuf>) -> io::Result<Self> {
        let conn = open_db(db_path)?;
        let mut map = HashMap::new();
        {
            let mut stmt = conn
                .prepare("SELECT path, size, mtime_secs, partial_hash, full_hash FROM hashes")
                .map_err(io_err)?;
            let rows = stmt
                .query_map([], |row| {
                    let partial: Vec<u8> = row.get(3)?;
                    let full: Option<Vec<u8>> = row.get(4)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, u64>(2)?,
                        partial,
                        full,
                    ))
                })
                .map_err(io_err)?;
            for row in rows {
                let (path, size, mtime, partial, full) = row.map_err(io_err)?;
                let Some(partial_hash) = to_32(&partial) else { continue };
                map.insert(
                    path,
                    CacheEntry {
                        size,
                        mtime_secs: mtime,
                        partial_hash,
                        full_hash: full.as_deref().and_then(to_32),
                    },
                );
            }
        }

        let mut cache = Self { map, dirty: HashSet::new(), db_path: db_path.to_path_buf() };
        let is_new_db = cache.map.is_empty();
        if is_new_db {
            if let Some(old) = legacy.iter().find(|p| p.exists()) {
                cache.map = read_legacy(old);
                cache.dirty = cache.map.keys().cloned().collect();
                cache.save()?;
            }
        }
        Ok(cache)
    }

    /// Write only the changed entries, in one transaction.
    pub fn save(&mut self) -> io::Result<()> {
        if self.dirty.is_empty() || self.db_path.as_os_str().is_empty() {
            return Ok(());
        }
        let mut conn = open_db(&self.db_path)?;
        let tx = conn.transaction().map_err(io_err)?;
        {
            let mut stmt = tx
                .prepare("INSERT OR REPLACE INTO hashes (path, size, mtime_secs, partial_hash, full_hash) VALUES (?1, ?2, ?3, ?4, ?5)")
                .map_err(io_err)?;
            for key in &self.dirty {
                let Some(e) = self.map.get(key) else { continue };
                stmt.execute(params![
                    key,
                    e.size,
                    e.mtime_secs,
                    e.partial_hash.as_slice(),
                    e.full_hash.as_ref().map(|h| h.to_vec()),
                ])
                .map_err(io_err)?;
            }
        }
        tx.commit().map_err(io_err)?;
        self.dirty.clear();
        Ok(())
    }

    pub fn get(&self, path: &Path, size: u64, mtime_secs: u64) -> Option<&CacheEntry> {
        let key = path.to_string_lossy().to_string();
        self.map.get(&key).filter(|e| e.size == size && e.mtime_secs == mtime_secs)
    }

    pub fn insert_partial(&mut self, path: &Path, size: u64, mtime_secs: u64, hash: [u8; 32]) {
        let key = path.to_string_lossy().to_string();
        self.map.insert(
            key.clone(),
            CacheEntry {
                size,
                mtime_secs,
                partial_hash: hash,
                full_hash: None,
            },
        );
        self.dirty.insert(key);
    }

    pub fn set_full(&mut self, path: &Path, size: u64, mtime_secs: u64, full: [u8; 32]) {
        let key = path.to_string_lossy().to_string();
        if let Some(e) = self.map.get_mut(&key) {
            if e.size == size && e.mtime_secs == mtime_secs {
                e.full_hash = Some(full);
                self.dirty.insert(key);
            }
        }
    }
}

/// Read the legacy blob (bincode first, then the older JSON). A file that cannot be read
/// is reported and skipped.
fn read_legacy(path: &Path) -> HashMap<String, CacheEntry> {
    let bin = fs::read(path).ok().and_then(|b| bincode::deserialize(&b).ok());
    if let Some(map) = bin {
        return map;
    }
    let json_path = path.with_extension("json");
    match fs::read_to_string(&json_path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(map) => map,
            Err(e) => {
                eprintln!("legacy hash cache {} unreadable, skipping: {e}", json_path.display());
                HashMap::new()
            }
        },
        Err(_) => HashMap::new(),
    }
}

pub fn entry_mtime_secs(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    Some(mtime_secs(&meta))
}

fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("sortwise"))
}

fn cache_db_path() -> Option<PathBuf> {
    cache_dir().map(|d| d.join("hash_cache.sqlite"))
}

fn cache_legacy_paths() -> Vec<PathBuf> {
    cache_dir().map(|d| vec![d.join("hash_cache.bin")]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sfm_cache_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn entries_survive_reopen() {
        let d = dir("reopen");
        let db = d.join("c.sqlite");
        let file = Path::new("/data/a.jpg");
        {
            let mut c = HashCache::open(&db, vec![]).unwrap();
            c.insert_partial(file, 10, 5, [1; 32]);
            c.set_full(file, 10, 5, [2; 32]);
            c.save().unwrap();
        }
        let c = HashCache::open(&db, vec![]).unwrap();
        let e = c.get(file, 10, 5).expect("entry persisted");
        assert_eq!(e.partial_hash, [1; 32]);
        assert_eq!(e.full_hash, Some([2; 32]));
        assert!(c.get(file, 11, 5).is_none(), "size mismatch must miss");
        assert!(c.get(file, 10, 6).is_none(), "mtime mismatch must miss");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn save_writes_only_changed_rows() {
        let d = dir("dirty");
        let db = d.join("c.sqlite");
        let mut c = HashCache::open(&db, vec![]).unwrap();
        c.insert_partial(Path::new("/x/1"), 1, 1, [1; 32]);
        c.save().unwrap();
        assert!(c.dirty.is_empty(), "save clears the dirty set");
        c.insert_partial(Path::new("/x/2"), 1, 1, [2; 32]);
        assert_eq!(c.dirty.len(), 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn legacy_blob_is_migrated_once() {
        let d = dir("migrate");
        fs::create_dir_all(&d).unwrap();
        let legacy = d.join("hash_cache.bin");
        let mut old: HashMap<String, CacheEntry> = HashMap::new();
        old.insert(
            "/old/file.png".into(),
            CacheEntry { size: 3, mtime_secs: 9, partial_hash: [7; 32], full_hash: None },
        );
        fs::write(&legacy, bincode::serialize(&old).unwrap()).unwrap();

        let db = d.join("c.sqlite");
        let c = HashCache::open(&db, vec![legacy.clone()]).unwrap();
        assert!(c.get(Path::new("/old/file.png"), 3, 9).is_some());
        assert!(legacy.exists(), "legacy file is kept as a fallback");
        drop(c);

        // Second open reads the SQLite rows, not the legacy file again.
        let c2 = HashCache::open(&db, vec![legacy.clone()]).unwrap();
        assert!(c2.get(Path::new("/old/file.png"), 3, 9).is_some());
        let _ = fs::remove_dir_all(&d);
    }
}
