//! Builds the picture for one frame of the timeline. Preview and export both
//! call `Compositor::render`, which is how they stay identical.

use crate::frame::Frame;
use crate::media::{read_still, FrameReader};
use crate::project::{Adjust, Clip, MediaKind, Project, TextAnim, TextLayer, Transform};
use crate::text::TextRenderer;
use crate::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Reading a few frames ahead is cheaper than reopening the file.
const MAX_SKIP: u64 = 15;

pub struct Compositor {
    pub text: TextRenderer,
    readers: HashMap<String, OpenReader>,
    stills: HashMap<(PathBuf, u32, u32), Frame>,
    /// Smaller copies of big videos, used instead of the original when set
    /// (the preview only; export always reads the originals).
    pub proxies: HashMap<PathBuf, PathBuf>,
}

struct OpenReader {
    clip: Clip,
    size: (u32, u32),
    /// The timeline frame the reader will return next.
    next: u64,
    reader: FrameReader,
    last: Option<Frame>,
}

impl Compositor {
    pub fn new(bundled_fonts: Option<&Path>) -> Self {
        Self { text: TextRenderer::new(bundled_fonts), readers: HashMap::new(), stills: HashMap::new(), proxies: HashMap::new() }
    }

    /// Like `new`, with several font folders (bundled fonts + fonts the user added).
    pub fn with_font_dirs(dirs: &[PathBuf]) -> Self {
        Self {
            text: TextRenderer::with_dirs(dirs.iter().map(|d| d.as_path())),
            readers: HashMap::new(),
            stills: HashMap::new(),
            proxies: HashMap::new(),
        }
    }

    /// Renders timeline frame `index` of `project` at the given output size.
    /// Playing frames in order is fast; jumping around reopens the files.
    pub fn render(&mut self, project: &Project, index: u64, width: u32, height: u32) -> Result<Frame> {
        let mut canvas = Frame::new(width, height);
        let layers: Vec<(Clip, Option<(String, f32)>)> = project
            .layers_at(index)
            .into_iter()
            .map(|l| (l.clip.clone(), l.transition.map(|(t, p)| (t.kind.clone(), p))))
            .collect();
        for (clip, transition) in &layers {
            let picture = match clip.media {
                MediaKind::Image => Some(self.still(&clip.source, width, height)?),
                _ => self.video_frame(project, clip, index, width, height)?,
            };
            let Some(mut p) = picture else { continue };
            if !clip.adjust.is_neutral() {
                apply_adjust(&mut p, &clip.adjust);
            }
            let mut t = clip.transform_at(index);
            let mut reveal_from = 0.0;
            if let Some((kind, raw)) = transition {
                let e = ease(*raw);
                match kind.as_str() {
                    "black" => {
                        // Out to black, then in from black.
                        fill_black(&mut canvas, (e * 2.0).min(1.0));
                        if e < 0.5 {
                            continue;
                        }
                        t.opacity *= e * 2.0 - 1.0;
                    }
                    // New picture comes in from the left: the next thing in Arabic reading order.
                    "slide" => t.x -= 1.0 - e,
                    // Revealed from right to left, the way Arabic is read.
                    "wipe" => reveal_from = 1.0 - e,
                    "zoom" => {
                        t.scale *= 1.0 + 0.15 * (1.0 - e);
                        t.opacity *= e;
                    }
                    _ => t.opacity *= e,
                }
            }
            draw_layer_from(&mut canvas, &p, &t, reveal_from);
        }
        // Close files that are no longer on screen.
        self.readers.retain(|id, _| layers.iter().any(|(c, _)| &c.id == id));

        for layer in project.texts.iter().filter(|t| index >= t.start && index < t.start + t.duration) {
            self.draw_text(&mut canvas, layer, index);
        }
        Ok(canvas)
    }

