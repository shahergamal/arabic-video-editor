//! Following a moving thing (a face, a car plate, a sign) across frames, so
//! a blur or shape can stay on it. Simple and dependable: compare a small
//! patch of the first frame against nearby places in each next frame.

use crate::media::FrameReader;
use crate::project::{Clip, MaskPoint};
use crate::time::Fps;
use crate::Result;
use std::sync::atomic::{AtomicBool, Ordering};

/// Width the frames are analysed at. Small is fast and plenty for tracking.
const WORK_W: u32 = 384;

fn luma_plane(f: &crate::frame::Frame) -> Vec<f32> {
    f.data.chunks_exact(4).map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).collect()
}

/// Follows the box centred at (x, y) with size (w, h) — all shares of the
/// frame — from `from_frame` (counted from the clip start) to the end of
/// the clip. Returns one point per frame.
#[allow(clippy::too_many_arguments)]
pub fn track_box(
    clip: &Clip,
    project_w: u32,
    project_h: u32,
    fps: Fps,
    from_frame: u64,
    (x, y, w, h): (f32, f32, f32, f32),
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<Vec<MaskPoint>> {
    let ww = WORK_W;
    let wh = ((WORK_W as f32 * project_h as f32 / project_w as f32 / 2.0).round() as u32 * 2).max(2);
    let start = clip.start + from_frame;
    let mut reader = FrameReader::open_at_speed(&clip.source, clip.source_time(start, fps), ww, wh, fps, clip.speed)?;
    let total = clip.duration.saturating_sub(from_frame);
    let (fw, fh) = (ww as i32, wh as i32);
    let tw = ((w * ww as f32) as i32).clamp(8, fw / 2);
    let th = ((h * wh as f32) as i32).clamp(8, fh / 2);
    let mut cx = (x * ww as f32) as i32;
    let mut cy = (y * wh as f32) as i32;

    let Some(first) = reader.next_frame()? else { return Ok(vec![]) };
    let plane = luma_plane(&first);
    // Big boxes are sampled sparsely: about 32 samples across is plenty to
    // match on, and keeps every frame quick.
    let stride = (tw.max(th) / 32).max(1);
    let offsets: Vec<(i32, i32)> = (-th / 2..th - th / 2)
        .step_by(stride as usize)
        .flat_map(|dy| (-tw / 2..tw - tw / 2).step_by(stride as usize).map(move |dx| (dx, dy)))
        .collect();
    let at = |p: &[f32], x: i32, y: i32| p[(y.clamp(0, fh - 1) * fw + x.clamp(0, fw - 1)) as usize];
    let patch = |p: &[f32], cx: i32, cy: i32| -> Vec<f32> { offsets.iter().map(|&(dx, dy)| at(p, cx + dx, cy + dy)).collect() };
    let mut template = patch(&plane, cx, cy);
    let mut out = vec![MaskPoint { frame: from_frame, x, y }];
    let radius = (tw.max(th) / 2).max(12);
    let step = (stride * 2).max(2);
    let (mut vx, mut vy) = (0f32, 0f32);

    for i in 1..total {
        if cancel.load(Ordering::Relaxed) {
            return Err(crate::Error::Cancelled);
        }
        let Some(f) = reader.next_frame()? else { break };
        let plane = luma_plane(&f);
        // Coarse search, then fine search around the best.
        // Lower is better. Both patches have their average brightness taken
        // away first, so a change of light does not throw the match off.
        let n = template.len() as f32;
        let t_mean = template.iter().sum::<f32>() / n;
        let score = |sx: i32, sy: i32| -> f32 {
            let c_mean = offsets.iter().map(|&(dx, dy)| at(&plane, sx + dx, sy + dy)).sum::<f32>() / n;
            offsets
                .iter()
                .zip(&template)
                .map(|(&(dx, dy), t)| ((at(&plane, sx + dx, sy + dy) - c_mean) - (t - t_mean)).abs())
                .sum()
        };
        // Search around where it should be if it keeps moving the same way.
        let (px, py) = (cx + vx.round() as i32, cy + vy.round() as i32);
        let mut best = (px, py, score(px, py));
        for sy in (py - radius..=py + radius).step_by(step as usize) {
            for sx in (px - radius..=px + radius).step_by(step as usize) {
                let s = score(sx, sy);
                if s < best.2 {
                    best = (sx, sy, s);
                }
            }
        }
        let (bx, by) = (best.0, best.1);
        for sy in by - step..=by + step {
            for sx in bx - step..=bx + step {
                let s = score(sx, sy);
                if s < best.2 {
                    best = (sx, sy, s);
                }
            }
        }
        let (nx, ny) = (best.0.clamp(0, fw - 1), best.1.clamp(0, fh - 1));
        vx = vx * 0.5 + (nx - cx) as f32 * 0.5;
        vy = vy * 0.5 + (ny - cy) as f32 * 0.5;
        cx = nx;
        cy = ny;
        // Let the template follow slow changes in light and angle.
        let now = patch(&plane, cx, cy);
        for (t, n) in template.iter_mut().zip(now) {
            *t = *t * 0.92 + n * 0.08;
        }
        out.push(MaskPoint { frame: from_frame + i, x: cx as f32 / ww as f32, y: cy as f32 / wh as f32 });
        progress(i, total);
    }
    Ok(out)
}
