//! Media-payload matching: find photos and videos whose picture/sound data is byte-identical
//! even though their metadata differs (edited date, location, title, re-tagged by an app).
//!
//! Only the encoded media bytes are compared, never decoded pixels, so a match means the media
//! data is exactly the same:
//! - MP4 / MOV / M4V / 3GP (ISO base media): the contents of every top-level `mdat` box.
//!   Metadata (`moov`, `udta`, `meta`, `free`, …) is skipped.
//! - JPEG: every segment except APP0–APP15 (EXIF, XMP, ICC, maker notes) and COM, plus all
//!   scan data from SOS to the end of the file.
//!
//! Anything that does not parse cleanly is not a candidate, so a broken file never matches.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Payloads smaller than this are ignored: too little data to call two files the same media.
pub const MIN_PAYLOAD: u64 = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaFormat {
    IsoBmff,
    Jpeg,
}

/// The byte ranges of `path` that hold the media data, in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub format: MediaFormat,
    pub ranges: Vec<(u64, u64)>,
}

impl Payload {
    pub fn len(&self) -> u64 {
        self.ranges.iter().map(|(s, e)| e - s).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Which parser applies to this path, by extension.
pub fn format_for(path: &Path) -> Option<MediaFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" | "mov" | "3gp" | "3g2" => Some(MediaFormat::IsoBmff),
        "jpg" | "jpeg" => Some(MediaFormat::Jpeg),
        _ => None,
    }
}

/// Locate the media data without reading it. Cheap: box/segment headers only.
pub fn locate(path: &Path) -> io::Result<Option<Payload>> {
    let Some(format) = format_for(path) else {
        return Ok(None);
    };
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    let ranges = match format {
        MediaFormat::IsoBmff => iso_mdat_ranges(&mut f, len)?,
        MediaFormat::Jpeg => jpeg_payload_ranges(&mut f, len)?,
    };
    let payload = ranges.map(|ranges| Payload { format, ranges });
    Ok(payload.filter(|p| p.len() >= MIN_PAYLOAD))
}

/// BLAKE3 of the media data located by [`locate`].
pub fn hash(path: &Path, payload: &Payload) -> io::Result<[u8; 32]> {
    let mut f = File::open(path)?;
    let mut h = blake3::Hasher::new();
    // Domain-separate the formats so a JPEG and a video can never share a hash.
    h.update(match payload.format {
        MediaFormat::IsoBmff => b"sortwise-media-iso\0",
        MediaFormat::Jpeg => b"sortwise-media-jpg\0",
    });
    let mut buf = vec![0u8; 1 << 20];
    for &(start, end) in &payload.ranges {
        f.seek(SeekFrom::Start(start))?;
        let mut left = end - start;
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            f.read_exact(&mut buf[..n])?;
            h.update(&buf[..n]);
            left -= n as u64;
        }
    }
    Ok(*h.finalize().as_bytes())
}

fn read_exact_at(f: &mut File, pos: u64, buf: &mut [u8]) -> io::Result<bool> {
    f.seek(SeekFrom::Start(pos))?;
    match f.read_exact(buf) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}

/// Top-level `mdat` contents. `None` if the box structure is invalid or has no `mdat`/`moov`.
fn iso_mdat_ranges(f: &mut File, file_len: u64) -> io::Result<Option<Vec<(u64, u64)>>> {
    let mut pos = 0u64;
    let mut ranges = Vec::new();
    let mut saw_moov = false;
    let mut first = true;
    while pos < file_len {
        let mut hdr = [0u8; 8];
        if !read_exact_at(f, pos, &mut hdr)? {
            return Ok(None);
        }
        let mut size = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
        let kind = [hdr[4], hdr[5], hdr[6], hdr[7]];
        if first && &kind != b"ftyp" {
            return Ok(None); // not an ISO base media file
        }
        first = false;
        let mut header_len = 8u64;
        if size == 1 {
            let mut large = [0u8; 8];
            if !read_exact_at(f, pos + 8, &mut large)? {
                return Ok(None);
            }
            size = u64::from_be_bytes(large);
            header_len = 16;
        } else if size == 0 {
            size = file_len - pos; // box runs to end of file
        }
        if size < header_len || pos.checked_add(size).is_none_or(|end| end > file_len) {
            return Ok(None); // truncated or corrupt: never a candidate
        }
        match &kind {
            b"mdat" => ranges.push((pos + header_len, pos + size)),
            b"moov" => saw_moov = true,
            _ => {}
        }
        pos += size;
    }
    Ok((saw_moov && !ranges.is_empty()).then_some(ranges))
}