    fn draw_text(&mut self, canvas: &mut Frame, layer: &TextLayer, index: u64) {
        let local = index - layer.start;
        let left = layer.start + layer.duration - index;
        let parts: Vec<(&str, f32)> = [phase(&layer.anim_in, local), phase(&layer.anim_out, left.saturating_sub(1))]
            .into_iter()
            .flatten()
            .collect();
        if parts.is_empty() {
            self.text.draw(canvas, layer);
            return;
        }
        let mut t = Transform::default();
        let mut words = None;
        for (kind, p) in parts {
            match kind {
                "rise" => {
                    t.y += 0.04 * (1.0 - p);
                    t.opacity *= p;
                }
                "pop" => {
                    t.scale *= 0.6 + 0.4 * back_out(p);
                    t.opacity *= (p * 2.0).min(1.0);
                }
                "zoom" => {
                    t.scale *= 1.3 - 0.3 * p;
                    t.opacity *= p;
                }
                "words" => words = Some(words.unwrap_or(1.0f32).min(p)),
                _ => t.opacity *= p,
            }
        }
        // Scale around the text's own centre, not the middle of the frame.
        t.x += (layer.x - 0.5) * (1.0 - t.scale);
        t.y += (layer.y - 0.5) * (1.0 - t.scale);
        if t.is_identity() {
            self.text.draw_with(canvas, layer, words);
            return;
        }
        let mut alone = Frame::transparent(canvas.width, canvas.height);
        self.text.draw_with(&mut alone, layer, words);
        draw_layer(canvas, &alone, &t);
    }

    fn still(&mut self, path: &Path, w: u32, h: u32) -> Result<Frame> {
        let key = (path.to_path_buf(), w, h);
        if let Some(f) = self.stills.get(&key) {
            return Ok(f.clone());
        }
        if self.stills.len() > 32 {
            self.stills.clear();
        }
        let f = read_still(path, w, h)?;
        self.stills.insert(key, f.clone());
        Ok(f)
    }

    fn video_frame(&mut self, project: &Project, clip: &Clip, index: u64, w: u32, h: u32) -> Result<Option<Frame>> {
        // During a transition the outgoing clip is asked for frames after its
        // end: show its last frame, held, so the result never depends on
        // whether the file happens to continue.
        let index = index.min(clip.end().saturating_sub(1));
        let reusable = matches!(self.readers.get(&clip.id), Some(r) if r.clip.source == clip.source
            && r.clip.source_in == clip.source_in && r.clip.start == clip.start && r.clip.speed == clip.speed && r.size == (w, h)
            && (index + 1 == r.next || (index >= r.next && index - r.next <= MAX_SKIP)));
        if !reusable {
            let seconds = clip.source_time(index, project.fps);
            let file = self.proxies.get(&clip.source).filter(|p| p.exists()).unwrap_or(&clip.source);
            let reader = FrameReader::open_at_speed(file, seconds, w, h, project.fps, clip.speed)?;
            self.readers
                .insert(clip.id.clone(), OpenReader { clip: clip.clone(), size: (w, h), next: index, reader, last: None });
        }
        let r = self.readers.get_mut(&clip.id).expect("reader opened above");
        if r.next == index + 1 {
            // Same frame asked again (e.g. a paused preview redrawing text).
            return Ok(r.last.clone());
        }
        while r.next < index {
            if let Some(f) = r.reader.next_frame()? {
                r.last = Some(f);
            }
            r.next += 1;
        }
        let f = r.reader.next_frame()?;
        r.next = index + 1;
        // Past the end of the file (e.g. during a transition): hold the last picture.
        if f.is_some() {
            r.last = f;
        }
        Ok(r.last.clone())
    }
}

