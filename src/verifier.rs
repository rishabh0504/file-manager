use crate::errors::FileResult;
use crate::types::FileEntry;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

pub fn verify_identical(a: &Path, b: &Path) -> FileResult<bool> {
    let fa = File::open(a)?;
    let fb = File::open(b)?;

    let mut ra = BufReader::with_capacity(64 * 1024, fa);
    let mut rb = BufReader::with_capacity(64 * 1024, fb);

    let mut buf_a = [0u8; 64 * 1024];
    let mut buf_b = [0u8; 64 * 1024];

    loop {
        let na = ra.read(&mut buf_a)?;
        let nb = rb.read(&mut buf_b)?;
        if na != nb || buf_a[..na] != buf_b[..nb] {
            return Ok(false);
        }
        if na == 0 {
            break;
        }
    }
    Ok(true)
}

/// O(N) verification: every file compared to the canonical first file only.
pub fn verify_group(files: &[FileEntry]) -> Vec<FileEntry> {
    if files.len() < 2 {
        return files.to_vec();
    }
    let canonical = &files[0];
    let mut confirmed = vec![canonical.clone()];

    for candidate in &files[1..] {
        match verify_identical(&canonical.path, &candidate.path) {
            Ok(true) => confirmed.push(candidate.clone()),
            Ok(false) => {}
            Err(_) => {}
        }
    }

    confirmed
}