/// JPEG bytes without APPn/COM segments. `None` if the marker structure is invalid.
fn jpeg_payload_ranges(f: &mut File, file_len: u64) -> io::Result<Option<Vec<(u64, u64)>>> {
    let mut soi = [0u8; 2];
    if !read_exact_at(f, 0, &mut soi)? || soi != [0xFF, 0xD8] {
        return Ok(None);
    }
    let mut ranges = vec![(0u64, 2u64)];
    let mut pos = 2u64;
    loop {
        let mut m = [0u8; 2];
        if !read_exact_at(f, pos, &mut m)? || m[0] != 0xFF {
            return Ok(None);
        }
        let marker = m[1];
        if marker == 0xFF {
            pos += 1; // fill byte
            continue;
        }
        // Standalone markers carry no length.
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            ranges.push((pos, pos + 2));
            pos += 2;
            continue;
        }
        if marker == 0xD9 {
            ranges.push((pos, file_len)); // EOI and anything after it
            break;
        }
        let mut l = [0u8; 2];
        if !read_exact_at(f, pos + 2, &mut l)? {
            return Ok(None);
        }
        let seg_len = u16::from_be_bytes(l) as u64;
        if seg_len < 2 || pos + 2 + seg_len > file_len {
            return Ok(None);
        }
        let end = pos + 2 + seg_len;
        if marker == 0xDA {
            // Start of scan: entropy-coded data follows; keep everything to the end.
            ranges.push((pos, file_len));
            break;
        }
        let is_metadata = (0xE0..=0xEF).contains(&marker) || marker == 0xFE;
        if !is_metadata {
            ranges.push((pos, end));
        }
        pos = end;
    }
    Ok(Some(merge(ranges)))
}

fn merge(mut ranges: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    ranges.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (s, e) in ranges {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }

    fn mp4(meta: &[u8], media: &[u8], moov_first: bool) -> Vec<u8> {
        let mut v = bx(b"ftyp", b"isom\0\0\0\0isomavc1");
        let moov = bx(b"moov", &bx(b"udta", meta));
        let mdat = bx(b"mdat", media);
        if moov_first {
            v.extend(moov);
            v.extend(mdat);
        } else {
            v.extend(mdat);
            v.extend(moov);
        }
        v
    }

    fn jpeg(exif: &[u8], scan: &[u8]) -> Vec<u8> {
        let seg = |m: u8, body: &[u8]| {
            let mut s = vec![0xFF, m];
            s.extend(((body.len() + 2) as u16).to_be_bytes());
            s.extend_from_slice(body);
            s
        };
        let mut v = vec![0xFF, 0xD8];
        v.extend(seg(0xE1, exif));
        v.extend(seg(0xDB, &[7u8; 65]));
        v.extend(seg(0xC0, &[8u8; 15]));
        v.extend(seg(0xDA, &[0u8; 10]));
        v.extend_from_slice(scan);
        v.extend([0xFF, 0xD9]);
        v
    }

    fn write(dir: &Path, name: &str, data: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, data).unwrap();
        p
    }

    fn payload_hash(p: &Path) -> Option<[u8; 32]> {
        let pl = locate(p).unwrap()?;
        Some(hash(p, &pl).unwrap())
    }

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "sfm_media_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn same_video_data_with_different_metadata_matches() {
        let d = tmp();
        let media: Vec<u8> = (0..20_000u32).map(|i| (i * 7) as u8).collect();
        let a = write(&d, "a.mp4", &mp4(b"title=Trip", &media, true));
        let b = write(&d, "b.mov", &mp4(b"title=Trip 2024, edited, location=Goa", &media, false));
        assert_ne!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
        assert_eq!(payload_hash(&a).unwrap(), payload_hash(&b).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn different_video_data_never_matches() {
        let d = tmp();
        let m1: Vec<u8> = (0..20_000u32).map(|i| i as u8).collect();
        let mut m2 = m1.clone();
        m2[10_000] ^= 1; // one bit
        let a = write(&d, "a.mp4", &mp4(b"x", &m1, true));
        let b = write(&d, "b.mp4", &mp4(b"x", &m2, true));
        assert_ne!(payload_hash(&a).unwrap(), payload_hash(&b).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn same_photo_with_different_exif_matches_and_different_pixels_do_not() {
        let d = tmp();
        let scan: Vec<u8> = (0..8_000u32).map(|i| (i % 200) as u8).collect();
        let a = write(&d, "a.jpg", &jpeg(b"Exif\0\0date=2020", &scan));
        let b = write(&d, "b.JPG", &jpeg(b"Exif\0\0date=2024 gps=15.2,73.9 edited", &scan));
        assert_eq!(payload_hash(&a).unwrap(), payload_hash(&b).unwrap());
        let mut other = scan.clone();
        other[5] = 255;
        let c = write(&d, "c.jpg", &jpeg(b"Exif\0\0date=2020", &other));
        assert_ne!(payload_hash(&a).unwrap(), payload_hash(&c).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn truncated_or_foreign_files_are_never_candidates() {
        let d = tmp();
        let media = vec![1u8; 20_000];
        let full = mp4(b"x", &media, true);
        let cut = write(&d, "cut.mp4", &full[..full.len() - 100]);
        assert!(locate(&cut).unwrap().is_none());
        let not_mp4 = write(&d, "fake.mp4", &[0u8; 30_000]);
        assert!(locate(&not_mp4).unwrap().is_none());
        let not_jpeg = write(&d, "fake.jpg", &[0x89u8; 30_000]);
        assert!(locate(&not_jpeg).unwrap().is_none());
        let other_ext = write(&d, "a.mkv", &full);
        assert!(locate(&other_ext).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
