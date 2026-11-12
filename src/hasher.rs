use crate::cancel::CancellationToken;
use crate::errors::{FileError, FileResult};
use crate::platform::{is_network_path, is_sparse_file};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Shipped behavior: ~25% of file per region, capped at 64 KiB.
/// 3 regions × 64 KiB = 192 KiB total sampled — excellent discrimination
/// for large video/RAW files while staying fast.
pub fn compute_chunk_size(file_size: u64) -> usize {
    const MAX_CHUNK: u64 = 65_536; // 64 KiB per region, up from 8 KiB
    (file_size / 4).clamp(1, MAX_CHUNK) as usize
}

/// Read the 3 partial-hash regions inline on the calling thread with a single
/// open file handle. Rayon already parallelizes across files in the pipeline,
/// so spawning a reader thread pool per file would only add oversubscription
/// and open the file once per region. Output is byte-identical to a sorted
/// (offset, data) BLAKE3 stream, so cached partial hashes stay valid.
pub fn partial_hash_from_reads(
    path: &Path,
    size: u64,
    chunk_size: usize,
    token: &CancellationToken,
) -> FileResult<[u8; 32]> {
    if token.is_cancelled() {
        return Err(FileError::Cancelled);
    }

    let mut regions = partial_regions(size, chunk_size);
    regions.sort_by_key(|&(offset, _)| offset);

    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = Vec::new();

    for &(offset, length) in &regions {
        if token.is_cancelled() {
            return Err(FileError::Cancelled);
        }
        if length == 0 {
            continue;
        }
        if offset > 0 {
            file.seek(SeekFrom::Start(offset))?;
        } else {
            file.rewind()?;
        }
        buf.clear();
        buf.resize(length, 0);
        let mut read_total = 0usize;
        while read_total < length {
            match file.read(&mut buf[read_total..]) {
                Ok(0) => break,
                Ok(n) => read_total += n,
                Err(e) => return Err(FileError::Io(e.to_string())),
            }
        }
        hasher.update(&buf[..read_total]);
    }

    Ok(*hasher.finalize().as_bytes())
}

/// `(offset, length)` of the three sampled regions: start, middle, end.
/// A zero-size file yields one empty region so the hash is still well-defined.
fn partial_regions(size: u64, chunk_size: usize) -> Vec<(u64, usize)> {
    if size == 0 {
        return vec![(0, 0)];
    }
    let chunk = chunk_size as u64;
    let mid = (size / 2).saturating_sub(chunk / 2);
    let end = size.saturating_sub(chunk);
    vec![(0, chunk_size), (mid, chunk_size), (end, chunk_size)]
}

pub fn full_hash(
    path: &Path,
    size: u64,
    token: &CancellationToken,
) -> FileResult<[u8; 32]> {
    if token.is_cancelled() {
        return Err(FileError::Cancelled);
    }

    // Use mmap for files above 4 MB — covers most photo/audio files (JPEG, HEIC, RAW,
    // MP3, FLAC etc.) and avoids the extra user-space copy in BufReader. Falls back to
    // buffered_hash automatically if mmap fails.
    // Removable and network drives read through buffers: a mapped file can fault
    // (SIGBUS) if the drive is pulled mid-read (P-10).
    let use_mmap = size > 4 * 1024 * 1024
        && !is_network_path(path)
        && !crate::platform::is_removable(path)
        && !is_sparse_file(path, size);

    if use_mmap {
        if let Ok(h) = try_mmap_hash(path, token) {
            return Ok(h);
        }
    }

    buffered_hash(path, token)
}

fn try_mmap_hash(path: &Path, token: &CancellationToken) -> FileResult<[u8; 32]> {
    let file = File::open(path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file).map_err(|e| FileError::Io(e.to_string()))? };
    if token.is_cancelled() {
        return Err(FileError::Cancelled);
    }
    Ok(*blake3::hash(&mmap).as_bytes())
}

fn buffered_hash(path: &Path, token: &CancellationToken) -> FileResult<[u8; 32]> {
    let file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut buf = [0u8; 256 * 1024];
    loop {
        if token.is_cancelled() {
            return Err(FileError::Cancelled);
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}
