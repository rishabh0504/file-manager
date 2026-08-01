use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId {
    pub volume_id: u64,
    pub file_index: u64,
}

pub fn file_id(path: &Path) -> std::io::Result<FileId> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path)?;
        Ok(FileId {
            volume_id: meta.dev(),
            file_index: meta.ino(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use winapi::um::fileapi::GetFileInformationByHandle;
        use winapi::um::winnt::BY_HANDLE_FILE_INFORMATION;

        let file = std::fs::File::open(path)?;
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(FileId {
            volume_id: info.dwVolumeSerialNumber as u64,
            file_index: ((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64),
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "file_id unsupported on this platform",
        ))
    }
}

pub fn is_network_path(path: &Path) -> bool {
    #[cfg(windows)]
    {
        path.to_str()
            .map(|s| s.starts_with("\\\\") || s.starts_with("//"))
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        let _ = path;
        false
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        false
    }
}

#[cfg(unix)]
pub fn is_sparse_file(path: &Path, apparent_size: u64) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    (meta.blocks() * 512) < apparent_size
}

#[cfg(not(unix))]
pub fn is_sparse_file(_path: &Path, _apparent_size: u64) -> bool {
    false
}