/// The animation running at `frames_from_edge` frames from the start (or
/// end) of a text, and how far along it is.
fn phase(a: &Option<TextAnim>, frames_from_edge: u64) -> Option<(&str, f32)> {
    a.as_ref()
        .filter(|a| a.kind != "none" && a.frames > 0 && frames_from_edge < a.frames)
        .map(|a| (a.kind.as_str(), ease(frames_from_edge as f32 / a.frames as f32)))
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Overshoots a little before settling, for a lively "pop".
fn back_out(t: f32) -> f32 {
    let c = 1.7;
    let u = t - 1.0;
    1.0 + (c + 1.0) * u * u * u + c * u * u
}

fn fill_black(dst: &mut Frame, amount: f32) {
    let keep = 1.0 - amount.clamp(0.0, 1.0);
    for px in dst.data.chunks_exact_mut(4) {
        for c in &mut px[..3] {
            *c = (*c as f32 * keep).round() as u8;
        }
    }
}

/// Brightness, contrast, warmth and colour strength, applied in place.
pub fn apply_adjust(f: &mut Frame, a: &Adjust) {
    // One lookup table per channel for brightness, contrast and warmth.
    let mut lut = [[0u8; 256]; 3];
    let warm = [a.temperature * 0.12, a.temperature * 0.03, -a.temperature * 0.12];
    for (ch, table) in lut.iter_mut().enumerate() {
        for (i, v) in table.iter_mut().enumerate() {
            let x = i as f32 / 255.0;
            let y = (x - 0.5) * (1.0 + a.contrast) + 0.5 + a.brightness * 0.35 + warm[ch];
            *v = (y.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    let sat = 1.0 + a.saturation;
    for px in f.data.chunks_exact_mut(4) {
        if px[3] == 0 {
            continue;
        }
        let rgb = [lut[0][px[0] as usize] as f32, lut[1][px[1] as usize] as f32, lut[2][px[2] as usize] as f32];
        if a.saturation == 0.0 {
            px[..3].copy_from_slice(&[rgb[0] as u8, rgb[1] as u8, rgb[2] as u8]);
            continue;
        }
        let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        for c in 0..3 {
            px[c] = (l + (rgb[c] - l) * sat).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Paints `src` (same size as `dst`, with transparency) over `dst`,
/// scaled, moved and faded by `t`.
pub fn draw_layer(dst: &mut Frame, src: &Frame, t: &Transform) {
    draw_layer_from(dst, src, t, 0.0);
}

/// Like `draw_layer`, but only the part of the frame to the right of
/// `reveal_from` (a share of the width) is painted.
pub fn draw_layer_from(dst: &mut Frame, src: &Frame, t: &Transform, reveal_from: f32) {
    let opacity = t.opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 || t.scale <= 0.0 || reveal_from >= 1.0 {
        return;
    }
    let (w, h) = (dst.width as usize, dst.height as usize);
    let min_x = (reveal_from.max(0.0) * w as f32).round() as usize;
    if t.scale == 1.0 && t.x == 0.0 && t.y == 0.0 && min_x == 0 {
        let o = (opacity * 255.0).round() as u32;
        for (d, s) in dst.data.chunks_exact_mut(4).zip(src.data.chunks_exact(4)) {
            over(d, [s[0] as f32, s[1] as f32, s[2] as f32], (s[3] as u32 * o / 255) as f32 / 255.0);
        }
        return;
    }
    // Where the scaled picture lands, then sample the source for each pixel.
    let (cx, cy) = (w as f32 / 2.0 + t.x * w as f32, h as f32 / 2.0 + t.y * h as f32);
    let (half_w, half_h) = (w as f32 * t.scale / 2.0, h as f32 * t.scale / 2.0);
    let x0 = ((cx - half_w).floor().max(0.0) as usize).max(min_x);
    let x1 = ((cx + half_w).ceil() as usize).min(w);
    let y0 = (cy - half_h).floor().max(0.0) as usize;
    let y1 = ((cy + half_h).ceil() as usize).min(h);
    for y in y0..y1 {
        let sy = (y as f32 + 0.5 - cy) / t.scale + h as f32 / 2.0 - 0.5;
        for x in x0..x1 {
            let sx = (x as f32 + 0.5 - cx) / t.scale + w as f32 / 2.0 - 0.5;
            if let Some((rgb, a)) = sample(src, sx, sy) {
                let i = (y * w + x) * 4;
                over(&mut dst.data[i..i + 4], rgb, a * opacity);
            }
        }
    }
}

#[inline]
fn over(d: &mut [u8], rgb: [f32; 3], a: f32) {
    if a <= 0.0 {
        return;
    }
    for c in 0..3 {
        d[c] = (rgb[c] * a + d[c] as f32 * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
    }
    d[3] = 255;
}

/// Bilinear sample with transparency; `None` outside the picture.
fn sample(src: &Frame, x: f32, y: f32) -> Option<([f32; 3], f32)> {
    let (w, h) = (src.width as i32, src.height as i32);
    if x < -0.5 || y < -0.5 || x > w as f32 - 0.5 || y > h as f32 - 0.5 {
        return None;
    }
    let (xf, yf) = (x.floor(), y.floor());
    let (fx, fy) = (x - xf, y - yf);
    let px = |xi: i32, yi: i32| {
        let xi = xi.clamp(0, w - 1) as usize;
        let yi = yi.clamp(0, h - 1) as usize;
        let i = (yi * src.width as usize + xi) * 4;
        &src.data[i..i + 4]
    };
    let (xi, yi) = (xf as i32, yf as i32);
    let p = [px(xi, yi), px(xi + 1, yi), px(xi, yi + 1), px(xi + 1, yi + 1)];
    let wts = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
    let mut a = 0.0;
    let mut rgb = [0f32; 3];
    for k in 0..4 {
        let pa = p[k][3] as f32 / 255.0 * wts[k];
        a += pa;
        for c in 0..3 {
            rgb[c] += p[k][c] as f32 * pa;
        }
    }
    if a <= 0.0 {
        return None;
    }
    Some(([rgb[0] / a, rgb[1] / a, rgb[2] / a], a))
}
