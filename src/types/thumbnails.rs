use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ThumbnailRequest {
    pub path: PathBuf,
    pub is_video: bool,
    pub epoch: usize,
}

#[derive(Debug, Clone)]
pub struct SnapshotPlan {
    pub path: PathBuf,
    pub frames: u32,
    pub requested: bool,
}

pub struct ThumbnailResponse {
    pub path: PathBuf,
    pub epoch: usize,
    pub image: Option<eframe::egui::ColorImage>,
}
