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
        use winapi::um::fileapi::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};

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

/// What the OS reports about the drive behind a path (Implementation-Plan P-8, P-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeInfo {
    /// `Some(true)` solid state, `Some(false)` spinning disk, `None` unknown.
    pub solid_state: Option<bool>,
    /// Removable or external media (USB, SD card). Memory-mapping these risks SIGBUS on unplug.
    pub removable: bool,
}

impl VolumeInfo {
    /// Unknown drives get the safe HDD behaviour.
    pub const UNKNOWN: VolumeInfo = VolumeInfo { solid_state: None, removable: false };
}

/// Volume info for `path`. Mount points come from one `mount` call and each mount is
/// probed once; per-file lookups are a prefix match in memory.
pub fn volume_info(path: &Path) -> VolumeInfo {
    #[cfg(target_os = "macos")]
    {
        macos::volume_info(path)
    }
    #[cfg(windows)]
    {
        windows::volume_info(path)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = path;
        VolumeInfo::UNKNOWN
    }
}

/// True when the path is on removable media (see `VolumeInfo::removable`).
pub fn is_removable(path: &Path) -> bool {
    volume_info(path).removable
}

#[cfg(target_os = "macos")]
mod macos {
    use super::VolumeInfo;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::{Mutex, OnceLock};

    struct Mounts {
        points: Vec<PathBuf>,
        info: Mutex<HashMap<PathBuf, VolumeInfo>>,
    }

    fn mounts() -> &'static Mounts {
        static MOUNTS: OnceLock<Mounts> = OnceLock::new();
        MOUNTS.get_or_init(|| Mounts {
            points: list_mount_points(),
            info: Mutex::new(HashMap::new()),
        })
    }

    /// Parse `mount` lines: `<device> on <mount point> (<options>)`.
    fn list_mount_points() -> Vec<PathBuf> {
        let Ok(out) = Command::new("mount").output() else {
            return Vec::new();
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let after = line.split_once(" on ")?.1;
                let end = after.find(" (").unwrap_or(after.len());
                Some(PathBuf::from(&after[..end]))
            })
            .collect()
    }

    pub fn volume_info(path: &Path) -> VolumeInfo {
        let m = mounts();
        // Longest mount point that prefixes the path wins (e.g. /Volumes/Backup over /).
        let Some(mount) = m
            .points
            .iter()
            .filter(|mp| path.starts_with(mp))
            .max_by_key(|mp| mp.as_os_str().len())
        else {
            return VolumeInfo::UNKNOWN;
        };
        if let Some(&info) = m.info.lock().unwrap().get(mount) {
            return info;
        }
        let info = probe(mount);
        m.info.lock().unwrap().insert(mount.clone(), info);
        info
    }

    fn probe(mount: &Path) -> VolumeInfo {
        let Ok(out) = Command::new("diskutil").arg("info").arg("-plist").arg(mount).output() else {
            return VolumeInfo::UNKNOWN;
        };
        let xml = String::from_utf8_lossy(&out.stdout);
        let removable = plist_bool(&xml, "Removable").unwrap_or(false)
            || plist_bool(&xml, "RemovableMedia").unwrap_or(false)
            || plist_bool(&xml, "Ejectable").unwrap_or(false)
            || plist_bool(&xml, "Internal") == Some(false);
        VolumeInfo {
            solid_state: plist_bool(&xml, "SolidState"),
            removable,
        }
    }

    /// Read `<key>NAME</key><true/>` or `<false/>` from diskutil's XML plist output.
    pub(super) fn plist_bool(xml: &str, key: &str) -> Option<bool> {
        let needle = format!("<key>{key}</key>");
        let after = &xml[xml.find(&needle)? + needle.len()..];
        let value = after.trim_start();
        if value.starts_with("<true/>") {
            Some(true)
        } else if value.starts_with("<false/>") {
            Some(false)
        } else {
            None
        }
    }

    #[cfg(test)]
    mod tests {
        use super::plist_bool;

        #[test]
        fn parses_boolean_keys() {
            let xml = "<dict><key>SolidState</key>\n\t<true/>\n<key>Removable</key>\n\t<false/></dict>";
            assert_eq!(plist_bool(xml, "SolidState"), Some(true));
            assert_eq!(plist_bool(xml, "Removable"), Some(false));
            assert_eq!(plist_bool(xml, "Missing"), None);
        }
    }
}

