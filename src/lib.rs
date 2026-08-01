pub mod cache;
pub mod cancel;
pub mod cdc;
pub mod errors;
pub mod excludes;
pub mod hasher;
pub mod heuristics;
pub mod io_scheduler;
pub mod pipeline;
pub mod platform;
pub mod resolver;
pub mod scanner;
pub mod similarity;
pub mod types;
pub mod verifier;

pub use pipeline::{refresh_duplicates_from_result, run_dedup, summarize_folders};
pub use types::ScanConfig;
