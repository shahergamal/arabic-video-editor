//! Finding the beat of a piece of music, so cuts can land on it.
//!
//! 1. The sound is cut into short overlapping windows; for each, how much
//!    the spectrum got louder since the last one ("spectral flux"). Drums and
//!    plucked notes make sharp rises.
//! 2. The tempo is the spacing that repeats most in that curve
//!    (autocorrelation), preferring the usual 70 to 180 beats a minute.
//! 3. The beats are the rises that best fit that steady spacing (dynamic
//!    programming, after D. Ellis, "Beat tracking by dynamic programming").

use crate::media::{command, ffmpeg_path};
use crate::{Error, Result};
use std::path::Path;

const RATE: u32 = 22050;
const WINDOW: usize = 1024;
const HOP: usize = 256;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Beats {
    /// Beats a minute.
    pub tempo: f64,
    /// Where each beat falls, in seconds from the start of the file.
    pub times: Vec<f64>,
}

fn pcm(path: &Path, start: f64, length: f64) -> Result<Vec<f32>> {
    let out = command(ffmpeg_path())
        .args(["-v", "error", "-nostdin", "-ss", &format!("{start:.6}"), "-t", &format!("{length:.6}"), "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", &RATE.to_string(), "-f", "f32le", "-"])
        .output()
        .map_err(Error::FfmpegMissing)?;
    if !out.status.success() {
        return Err(Error::Ffmpeg(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    Ok(out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

/// How strongly something starts in each window (one value per `HOP` samples).
pub fn onset_strength(samples: &[f32]) -> Vec<f32> {
    use rustfft::{num_complex::Complex, FftPlanner};
    if samples.len() < WINDOW {
        return vec![];
    }
    let fft = FftPlanner::<f32>::new().plan_fft_forward(WINDOW);
    let hann: Vec<f32> = (0..WINDOW).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WINDOW as f32).cos()).collect();
    let mut prev = vec![0f32; WINDOW / 2];
    let mut out = vec![];
    let mut buf = vec![Complex::new(0f32, 0f32); WINDOW];
    let mut pos = 0;
    while pos + WINDOW <= samples.len() {
        for i in 0..WINDOW {
            buf[i] = Complex::new(samples[pos + i] * hann[i], 0.0);
        }
        fft.process(&mut buf);
        let mut flux = 0f32;
        for k in 0..WINDOW / 2 {
            let m = (1.0 + 1000.0 * buf[k].norm()).ln();
            flux += (m - prev[k]).max(0.0);
            prev[k] = m;
        }
        out.push(flux);
        pos += HOP;
    }
    if let Some(first) = out.first_mut() {
        *first = 0.0;
    }
    // Take away the slowly moving average so only the sharp rises remain.
    let n = out.len();
    let reach = 8;
    let mut clean = vec![0f32; n];
    for i in 0..n {
        let (a, b) = (i.saturating_sub(reach), (i + reach + 1).min(n));
        let mean = out[a..b].iter().sum::<f32>() / (b - a) as f32;
        clean[i] = (out[i] - mean).max(0.0);
    }
    let sd = (clean.iter().map(|v| v * v).sum::<f32>() / n.max(1) as f32).sqrt().max(1e-9);
    clean.iter().map(|v| v / sd).collect()
}

/// The beat spacing in windows.
fn period(onset: &[f32], per_second: f64) -> f64 {
    let lag = |bpm: f64| per_second * 60.0 / bpm;
    let (lo, hi) = (lag(220.0).floor() as usize, (lag(50.0).ceil() as usize).min(onset.len().saturating_sub(1)));
    let mut best = (0usize, f64::MIN);
    for l in lo.max(1)..=hi.max(lo + 1) {
        if l >= onset.len() {
            break;
        }
        let ac: f64 = onset.iter().zip(&onset[l..]).map(|(a, b)| (*a * *b) as f64).sum::<f64>() / (onset.len() - l) as f64;
        // Lean towards about 120 a minute, the way people tap along.
        let bpm = per_second * 60.0 / l as f64;
        let weight = (-0.5 * ((bpm / 120.0).log2() / 0.9).powi(2)).exp();
        if ac * weight > best.1 {
            best = (l, ac * weight);
        }
    }
    // Refine between neighbours (parabola through the peak).
    let l = best.0.max(1);
    let at = |l: usize| -> f64 {
        if l == 0 || l >= onset.len() {
            return 0.0;
        }
        onset.iter().zip(&onset[l..]).map(|(a, b)| (*a * *b) as f64).sum::<f64>() / (onset.len() - l) as f64
    };
    let (a, b, c) = (at(l - 1), at(l), at(l + 1));
    let d = a - 2.0 * b + c;
    if d.abs() > 1e-12 {
        l as f64 + (0.5 * (a - c) / d).clamp(-0.5, 0.5)
    } else {
        l as f64
    }
}

/// Picks the beats: each beat is a strong rise about one period after the
/// one before.
fn track(onset: &[f32], period: f64) -> Vec<usize> {
    let n = onset.len();
    if n == 0 || period < 1.0 {
        return vec![];
    }
    let tightness = 100.0f64;
    let mut score = vec![0f64; n];
    let mut back = vec![usize::MAX; n];
    for i in 0..n {
        let (lo, hi) = ((i as f64 - 2.0 * period).round(), (i as f64 - period / 2.0).round());
        let mut best = (0.0f64, usize::MAX);
        if hi >= 0.0 {
            for j in (lo.max(0.0) as usize)..=(hi as usize).min(i.saturating_sub(1)) {
                let gap = (i - j) as f64;
                let s = score[j] - tightness * (gap / period).ln().powi(2);
                if best.1 == usize::MAX || s > best.0 {
                    best = (s, j);
                }
            }
        }
        score[i] = onset[i] as f64 + if best.1 == usize::MAX { 0.0 } else { best.0 };
        back[i] = best.1;
    }
    // Start from the best-scoring beat in the last period.
    let tail = ((n as f64 - period).max(0.0)) as usize;
    let mut i = (tail..n).max_by(|a, b| score[*a].total_cmp(&score[*b])).unwrap_or(n - 1);
    let mut beats = vec![i];
    while back[i] != usize::MAX {
        i = back[i];
        beats.push(i);
    }
    beats.reverse();
    beats
}

/// Beats in `length` seconds of `path`, from `start` seconds.
pub fn detect(path: &Path, start: f64, length: f64) -> Result<Beats> {
    let samples = pcm(path, start, length)?;
    Ok(detect_samples(&samples, RATE, start))
}

pub fn detect_samples(samples: &[f32], rate: u32, offset: f64) -> Beats {
    let onset = onset_strength(samples);
    let per_second = rate as f64 / HOP as f64;
    if onset.len() < 8 {
        return Beats { tempo: 0.0, times: vec![] };
    }
    let p = period(&onset, per_second);
    let beats = track(&onset, p);
    // The middle of the window is where the sound actually started.
    let shift = (WINDOW as f64 / 2.0) / rate as f64;
    let times = beats.iter().map(|b| offset + *b as f64 / per_second + shift - HOP as f64 / rate as f64 / 2.0).collect();
    Beats { tempo: 60.0 * per_second / p, times }
}
