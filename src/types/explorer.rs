use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerSortKey {
    #[default]
    Name,
    Size,
    Modified,
    Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerViewMode {
    #[default]
    List,
    Grid,
    Folders,
    Files,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerFilesSort {
    #[default]
    Size,
    Name,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExplorerTypeFilter {
    #[default]
    All,
    Folders,
    Image,
    Video,
    Archive,
}

#[derive(Debug, Clone)]
pub struct ExplorerEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}
