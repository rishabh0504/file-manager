//! Synthetic dataset generator for scan benchmarks (Implementation-Plan Phase 0).
//!
//! Usage:
//!   cargo run --release --example gen_dataset -- <out_dir> <files> <dup_ratio> <max_kb> [seed]
//!
//! - `dup_ratio` (0.0..1.0): fraction of files that are byte-identical copies of an earlier file.
//! - `max_kb`: upper bound for generated file size (sizes are 4 KB .. max_kb).
//! - Writes `<out_dir>/manifest.tsv` (path, content_id, size) and prints the expected
//!   number of duplicate groups so scan results can be checked.
//!
//! Content is deterministic per content id, so duplicates are real byte copies.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

const MIN_SIZE: u64 = 4 * 1024;
const EXTENSIONS: [&str; 6] = ["jpg", "png", "cr2", "mp4", "zip", "mp3"];

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }
}

/// Size and extension are pure functions of the content id, so every copy agrees.
fn content_shape(content_id: u64, seed: u64, max_kb: u64) -> (u64, &'static str) {
    let mut r = Rng::new(seed ^ content_id.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let span = (max_kb * 1024).saturating_sub(MIN_SIZE).max(1);
    let size = MIN_SIZE + r.below(span);
    let ext = EXTENSIONS[r.below(EXTENSIONS.len() as u64) as usize];
    (size, ext)
}

fn write_content(path: &Path, content_id: u64, seed: u64, size: u64) -> std::io::Result<()> {
    let mut r = Rng::new(seed ^ content_id.wrapping_mul(0xD1B5_4A32_D192_ED03) ^ 0xABCD);
    let mut w = BufWriter::with_capacity(1 << 20, File::create(path)?);
    let mut remaining = size;
    let mut buf = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let n = remaining.min(buf.len() as u64) as usize;
        for chunk in buf[..n].chunks_mut(8) {
            let v = r.next_u64().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
        w.write_all(&buf[..n])?;
        remaining -= n as u64;
    }
    w.flush()
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        eprintln!("usage: gen_dataset <out_dir> <files> <dup_ratio> <max_kb> [seed]");
        std::process::exit(2);
    }
    let out = PathBuf::from(&args[0]);
    let files: u64 = args[1].parse().expect("files must be an integer");
    let dup_ratio: f64 = args[2].parse().expect("dup_ratio must be a float");
    let max_kb: u64 = args[3].parse().expect("max_kb must be an integer");
    let seed: u64 = args.get(4).map(|s| s.parse().expect("seed")).unwrap_or(42);

    fs::create_dir_all(&out)?;
    let mut pick = Rng::new(seed);
    let mut originals: Vec<u64> = Vec::new();
    let mut copies: HashMap<u64, u64> = HashMap::new();
    let mut manifest = BufWriter::new(File::create(out.join("manifest.tsv"))?);
    writeln!(manifest, "path\tcontent_id\tsize")?;

    for i in 0..files {
        let content_id = if pick.unit() < dup_ratio && !originals.is_empty() {
            originals[pick.below(originals.len() as u64) as usize]
        } else {
            originals.push(i + 1);
            i + 1
        };
        let (size, ext) = content_shape(content_id, seed, max_kb);
        let dir = out.join(format!("d{:05}", i / 1000));
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("f{i:08}.{ext}"));
        write_content(&path, content_id, seed, size)?;
        writeln!(manifest, "{}\t{}\t{}", path.display(), content_id, size)?;
        *copies.entry(content_id).or_default() += 1;
    }
    manifest.flush()?;

    let expected_groups = copies.values().filter(|&&c| c >= 2).count();
    let expected_wasted: u64 = copies
        .iter()
        .filter(|(_, &c)| c >= 2)
        .map(|(&id, &c)| (c - 1) * content_shape(id, seed, max_kb).0)
        .sum();
    println!("files={files} originals={} expected_groups={expected_groups} expected_wasted_bytes={expected_wasted}", originals.len());
    Ok(())
}
