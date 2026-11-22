use super::scan::DupGroupSortOrder;

#[derive(Debug, Clone)]
pub struct ExtractBanner {
    pub zip: std::path::PathBuf,
    pub dest: std::path::PathBuf,
    pub dir: std::path::PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct DupListCache {
    pub key: Option<(u64, String, DupGroupSortOrder, Option<crate::heuristics::MediaKind>)>,
    pub visible: Vec<usize>,
    pub max_wasted: u64,
}
