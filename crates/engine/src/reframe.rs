//! Turning a wide video into a tall one (or the other way) while keeping
//! the person or main subject in the frame. The subject is found with the
//! background-removal model a few times a second; the picture then follows
//! it with a smooth, calm movement (few keyframes, no jitter).

use crate::matte::{Matter, SIZE};
use crate::media::{probe, FrameReader};
use crate::project::{Clip, Keyframe, MediaKind, Project, Transform};
use crate::{Error, Result};
use std::sync::atomic::{AtomicBool, Ordering};

/// How often the subject is looked for, in seconds.
const STEP: f64 = 0.25;

/// Centre of the subject (shares of the picture, 0 to 1) at frames counted
/// from the clip's start.
pub fn subject_path(project: &Project, clip: &Clip, matter: &Matter, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<Vec<(u64, f32, f32)>> {
    let info = probe(&clip.source)?;
    let k = SIZE as f32 / info.width.max(info.height).max(1) as f32;
    let (w, h) = (((info.width as f32 * k) as u32).max(2), ((info.height as f32 * k) as u32).max(2));
    let fps = project.fps.as_f64();
    let every = ((STEP * fps).round() as u64).max(1);
    let mut reader = FrameReader::open_at_speed(&clip.source, clip.source_in, w, h, project.fps, clip.speed)?;
    let mut out = vec![];
    let (mut last_x, mut last_y) = (0.5f32, 0.5f32);
    for local in 0..clip.duration {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let Some(frame) = reader.next_frame()? else { break };
        if local % every != 0 && local + 1 != clip.duration {
            continue;
        }
        let mask = matter.matte(&frame)?;
        // Weighted centre of the clearly-subject pixels; keep the last
        // place when nothing is found (empty room, cut-away).
        let (mut sx, mut sy, mut sw) = (0f64, 0f64, 0f64);
        for (i, m) in mask.iter().enumerate() {
            if *m > 100 {
                let wgt = *m as f64;
                sx += (i % SIZE) as f64 * wgt;
                sy += (i / SIZE) as f64 * wgt;
                sw += wgt;
            }
        }
        if sw > 255.0 * (SIZE * SIZE) as f64 * 0.002 {
            last_x = ((sx / sw + 0.5) / SIZE as f64) as f32;
            // Faces sit in the upper part of a person: aim a little higher.
            last_y = (((sy / sw + 0.5) / SIZE as f64) as f32 - 0.08).max(0.0);
        }
        out.push((local, last_x, last_y));
        progress(local as f32 / clip.duration.max(1) as f32);
    }
    Ok(out)
}

/// Smooths a path and keeps only the points needed to follow it within
/// `tolerance` (Douglas–Peucker), so the camera moves calmly.
pub fn calm(path: &[(u64, f32, f32)], tolerance: f32) -> Vec<(u64, f32, f32)> {
    if path.len() < 3 {
        return path.to_vec();
    }
    // Moving average over about a second each way.
    let reach = 4usize;
    let smooth: Vec<(u64, f32, f32)> = (0..path.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(reach), (i + reach + 1).min(path.len()));
            let n = (b - a) as f32;
            let x = path[a..b].iter().map(|p| p.1).sum::<f32>() / n;
            let y = path[a..b].iter().map(|p| p.2).sum::<f32>() / n;
            (path[i].0, x, y)
        })
        .collect();
    let mut keep = vec![false; smooth.len()];
    keep[0] = true;
    *keep.last_mut().unwrap() = true;
    fn split(p: &[(u64, f32, f32)], a: usize, b: usize, tol: f32, keep: &mut [bool]) {
        if b <= a + 1 {
            return;
        }
        let (t0, t1) = (p[a].0 as f32, p[b].0 as f32);
        let mut worst = (0.0f32, a);
        for (i, q) in p.iter().enumerate().take(b).skip(a + 1) {
            let t = (q.0 as f32 - t0) / (t1 - t0).max(1.0);
            let (x, y) = (p[a].1 + (p[b].1 - p[a].1) * t, p[a].2 + (p[b].2 - p[a].2) * t);
            let d = (q.1 - x).abs().max((q.2 - y).abs());
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > tol {
            keep[worst.1] = true;
            split(p, a, worst.1, tol, keep);
            split(p, worst.1, b, tol, keep);
        }
    }
    split(&smooth, 0, smooth.len() - 1, tolerance, &mut keep);
    smooth.into_iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| p).collect()
}

/// Keyframes that fill a `width`×`height` frame with a source of
/// `src` size, with the subject path in the middle where possible.
pub fn follow_keyframes(path: &[(u64, f32, f32)], src: (u32, u32), width: u32, height: u32, zoom: f32) -> Vec<Keyframe> {
    let (sw, sh) = (src.0.max(1) as f32, src.1.max(1) as f32);
    let (w, h) = (width as f32, height as f32);
    let fit = (w / sw).min(h / sh);
    let fill = (w / sw).max(h / sh);
    let scale = fill / fit * zoom.max(1.0);
    // Size of the scaled picture as a share of the frame.
    let (pw, ph) = (sw * fit * scale / w, sh * fit * scale / h);
    let (room_x, room_y) = (((pw - 1.0) / 2.0).max(0.0), ((ph - 1.0) / 2.0).max(0.0));
    path.iter()
        .map(|(frame, u, v)| Keyframe {
            frame: *frame,
            transform: Transform {
                scale,
                x: (-(u - 0.5) * pw).clamp(-room_x, room_x),
                y: (-(v - 0.5) * ph).clamp(-room_y, room_y),
                ..Transform::default()
            },
        })
        .collect()
}

/// The clips worth following: videos that fill the frame (not small
/// picture-in-picture clips or logos).
pub fn candidates(project: &Project) -> Vec<Clip> {
    project
        .all_clips()
        .map(|(_, c)| c)
        .filter(|c| c.media == MediaKind::Video && c.transform.scale >= 0.99 && c.transform.x.abs() < 0.01 && c.transform.y.abs() < 0.01 && c.keyframes.is_empty())
        .cloned()
        .collect()
}
