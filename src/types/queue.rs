use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FileQueue {
    pub name: String,
    pub target: PathBuf,
    pub items: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovableItem {
    pub path: PathBuf,
    pub target_queue: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GroupOutcome {
    pub name: String,
    pub target: PathBuf,
    pub items: Vec<PathBuf>,
}
