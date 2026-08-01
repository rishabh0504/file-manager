use crate::types::FileEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDisposition {
    HashNormally,
    SkipHeuristic,
}

pub fn classify(entry: &FileEntry, skip_logs_and_locks: bool) -> FileDisposition {
    if !skip_logs_and_locks {
        return FileDisposition::HashNormally;
    }
    if let Some(ext) = entry.path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "log" | "trace" | "lock" | "pid" => return FileDisposition::SkipHeuristic,
            _ => {}
        }
    }
    FileDisposition::HashNormally
}
