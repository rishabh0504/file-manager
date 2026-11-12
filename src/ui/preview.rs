//! Read-only file preview: text, hex, image, video hint, reveal / open in system apps.
use eframe::egui::ColorImage;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Command;

const MAX_TEXT_READ: usize = 65_536;
const MAX_HEX_BYTES: usize = 256;
/// In-app image preview (larger than old thumbnail-only cap).
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
/// Max dimension for in-app image preview (scroll to see full image when larger than the viewport).
const PREVIEW_IMAGE_MAX: u32 = 2048;

#[derive(Debug)]
pub enum PreviewKind {
    Empty,
    Text { content: String, truncated: bool },
    Hex(String),
    Image(ColorImage),
    /// No in-app decoder; open with Preview / QuickTime / default app.
    VideoPlaceholder,
    SkippedLarge { size: u64, reason: &'static str },
    IoError(String),
}

pub fn format_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if n < KB {
        format!("{n} B")
    } else if n < MB {
        format!("{:.2} KB", n as f64 / KB as f64)
    } else if n < GB {
        format!("{:.2} MB", n as f64 / MB as f64)
    } else {
        format!("{:.2} GB", n as f64 / GB as f64)
    }
}

pub fn load_preview(path: &Path) -> PreviewKind {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return PreviewKind::IoError(e.to_string()),
    };
    let len = meta.len();
    if len == 0 {
        return PreviewKind::Empty;
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());

    if is_video_ext(ext.as_deref()) {
        return PreviewKind::VideoPlaceholder;
    }

    if is_image_ext(ext.as_deref()) {
        if len > MAX_IMAGE_BYTES {
            return PreviewKind::SkippedLarge {
                size: len,
                reason: "File too large for in-app image preview (limit 32 MiB).",
            };
        }
        match load_image_preview(path) {
            Ok(img) => return PreviewKind::Image(img),
            Err(_) => {
                // Fall through to text/hex attempt for corrupt images
            }
        }
    }

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) => return PreviewKind::IoError(e.to_string()),
    };
    let to_read = (len as usize).min(MAX_TEXT_READ);
    let mut buf = vec![0u8; to_read];
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(e) => return PreviewKind::IoError(e.to_string()),
    };
    buf.truncate(n);

    let truncated = len as usize > MAX_TEXT_READ;
    if is_mostly_utf8_text(&buf) {
        let content = String::from_utf8_lossy(&buf).into_owned();
        return PreviewKind::Text { content, truncated };
    }

    let slice = &buf[..buf.len().min(MAX_HEX_BYTES)];
    PreviewKind::Hex(hex_dump(slice))
}

pub fn is_video_path(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());
    is_video_ext(ext.as_deref())
}

fn is_video_ext(ext: Option<&str>) -> bool {
    matches!(
        ext,
        Some("mp4")
            | Some("mov")
            | Some("m4v")
            | Some("webm")
            | Some("mkv")
            | Some("avi")
            | Some("wmv")
            | Some("mpg")
            | Some("mpeg")
            | Some("flv")
            | Some("ts")
            | Some("mts")
            | Some("m2ts")
            | Some("3gp")
            | Some("3g2")
            | Some("ogv")
            | Some("rm")
            | Some("rmvb")
            | Some("asf")
            | Some("divx")
            | Some("vob")
            | Some("f4v")
    )
}

fn is_image_ext(ext: Option<&str>) -> bool {
    matches!(
        ext,
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp")
    )
}

pub fn is_image_path(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());
    is_image_ext(ext.as_deref())
}

fn load_image_preview(path: &Path) -> Result<ColorImage, Box<dyn std::error::Error + Send + Sync>> {
    let buf = std::fs::read(path)?;
    let img = image::load_from_memory(&buf)?;
    let img = img.thumbnail(PREVIEW_IMAGE_MAX, PREVIEW_IMAGE_MAX);
    let rgba = img.to_rgba8();
    let w = rgba.width() as usize;
    let h = rgba.height() as usize;
    Ok(ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw()))
}

fn is_mostly_utf8_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    let Ok(s) = std::str::from_utf8(bytes) else {
        return false;
    };
    let printable = s
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        .count();
    printable * 10 >= s.len() * 7
}

fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(16).enumerate() {
        out.push_str(&format!("{:08x}  ", i * 16));
        for (j, b) in chunk.iter().enumerate() {
            if j == 8 {
                out.push(' ');
            }
            out.push_str(&format!("{:02x} ", b));
        }
        out.push_str(" |");
        for b in chunk {
            let c = if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            };
            out.push(c);
        }
        out.push_str("|\n");
    }
    out
}

pub fn file_modified_label(path: &Path) -> String {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .unwrap_or_else(|_| "—".into())
}

pub fn reveal_in_folder(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .args(["-R", &path.display().to_string()])
            .spawn();
    }
    #[cfg(windows)]
    {
        let p = path.display().to_string();
        let arg = format!("/select,{}", p);
        let _ = Command::new("explorer").arg(arg).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(parent) = path.parent() {
            let _ = Command::new("xdg-open").arg(parent).spawn();
        }
    }
}

/// Open with the OS default handler (Preview, Photos, QuickTime, etc.).
pub fn open_in_default_app(path: &Path) {
    let p = path.display().to_string();
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(&p).spawn();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("cmd")
            .args(["/C", "start", "", &p])
            .spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(path).spawn();
    }
}

/// Open the macOS Files & Folders privacy pane (R4). Errors are returned, not swallowed.
pub fn open_privacy_settings() -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "privacy settings are macOS only"))
    }
}

/// True when an error string is a macOS privacy denial (R4).
pub fn is_privacy_denied(reason: &str) -> bool {
    reason.contains("Operation not permitted") || reason.contains("Permission denied")
}
