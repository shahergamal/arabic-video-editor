//! Looking at a shot before the automatic edit: which seconds are sharp,
//! well lit and steady, and how much happens in them. Everything runs on
//! this computer from small, quick-to-read pictures.

use crate::media::{probe, FrameReader};
use crate::project::MediaKind;
use crate::time::Fps;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Pictures looked at per second.
const RATE: u32 = 4;
const W: u32 = 96;
const H: u32 = 96;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Moment {
    /// Seconds into the file.
    pub at: f64,
    /// 0 (blurry) to 1 (sharp), compared with the rest of the file.
    pub sharp: f32,
    /// Average brightness, 0 to 1.
    pub bright: f32,
    /// How much changes since the previous picture, 0 to 1.
    pub motion: f32,
    /// How jumpy the camera is (sudden shifts), 0 to 1.
    pub shake: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShotInfo {
    pub path: PathBuf,
    pub kind: MediaKind,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    #[serde(default)]
    pub moments: Vec<Moment>,
    /// What is said in it (filled in by the app when captions are set up).
    #[serde(default)]
    pub speech: String,
}

impl ShotInfo {
    /// How good a moment is to show, 0 to 1.
    pub fn quality(m: &Moment) -> f32 {
        let light = 1.0 - ((m.bright - 0.5).abs() * 2.0).powi(2);
        let steady = 1.0 - m.shake.min(1.0);
        (0.4 * m.sharp + 0.3 * light.max(0.0) + 0.3 * steady).clamp(0.0, 1.0)
    }

    /// The best `length` seconds of the shot: (start, average quality).
    pub fn best_part(&self, length: f64) -> (f64, f32) {
        if self.kind == MediaKind::Image || self.moments.is_empty() || length >= self.duration {
            return (0.0, self.moments.iter().map(Self::quality).sum::<f32>() / self.moments.len().max(1) as f32);
        }
        let span = ((length * RATE as f64).round() as usize).max(1);
        let q: Vec<f32> = self.moments.iter().map(Self::quality).collect();
        let mut best = (0usize, f32::MIN);
        for i in 0..q.len().saturating_sub(span).max(1) {
            let end = (i + span).min(q.len());
            let avg = q[i..end].iter().sum::<f32>() / (end - i) as f32;
            // A little preference for the start: shots are framed to begin with.
            let avg = avg - i as f32 * 0.0005;
            if avg > best.1 {
                best = (i, avg);
            }
        }
        let start = (self.moments[best.0].at).min((self.duration - length).max(0.0));
        (start, best.1)
    }
}

fn luma(f: &crate::Frame) -> Vec<f32> {
    f.data.chunks_exact(4).map(|p| if p[3] == 0 { -1.0 } else { (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0 }).collect()
}

/// Sharpness: how strong the fine detail is (mean absolute Laplacian).
fn detail(y: &[f32]) -> f32 {
    let (w, h) = (W as usize, H as usize);
    let mut sum = 0f32;
    let mut n = 0f32;
    for r in 1..h - 1 {
        for c in 1..w - 1 {
            let i = r * w + c;
            let v = [y[i], y[i - 1], y[i + 1], y[i - w], y[i + w]];
            if v.iter().any(|x| *x < 0.0) {
                continue;
            }
            sum += (4.0 * v[0] - v[1] - v[2] - v[3] - v[4]).abs();
            n += 1.0;
        }
    }
    sum / n.max(1.0)
}

/// Best whole-picture shift between two pictures (within ±4 pixels).
fn shift(a: &[f32], b: &[f32]) -> (f32, f32) {
    let (w, h) = (W as i32, H as i32);
    let mut best = ((0, 0), f32::MAX);
    for dy in -4..=4 {
        for dx in -4..=4 {
            let mut sum = 0f32;
            let mut n = 0f32;
            for r in (8..h - 8).step_by(2) {
                for c in (8..w - 8).step_by(2) {
                    let (p, q) = (a[(r * w + c) as usize], b[((r + dy) * w + c + dx) as usize]);
                    if p < 0.0 || q < 0.0 {
                        continue;
                    }
                    sum += (p - q).abs();
                    n += 1.0;
                }
            }
            let s = sum / n.max(1.0);
            if s < best.1 {
                best = ((dx, dy), s);
            }
        }
    }
    (best.0 .0 as f32, best.0 .1 as f32)
}

pub fn analyze(path: &Path) -> Result<ShotInfo> {
    let info = probe(path)?;
    let kind = crate::interchange::kind_of(path);
    let kind = if kind == MediaKind::Video && info.width == 0 { MediaKind::Audio } else { kind };
    let mut shot = ShotInfo {
        path: path.to_path_buf(),
        kind,
        duration: info.duration,
        width: info.width,
        height: info.height,
        has_audio: info.has_audio,
        moments: vec![],
        speech: String::new(),
    };
    if kind != MediaKind::Video {
        if kind == MediaKind::Image {
            shot.duration = 0.0;
            let f = crate::media::read_still(path, W, H)?;
            let y = luma(&f);
            let bright = y.iter().filter(|v| **v >= 0.0).sum::<f32>() / y.iter().filter(|v| **v >= 0.0).count().max(1) as f32;
            shot.moments.push(Moment { at: 0.0, sharp: 0.7, bright, motion: 0.0, shake: 0.0 });
        }
        return Ok(shot);
    }
    let mut reader = FrameReader::open(path, 0.0, W, H, Fps::new(RATE, 1))?;
    let mut prev: Option<Vec<f32>> = None;
    let mut raw = vec![];
    let mut shifts = vec![];
    let mut k = 0u32;
    while let Some(f) = reader.next_frame()? {
        let y = luma(&f);
        let valid: Vec<f32> = y.iter().cloned().filter(|v| *v >= 0.0).collect();
        let bright = valid.iter().sum::<f32>() / valid.len().max(1) as f32;
        let (motion, sh) = match &prev {
            Some(p) => {
                let diff = p.iter().zip(&y).filter(|(a, b)| **a >= 0.0 && **b >= 0.0).map(|(a, b)| (a - b).abs()).sum::<f32>() / valid.len().max(1) as f32;
                (diff, shift(p, &y))
            }
            None => (0.0, (0.0, 0.0)),
        };
        shifts.push(sh);
        raw.push((k as f64 / RATE as f64, detail(&y), bright, motion));
        prev = Some(y);
        k += 1;
    }
    let top = raw.iter().map(|r| r.1).fold(1e-6f32, f32::max);
    for (i, (at, sharp, bright, motion)) in raw.into_iter().enumerate() {
        // Shake: how far the shift jumps from its neighbours (a pan is smooth, a shake is not).
        let n = shifts.len();
        let (a, b) = (i.saturating_sub(2), (i + 3).min(n));
        let (mx, my) = shifts[a..b].iter().fold((0.0, 0.0), |s, v| (s.0 + v.0, s.1 + v.1));
        let (mx, my) = (mx / (b - a) as f32, my / (b - a) as f32);
        let jump = ((shifts[i].0 - mx).powi(2) + (shifts[i].1 - my).powi(2)).sqrt();
        shot.moments.push(Moment { at, sharp: sharp / top, bright, motion: (motion * 6.0).min(1.0), shake: (jump / 3.0).min(1.0) });
    }
    Ok(shot)
}
