//! File movement operations: atomic rename, cross-volume copy, collision handling.

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn fast_move(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.exists() || dst.symlink_metadata().is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("Destination already exists: {}", dst.display()),
        ));
    }

    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let clone = std::process::Command::new("cp")
            .args(["-c", "--"])
            .arg(src)
            .arg(dst)
            .status();
        if clone.map(|s| s.success()).unwrap_or(false) {
            std::fs::remove_file(src)?;
            return Ok(());
        }
    }

    {
        use std::io::{BufReader, BufWriter, Read, Write};
        const BUF: usize = 4 * 1024 * 1024;
        let mut r = BufReader::with_capacity(BUF, std::fs::File::open(src)?);
        let mut w = BufWriter::with_capacity(BUF, std::fs::File::create(dst)?);
        let mut buf = vec![0u8; BUF];
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 { break; }
            w.write_all(&buf[..n])?;
        }
        w.flush()?;
    }
    std::fs::remove_file(src)?;
    Ok(())
}

pub fn fast_move_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.exists() || dst.symlink_metadata().is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("Destination directory already exists: {}", dst.display()),
        ));
    }

    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("cp")
            .args(["-R", "--"])
            .arg(src)
            .arg(dst)
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            std::fs::remove_dir_all(src)?;
            return Ok(());
        }
        let _ = std::fs::remove_dir_all(dst);
    }

    copy_dir_recursive(src, dst)?;
    std::fs::remove_dir_all(src)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in WalkDir::new(src).follow_links(false) {
        let entry = entry?;
        let rel = match entry.path().strip_prefix(src) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest_path = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

pub fn unique_dir_dest(target_dir: &Path, folder_name: &str) -> PathBuf {
    let mut dest = target_dir.join(folder_name);
    let mut counter = 2u32;
    while dest.exists() || dest.symlink_metadata().is_ok() {
        dest = target_dir.join(format!("{folder_name}_{counter}"));
        counter += 1;
    }
    dest
}

pub fn unique_file_dest(target_dir: &Path, file_path: &Path) -> PathBuf {
    let stem = file_path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let original_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("file");

    let mut dest = target_dir.join(original_name);
    let mut counter = 1u32;
    while dest.exists() || dest.symlink_metadata().is_ok() {
        let new_name = if ext.is_empty() {
            format!("{stem}_{counter}")
        } else {
            format!("{stem}_{counter}.{ext}")
        };
        dest = target_dir.join(&new_name);
        counter += 1;
    }
    dest
}
