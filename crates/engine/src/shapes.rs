//! Shapes and arrows over the video, drawn with smooth (anti-aliased)
//! edges from distance functions, so they look the same at every size.

use crate::frame::Frame;
use crate::project::{Shape, TextAnim};

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn back_out(t: f32) -> f32 {
    let c = 1.7;
    let u = t - 1.0;
    1.0 + (c + 1.0) * u * u * u + c * u * u
}

fn phase(a: &Option<TextAnim>, from_edge: u64) -> Option<(&str, f32)> {
    a.as_ref()
        .filter(|a| a.kind != "none" && a.frames > 0 && from_edge < a.frames)
        .map(|a| (a.kind.as_str(), ease(from_edge as f32 / a.frames as f32)))
}

/// Distance (in pixels) from a point to the shape's edge, negative inside.
/// `p` is in the shape's own coordinates (centre at 0, not turned).
fn rounded_box(p: (f32, f32), half: (f32, f32), r: f32) -> f32 {
    let q = (p.0.abs() - half.0 + r, p.1.abs() - half.1 + r);
    let outside = (q.0.max(0.0).powi(2) + q.1.max(0.0).powi(2)).sqrt();
    outside + q.0.max(q.1).min(0.0) - r
}

fn ellipse(p: (f32, f32), half: (f32, f32)) -> f32 {
    let (a, b) = (half.0.max(0.5), half.1.max(0.5));
    let k0 = ((p.0 / a).powi(2) + (p.1 / b).powi(2)).sqrt();
    let k1 = ((p.0 / (a * a)).powi(2) + (p.1 / (b * b)).powi(2)).sqrt();
    if k1 == 0.0 {
        return -a.min(b);
    }
    k0 * (k0 - 1.0) / k1
}

fn segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (pa, ba) = ((p.0 - a.0, p.1 - a.1), (b.0 - a.0, b.1 - a.1));
    let h = ((pa.0 * ba.0 + pa.1 * ba.1) / (ba.0 * ba.0 + ba.1 * ba.1).max(1e-6)).clamp(0.0, 1.0);
    ((pa.0 - ba.0 * h).powi(2) + (pa.1 - ba.1 * h).powi(2)).sqrt()
}

/// Filled triangle, pointing along +x with the tip at `tip`.
fn triangle(p: (f32, f32), tip: f32, len: f32, half_w: f32) -> f32 {
    // Inside when behind the tip, in front of the base and within the sides.
    let x = tip - p.0;
    let side = (p.1.abs() - half_w * (x / len).clamp(0.0, 1.0)) * len / (len * len + half_w * half_w).sqrt();
    side.max(-x).max(x - len)
}

/// Five-pointed star (outer radius `r`, inner about 0.45 r), as a polygon.
fn star(p: (f32, f32), r: f32) -> f32 {
    let inner = r * 0.45;
    let pts: Vec<(f32, f32)> = (0..10)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
            let rr = if i % 2 == 0 { r } else { inner };
            (rr * a.cos(), rr * a.sin())
        })
        .collect();
    let mut d = f32::MAX;
    let mut inside = false;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        d = d.min(segment(p, a, b));
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
    }
    if inside {
        -d
    } else {
        d
    }
}

