//! Video decoding for the in-app player via **ffmpeg** (video only, muted).
//! Supports pause (pipe backpressure) and seek (restart ffmpeg at an offset),
//! and reports playback position / duration so the UI can show a scrub bar.
use eframe::egui::ColorImage;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Fixed output frame rate; playback position is derived from emitted frames.
const OUTPUT_FPS: f64 = 30.0;
const MAX_EDGE: u32 = 720;

pub struct InlineVideoDecoder {
    latest: Arc<Mutex<Option<ColorImage>>>,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    width: u32,
    height: u32,
    duration: f64,
    start_offset: f64,
    child: std::process::Child,
    join: Option<JoinHandle<()>>,
}

fn resolve_bin(name: &str) -> Option<String> {
    let on_path = Command::new(name)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if on_path {
        return Some(name.to_string());
    }
    for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin", "/usr/bin"] {
        let p = format!("{dir}/{name}");
        if Path::new(&p).exists() {
            return Some(p);
        }
    }
    None
}

fn probe_dims(path: &str) -> (u32, u32) {
    let probe = || -> Option<(u32, u32)> {
        let ffprobe = resolve_bin("ffprobe")?;
        let out = Command::new(ffprobe)
            .args([
                "-v", "error", "-select_streams", "v:0",
                "-show_entries", "stream=width,height",
                "-of", "csv=s=x:p=0", path,
            ])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let (w, h) = text.lines().next()?.trim().split_once('x')?;
        Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
    };
    let (sw, sh) = probe().filter(|(w, h)| *w > 0 && *h > 0).unwrap_or((640, 360));
    let scale = (MAX_EDGE as f32 / sw.max(sh) as f32).min(1.0);
    let mut w = ((sw as f32 * scale).round() as u32).max(2);
    let mut h = ((sh as f32 * scale).round() as u32).max(2);
    w &= !1;
    h &= !1;
    (w.max(2), h.max(2))
}

fn probe_duration(path: &str) -> f64 {
    let probe = || -> Option<f64> {
        let ffprobe = resolve_bin("ffprobe")?;
        let out = Command::new(ffprobe)
            .args([
                "-v", "error", "-show_entries", "format=duration",
                "-of", "default=noprint_wrappers=1:nokey=1", path,
            ])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout).trim().parse().ok()
    };
    probe().unwrap_or(0.0)
}

impl InlineVideoDecoder {
    pub fn try_spawn(path: &Path) -> Result<Self, String> {
        Self::spawn_at(path, 0.0)
    }

    pub fn spawn_at(path: &Path, offset: f64) -> Result<Self, String> {
        let ffmpeg = resolve_bin("ffmpeg").ok_or_else(|| {
            "ffmpeg not found. Install it (e.g. `brew install ffmpeg`) or use “Open in default app”."
                .to_string()
        })?;
        let path_str = path.to_string_lossy().to_string();
        let (out_w, out_h) = probe_dims(&path_str);
        let duration = probe_duration(&path_str);
        let frame_bytes = (out_w * out_h * 3) as usize;
        let vf = format!("scale={out_w}:{out_h}:flags=bilinear");

        let mut cmd = Command::new(&ffmpeg);
        cmd.arg("-hide_banner").arg("-loglevel").arg("error");
        if offset > 0.05 {
            cmd.arg("-ss").arg(format!("{offset:.3}"));
        }
        cmd.args(["-an", "-i", &path_str, "-vf", &vf, "-r", "30", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]);
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Could not start ffmpeg: {e}"))?;

        let stdout = child.stdout.take().ok_or_else(|| "ffmpeg: no stdout".to_string())?;

        let latest = Arc::new(Mutex::new(None::<ColorImage>));
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let frames = Arc::new(AtomicU64::new(0));
        let (l2, s2, p2, f2) = (
            Arc::clone(&latest),
            Arc::clone(&stop),
            Arc::clone(&paused),
            Arc::clone(&frames),
        );

        let join = std::thread::spawn(move || {
            let mut stdout = stdout;
            let mut buf = vec![0u8; frame_bytes];
            loop {
                if s2.load(Ordering::Relaxed) {
                    break;
                }
                if p2.load(Ordering::Relaxed) {
                    // Don't read: ffmpeg blocks on a full pipe → playback pauses.
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    continue;
                }
                match stdout.read_exact(&mut buf) {
                    Ok(()) => {
                        let img = ColorImage::from_rgb([out_w as usize, out_h as usize], &buf);
                        if let Ok(mut g) = l2.lock() {
                            *g = Some(img);
                        }
                        f2.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            latest,
            stop,
            paused,
            frames,
            width: out_w,
            height: out_h,
            duration,
            start_offset: offset,
            child,
            join: Some(join),
        })
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn duration(&self) -> f64 {
        self.duration
    }

    /// Estimated playback position in seconds.
    pub fn position(&self) -> f64 {
        self.start_offset + self.frames.load(Ordering::Relaxed) as f64 / OUTPUT_FPS
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn toggle_pause(&self) {
        self.paused.fetch_xor(true, Ordering::Relaxed);
    }

    /// Take the most recently decoded frame (if any).
    pub fn pop_frame(&self) -> Option<ColorImage> {
        self.latest.lock().ok()?.take()
    }

    #[allow(dead_code)]
    pub fn seek_to(&mut self, path: &Path, offset: f64) -> Result<Self, String> {
        Self::spawn_at(path, offset)
    }
}

impl Drop for InlineVideoDecoder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst); // unblock the reader so it can exit
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
