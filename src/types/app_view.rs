#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppView {
    Scan,
    Scanning,
    Results,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    Explorer,
    Dedupe,
    CompareSourceDest,
    EmptyFolders,
    ArchiveFiles,
}
