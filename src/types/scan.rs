#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScanWorkKind {
    #[default]
    FullScan,
    RefreshFindings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DupGroupSortOrder {
    #[default]
    ScanOrder,
    WastedBytesDesc,
    WastedBytesAsc,
    FileCountDesc,
    FileCountAsc,
    HashAsc,
}
