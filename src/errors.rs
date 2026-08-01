use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    Io(String),
    NotFound(PathBuf),
    TooLarge(u64),
    Cancelled,
    VerificationFailed,
    CrossVolumeHardLink,
    UnsupportedPath(String),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::Io(e) => write!(f, "I/O error: {e}"),
            FileError::NotFound(p) => write!(f, "Not found: {}", p.display()),
            FileError::TooLarge(n) => write!(f, "File too large: {n} bytes"),
            FileError::Cancelled => write!(f, "Cancelled"),
            FileError::VerificationFailed => write!(f, "Verification failed"),
            FileError::CrossVolumeHardLink => write!(f, "Cross-volume hard link"),
            FileError::UnsupportedPath(s) => write!(f, "Unsupported path: {s}"),
        }
    }
}

impl std::error::Error for FileError {}

impl From<std::io::Error> for FileError {
    fn from(e: std::io::Error) -> Self {
        FileError::Io(e.to_string())
    }
}

pub type FileResult<T> = Result<T, FileError>;
