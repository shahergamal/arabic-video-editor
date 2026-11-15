//! Builds the picture for one frame of the timeline. Preview and export both
//! call `Compositor::render`, which is how they stay identical.

use crate::frame::Frame;
use crate::media::{read_still, FrameReader};
use crate::project::{Clip, MediaKind, Project, Transform};
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
        Self { text: TextRenderer::new(bundled_fonts), readers: HashMap::new(), stills: HashMap::new() }
    }

    /// Renders timeline frame `index` of `project` at the given output size.
    /// Playing frames in order is fast; jumping around reopens the files.
    pub fn render(&mut self, project: &Project, index: u64, width: u32, height: u32) -> Result<Frame> {
        let mut canvas = Frame::new(width, height);
        let layers: Vec<Clip> = project.pictures_at(index).into_iter().cloned().collect();
        for clip in &layers {
            let picture = match clip.media {
                MediaKind::Image => Some(self.still(&clip.source, width, height)?),
                _ => self.video_frame(project, clip, index, width, height)?,
            };
            if let Some(p) = picture {
                draw_layer(&mut canvas, &p, &clip.transform);
            }
        }
        // Close files that are no longer on screen.
        self.readers.retain(|id, _| layers.iter().any(|c| &c.id == id));

        for layer in project.texts.iter().filter(|t| index >= t.start && index < t.start + t.duration) {
            self.text.draw(&mut canvas, layer);
        }
        Ok(canvas)
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
        let reusable = matches!(self.readers.get(&clip.id), Some(r) if r.clip.source == clip.source
            && r.clip.source_in == clip.source_in && r.clip.start == clip.start && r.size == (w, h)
            && (index + 1 == r.next || (index >= r.next && index - r.next <= MAX_SKIP)));
        if !reusable {
            let offset = index - clip.start;
            let seconds = clip.source_in + project.fps.frame_to_seconds(offset);
            let reader = FrameReader::open(&clip.source, seconds, w, h, project.fps)?;
            self.readers
                .insert(clip.id.clone(), OpenReader { clip: clip.clone(), size: (w, h), next: index, reader, last: None });
        }
        let r = self.readers.get_mut(&clip.id).expect("reader opened above");
        if r.next == index + 1 {
            // Same frame asked again (e.g. a paused preview redrawing text).
            return Ok(r.last.clone());
        }
        while r.next < index {
            r.reader.next_frame()?;
            r.next += 1;
        }
        let f = r.reader.next_frame()?;
        r.next = index + 1;
        r.last = f.clone();
        Ok(f)
    }
}

/// Paints `src` (same size as `dst`, with transparency) over `dst`,
/// scaled, moved and faded by `t`.
pub fn draw_layer(dst: &mut Frame, src: &Frame, t: &Transform) {
    let opacity = t.opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 || t.scale <= 0.0 {
        return;
    }
    let (w, h) = (dst.width as usize, dst.height as usize);
    if t.scale == 1.0 && t.x == 0.0 && t.y == 0.0 {
        let o = (opacity * 255.0).round() as u32;
        for (d, s) in dst.data.chunks_exact_mut(4).zip(src.data.chunks_exact(4)) {
            over(d, [s[0] as f32, s[1] as f32, s[2] as f32], (s[3] as u32 * o / 255) as f32 / 255.0);
        }
        return;
    }
    // Where the scaled picture lands, then sample the source for each pixel.
    let (cx, cy) = (w as f32 / 2.0 + t.x * w as f32, h as f32 / 2.0 + t.y * h as f32);
    let (half_w, half_h) = (w as f32 * t.scale / 2.0, h as f32 * t.scale / 2.0);
    let x0 = (cx - half_w).floor().max(0.0) as usize;
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
