//! Lining up several cameras that filmed the same moment, by their sound.
//!
//! Each file's sound is turned into a loudness curve (how loud, 1000 times a
//! second). The curves are compared at every possible shift: claps, words
//! and door slams line up where the cameras agree. A coarse pass at 100 per
//! second finds the area, a fine pass at 1000 per second the exact spot.

use crate::media::{command, ffmpeg_path};
use crate::{Error, Result};
use std::path::Path;

const RATE: u32 = 8000;
/// How much of each file to listen to, in seconds.
const LISTEN: f64 = 300.0;

fn pcm(path: &Path) -> Result<Vec<f32>> {
    let out = command(ffmpeg_path())
        .args(["-v", "error", "-nostdin", "-t", &LISTEN.to_string(), "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", &RATE.to_string(), "-f", "f32le", "-"])
        .output()
        .map_err(Error::FfmpegMissing)?;
    if !out.status.success() {
        return Err(Error::Ffmpeg(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    Ok(out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

/// Loudness curve with `per_second` points; rises are what get matched,
/// so steady hum and different microphone levels matter little.
fn onsets(samples: &[f32], per_second: u32) -> Vec<f32> {
    let win = (RATE / per_second) as usize;
    let env: Vec<f32> = samples.chunks(win).map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt()).collect();
    let peak = env.iter().cloned().fold(1e-9f32, f32::max);
    let log: Vec<f32> = env.iter().map(|e| (e / peak + 1e-4).ln()).collect();
    let mut d: Vec<f32> = log.windows(2).map(|w| (w[1] - w[0]).max(0.0)).collect();
    d.insert(0, 0.0);
    let mean = d.iter().sum::<f32>() / d.len().max(1) as f32;
    let norm = d.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>().sqrt().max(1e-9);
    d.iter().map(|v| (v - mean) / norm).collect()
}

/// c[k] = sum of a[i + k] * b[i], for every shift k (negative shifts wrap
/// to the end).
fn correlate(a: &[f32], b: &[f32]) -> Vec<f32> {
    use rustfft::{num_complex::Complex, FftPlanner};
    let n = (a.len() + b.len()).next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let (fwd, inv) = (planner.plan_fft_forward(n), planner.plan_fft_inverse(n));
    let pad = |v: &[f32]| {
        let mut x: Vec<Complex<f32>> = v.iter().map(|r| Complex::new(*r, 0.0)).collect();
        x.resize(n, Complex::new(0.0, 0.0));
        x
    };
    let (mut fa, mut fb) = (pad(a), pad(b));
    fwd.process(&mut fa);
    fwd.process(&mut fb);
    let mut prod: Vec<Complex<f32>> = fa.iter().zip(&fb).map(|(x, y)| x * y.conj()).collect();
    inv.process(&mut prod);
    prod.iter().map(|c| c.re / n as f32).collect()
}

/// Similarity of `a` and `b` when `b` is shifted by `lag` points
/// (b[i] lines up with a[i + lag]).
fn score(a: &[f32], b: &[f32], lag: i64) -> f32 {
    let (mut s, mut n) = (0f32, 0usize);
    for (i, bv) in b.iter().enumerate() {
        let j = i as i64 + lag;
        if j >= 0 && (j as usize) < a.len() {
            s += a[j as usize] * bv;
            n += 1;
        }
    }
    if n < 50 {
        0.0
    } else {
        s
    }
}

/// How the second recording sits against the first: `b`'s time 0 is at
/// `offset` seconds of `a` (negative: `b` started earlier). Also returns a
/// confidence from 0 to 1; below about 0.3 the match is doubtful.
pub fn audio_offset(a: &Path, b: &Path) -> Result<(f64, f32)> {
    let (sa, sb) = (pcm(a)?, pcm(b)?);
    if sa.is_empty() || sb.is_empty() {
        return Err(Error::Ffmpeg("one of the files has no sound".into()));
    }
    // Coarse: 100 points a second, every shift at once (FFT correlation).
    let (ca, cb) = (onsets(&sa, 100), onsets(&sb, 100));
    let corr = correlate(&ca, &cb);
    let n = corr.len() as i64;
    let lag_of = |k: usize| if (k as i64) < n - cb.len() as i64 { k as i64 } else { k as i64 - n };
    let mut order: Vec<usize> = (0..corr.len()).collect();
    order.sort_by(|x, y| corr[*y].total_cmp(&corr[*x]));
    let best = (lag_of(order[0]), corr[order[0]]);
    // Confidence: how far the best stands above the next shift that is not
    // right next to it.
    let second = order.iter().map(|k| (lag_of(*k), corr[*k])).find(|(l, _)| (l - best.0).abs() > 10).map(|s| s.1).unwrap_or(0.0);
    let confidence = if best.1 > 0.0 { ((best.1 - second.max(0.0)) / best.1).clamp(0.0, 1.0) } else { 0.0 };
    // Fine: 1000 points a second, within 20 ms of the coarse answer.
    let (fa, fb) = (onsets(&sa, 1000), onsets(&sb, 1000));
    let centre = best.0 * 10;
    let fine = (centre - 20..=centre + 20).max_by(|x, y| score(&fa, &fb, *x).total_cmp(&score(&fa, &fb, *y))).unwrap_or(centre);
    Ok((fine as f64 / 1000.0, confidence))
}