/// Draws one shape at timeline frame `index`.
pub fn draw(canvas: &mut Frame, s: &Shape, index: u64) {
    let local = index.saturating_sub(s.start);
    let left = (s.start + s.duration).saturating_sub(index + 1);
    let (mut scale, mut opacity, mut lift, mut grow) = (1.0f32, 1.0f32, 0.0f32, 1.0f32);
    for (kind, p) in [phase(&s.anim_in, local), phase(&s.anim_out, left)].into_iter().flatten() {
        match kind {
            "rise" => {
                lift += 0.04 * (1.0 - p);
                opacity *= p;
            }
            "pop" => {
                scale *= 0.6 + 0.4 * back_out(p);
                opacity *= (p * 2.0).min(1.0);
            }
            "zoom" => {
                scale *= 1.3 - 0.3 * p;
                opacity *= p;
            }
            "draw" => grow = grow.min(p),
            _ => opacity *= p,
        }
    }
    if opacity <= 0.0 || scale <= 0.0 {
        return;
    }
    let (w, h) = (canvas.width as f32, canvas.height as f32);
    let (cx, cy) = (s.x * w, (s.y + lift) * h);
    let half = (s.w * w * scale / 2.0, s.h * h * scale / 2.0);
    let stroke = (s.stroke_width * h * scale).max(0.0);
    let line = s.kind == "line" || s.kind == "arrow";
    let line_w = stroke.max(1.0);
    let head = (line_w * 3.2).max(8.0);
    // Lines grow from their start (the right end in Arabic reading order).
    let (a, mut b) = ((half.0, 0.0), (-half.0, 0.0));
    if line {
        b.0 = a.0 + (b.0 - a.0) * grow;
    }
    let (sin, cos) = s.rotation.to_radians().sin_cos();
    let reach = if line { half.0 + head } else { (half.0 * half.0 + half.1 * half.1).sqrt() + stroke } + 2.0;
    let shadow = if s.shadow { (h * 0.004).max(1.5) } else { 0.0 };
    let x0 = (cx - reach).floor().max(0.0) as usize;
    let x1 = ((cx + reach + shadow).ceil().max(0.0) as usize).min(canvas.width as usize);
    let y0 = (cy - reach).floor().max(0.0) as usize;
    let y1 = ((cy + reach + shadow).ceil().max(0.0) as usize).min(canvas.height as usize);
    let dist = |px: f32, py: f32| -> (f32, f32) {
        let (dx, dy) = (px - cx, py - cy);
        let p = (dx * cos + dy * sin, -dx * sin + dy * cos);
        match s.kind.as_str() {
            "ellipse" => (ellipse(p, half), f32::MAX),
            "star" => (star(p, half.0.min(half.1)), f32::MAX),
            "line" | "arrow" => {
                let len = (a.0 - b.0).abs();
                let shaft_end = if s.kind == "arrow" && len > head { (b.0 + head * 0.8, 0.0) } else { b };
                let mut d = segment(p, a, shaft_end) - line_w / 2.0;
                if s.kind == "arrow" && len > 1.0 {
                    // The point is at the far end of the line (`b`): flip x so it points along +x.
                    let q = (-(p.0), p.1);
                    d = d.min(triangle(q, -b.0, head.min(len), head * 0.6));
                }
                (f32::MAX, d)
            }
            _ => (rounded_box(p, half, s.radius.clamp(0.0, 0.5) * 2.0 * half.0.min(half.1)), f32::MAX),
        }
    };
    let cover = |d: f32| (0.5 - d).clamp(0.0, 1.0);
    let paint = |canvas: &mut Frame, x: usize, y: usize, c: [u8; 4], k: f32| {
        let a = c[3] as f32 / 255.0 * k * opacity;
        if a <= 0.0 {
            return;
        }
        let i = (y * canvas.width as usize + x) * 4;
        for ch in 0..3 {
            let d = canvas.data[i + ch] as f32;
            canvas.data[i + ch] = (c[ch] as f32 * a + d * (1.0 - a)).round() as u8;
        }
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            if shadow > 0.0 {
                let (fill_d, line_d) = dist(px - shadow, py - shadow);
                let k = cover(fill_d.min(line_d).min(if s.fill[3] == 0 && stroke > 0.0 { fill_d.abs() - stroke / 2.0 } else { f32::MAX }));
                paint(canvas, x, y, [0, 0, 0, 110], k);
            }
            let (fill_d, line_d) = dist(px, py);
            if line {
                paint(canvas, x, y, s.stroke, cover(line_d));
                continue;
            }
            if s.fill[3] > 0 {
                paint(canvas, x, y, s.fill, cover(fill_d));
            }
            if stroke > 0.0 && s.stroke[3] > 0 {
                paint(canvas, x, y, s.stroke, cover(fill_d.abs() - stroke / 2.0));
            }
        }
    }
}