/// Windows drive lookup: `GetDriveTypeW` for removable media, and a zero-access
/// `IOCTL_STORAGE_QUERY_PROPERTY` on the volume for bus type and seek penalty (HDD vs SSD).
/// Zero access needs no admin rights. Any failure falls back to `None` (safe HDD path).
#[cfg(windows)]
mod windows {
    use super::VolumeInfo;
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Path, Prefix};
    use std::ptr::null_mut;
    use std::sync::{Mutex, OnceLock};
    use winapi::shared::minwindef::{DWORD, LPVOID};
    use winapi::shared::ntdef::HANDLE;
    use winapi::um::fileapi::{CreateFileW, GetDriveTypeW, OPEN_EXISTING};
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::ioapiset::DeviceIoControl;
    use winapi::um::winbase::{DRIVE_CDROM, DRIVE_REMOVABLE};
    use winapi::um::winioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
        StorageDeviceProperty, StorageDeviceSeekPenaltyProperty,
    };
    use winapi::um::winnt::{FILE_SHARE_READ, FILE_SHARE_WRITE};

    // STORAGE_BUS_TYPE values (winapi 0.3 does not expose these structs).
    const BUS_TYPE_USB: u32 = 7;
    const BUS_TYPE_SD: u32 = 12;
    const BUS_TYPE_MMC: u32 = 13;

    /// Fixed header of STORAGE_DEVICE_DESCRIPTOR. Field offsets match the Windows SDK layout.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct StorageDeviceDescriptor {
        version: u32,
        size: u32,
        device_type: u8,
        device_type_modifier: u8,
        removable_media: u8,
        command_queueing: u8,
        vendor_id_offset: u32,
        product_id_offset: u32,
        product_revision_offset: u32,
        serial_number_offset: u32,
        bus_type: u32,
        raw_properties_length: u32,
    }

    /// DEVICE_SEEK_PENALTY_DESCRIPTOR.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct DeviceSeekPenalty {
        version: u32,
        size: u32,
        incurs_seek_penalty: u8,
    }

    // Layout checks against the Windows SDK definitions: a wrong layout fails the build.
    const _: () = assert!(size_of::<StorageDeviceDescriptor>() == 36);
    const _: () = assert!(std::mem::offset_of!(StorageDeviceDescriptor, bus_type) == 28);
    const _: () = assert!(size_of::<DeviceSeekPenalty>() == 12);
    const _: () = assert!(std::mem::offset_of!(DeviceSeekPenalty, incurs_seek_penalty) == 8);

    static CACHE: OnceLock<Mutex<HashMap<u8, VolumeInfo>>> = OnceLock::new();

    pub fn volume_info(path: &Path) -> VolumeInfo {
        // UNC and device paths have no drive letter: treat as unknown (network paths are handled elsewhere).
        let Some(Component::Prefix(prefix)) = path.components().next() else {
            return VolumeInfo::UNKNOWN;
        };
        let letter = match prefix.kind() {
            Prefix::Disk(l) | Prefix::VerbatimDisk(l) => l.to_ascii_uppercase(),
            _ => return VolumeInfo::UNKNOWN,
        };
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(&info) = cache.lock().unwrap().get(&letter) {
            return info;
        }
        let info = probe(letter);
        cache.lock().unwrap().insert(letter, info);
        info
    }

    fn probe(letter: u8) -> VolumeInfo {
        let root = wide(&format!("{}:\\", letter as char));
        let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
        let optical_or_removable = drive_type == DRIVE_REMOVABLE || drive_type == DRIVE_CDROM;

        let Some(handle) = open_volume(letter) else {
            return VolumeInfo { solid_state: None, removable: optical_or_removable };
        };
        let desc: Option<StorageDeviceDescriptor> = query(handle, StorageDeviceProperty);
        let seek: Option<DeviceSeekPenalty> = query(handle, StorageDeviceSeekPenaltyProperty);
        unsafe { CloseHandle(handle) };

        let bus_removable = desc.is_some_and(|d| {
            d.removable_media != 0 || matches!(d.bus_type, BUS_TYPE_USB | BUS_TYPE_SD | BUS_TYPE_MMC)
        });
        VolumeInfo {
            solid_state: seek.map(|s| s.incurs_seek_penalty == 0),
            removable: optical_or_removable || bus_removable,
        }
    }

    fn open_volume(letter: u8) -> Option<HANDLE> {
        let name = wide(&format!("\\\\.\\{}:", letter as char));
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null_mut(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        (handle != INVALID_HANDLE_VALUE).then_some(handle)
    }

    fn query<T: Copy>(handle: HANDLE, id: u32) -> Option<T> {
        let mut request = STORAGE_PROPERTY_QUERY {
            PropertyId: id,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0],
        };
        let mut out: T = unsafe { std::mem::zeroed() };
        let mut returned: DWORD = 0;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                &mut request as *mut _ as LPVOID,
                size_of::<STORAGE_PROPERTY_QUERY>() as DWORD,
                &mut out as *mut _ as LPVOID,
                size_of::<T>() as DWORD,
                &mut returned,
                null_mut(),
            )
        };
        (ok != 0 && returned as usize >= size_of::<T>()).then_some(out)
    }

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }
}
