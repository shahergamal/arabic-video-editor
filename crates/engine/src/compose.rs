//! Builds the picture for one frame of the timeline. Preview and export both
//! call `Compositor::render`, which is how they stay identical.

use crate::frame::Frame;
use crate::media::{read_still, FrameReader};
use crate::lut::Lut;
use crate::matte::{Matter, SIZE as MATTE_SIZE};
use crate::project::{Adjust, ChromaKey, Clip, Mask, MediaKind, Project, TextAnim, TextLayer, Transform};
use std::sync::Arc;
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
    /// Where cut-out masks are kept, and the models that make them.
    pub mattes: Option<MatteSetup>,
    luts: HashMap<PathBuf, Option<Arc<Lut>>>,
    sizes: HashMap<PathBuf, (u32, u32)>,
}

/// Background removal: masks are worked out once per source frame and kept
/// on disk, so the preview and every export use exactly the same mask.
pub struct MatteSetup {
    pub cache: PathBuf,
    /// Model id ("people", "objects") → model file. Missing files are fine:
    /// masks already in the cache are still used.
    pub models: HashMap<String, PathBuf>,
    loaded: HashMap<String, Arc<Matter>>,
}

impl MatteSetup {
    pub fn new(cache: PathBuf, models: HashMap<String, PathBuf>) -> Self {
        Self { cache, models, loaded: HashMap::new() }
    }

    fn model(&mut self, id: &str) -> Option<Arc<Matter>> {
        if let Some(m) = self.loaded.get(id) {
            return Some(m.clone());
        }
        let path = self.models.get(id).filter(|p| p.exists())?;
        let m = Arc::new(Matter::load(path).ok()?);
        self.loaded.insert(id.to_string(), m.clone());
        Some(m)
    }
}

/// Folder for one source file's masks made with one model.
pub fn matte_dir(cache: &Path, source: &Path, model: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    if let Ok(m) = std::fs::metadata(source) {
        m.len().hash(&mut h);
        m.modified().ok().hash(&mut h);
    }
    cache.join(format!("{:016x}-{model}", h.finish()))
}

/// Masks are stored for every 1/30 s of the source.
pub fn matte_key(seconds: f64) -> u64 {
    (seconds * 30.0).round().max(0.0) as u64
}

struct OpenReader {
    clip: Clip,
    size: (u32, u32),
    /// The timeline frame the reader will return next.
    next: u64,
    reader: FrameReader,
    last: Option<Frame>,
    /// For speed ramps: the source time the reader was opened at, and the
    /// number of source frames read since.
    open_t: f64,
    src_next: u64,
}

impl Compositor {
    pub fn new(bundled_fonts: Option<&Path>) -> Self {
        Self::from_text(TextRenderer::new(bundled_fonts))
    }

    /// Like `new`, with several font folders (bundled fonts + fonts the user added).
    pub fn with_font_dirs(dirs: &[PathBuf]) -> Self {
        Self::from_text(TextRenderer::with_dirs(dirs.iter().map(|d| d.as_path())))
    }

    fn from_text(text: TextRenderer) -> Self {
        Self {
            text,
            readers: HashMap::new(),
            stills: HashMap::new(),
            proxies: HashMap::new(),
            mattes: None,
            luts: HashMap::new(),
            sizes: HashMap::new(),
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
                if let Some(lut) = clip.adjust.lut.as_ref().and_then(|l| self.lut(l)) {
                    lut.apply(&mut p, clip.adjust.lut_strength);
                }
            }
            if let Some(ck) = &clip.chroma {
                apply_chroma(&mut p, ck);
            }
            if let Some(model) = &clip.remove_bg {
                self.cut_out(&mut p, project, clip, model, index)?;
            }
            if let Some(m) = &clip.mask {
                apply_mask(&mut p, m, index.min(clip.end().saturating_sub(1)).saturating_sub(clip.start));
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

    fn lut(&mut self, path: &Path) -> Option<Arc<Lut>> {
        self.luts.entry(path.to_path_buf()).or_insert_with(|| Lut::load(path).ok().map(Arc::new)).clone()
    }

    /// Where the source picture sits inside a frame of this size (it is
    /// fitted, so there may be empty bands).
    fn picture_rect(&mut self, source: &Path, w: u32, h: u32) -> (u32, u32, u32, u32) {
        let (sw, sh) = *self
            .sizes
            .entry(source.to_path_buf())
            .or_insert_with(|| crate::media::probe(source).map(|i| (i.width, i.height)).unwrap_or((w, h)));
        let k = (w as f32 / sw.max(1) as f32).min(h as f32 / sh.max(1) as f32);
        let (pw, ph) = (((sw as f32 * k).round() as u32).clamp(1, w), ((sh as f32 * k).round() as u32).clamp(1, h));
        ((w - pw) / 2, (h - ph) / 2, pw, ph)
    }

    /// Makes the background see-through, using the stored mask for this
    /// moment of the source, or working it out (and storing it) if needed.
    fn cut_out(&mut self, p: &mut Frame, project: &Project, clip: &Clip, model: &str, index: u64) -> Result<()> {
        let Some(setup) = self.mattes.as_mut() else { return Ok(()) };
        let t = clip.source_time(index.min(clip.end().saturating_sub(1)), project.fps);
        let dir = matte_dir(&setup.cache, &clip.source, model);
        let file = dir.join(format!("{}.png", matte_key(t)));
        let rect = self.picture_rect(&clip.source, p.width, p.height);
        let setup = self.mattes.as_mut().expect("checked above");
        let mask = match read_gray(&file) {
            Some(m) => m,
            None => {
                let Some(m) = setup.model(model) else { return Ok(()) };
                let crop = crop(p, rect);
                let mask = m.matte(&crop)?;
                std::fs::create_dir_all(&dir)?;
                write_gray(&file, &mask)?;
                mask
            }
        };
        apply_matte_rect(p, &mask, rect);
        Ok(())
    }

    fn video_frame(&mut self, project: &Project, clip: &Clip, index: u64, w: u32, h: u32) -> Result<Option<Frame>> {
        // During a transition the outgoing clip is asked for frames after its
        // end: show its last frame, held, so the result never depends on
        // whether the file happens to continue.
        let index = index.min(clip.end().saturating_sub(1));
        if !clip.speed_ramp.is_empty() {
            return self.ramp_frame(project, clip, index, w, h);
        }
        let reusable = matches!(self.readers.get(&clip.id), Some(r) if r.clip.source == clip.source
            && r.clip.source_in == clip.source_in && r.clip.start == clip.start && r.clip.speed == clip.speed
            && r.clip.speed_ramp.is_empty() && r.size == (w, h)
            && (index + 1 == r.next || (index >= r.next && index - r.next <= MAX_SKIP)));
        if !reusable {
            let seconds = clip.source_time(index, project.fps);
            let file = self.proxies.get(&clip.source).filter(|p| p.exists()).unwrap_or(&clip.source);
            let reader = FrameReader::open_at_speed(file, seconds, w, h, project.fps, clip.speed)?;
            self.readers.insert(
                clip.id.clone(),
                OpenReader { clip: clip.clone(), size: (w, h), next: index, reader, last: None, open_t: seconds, src_next: 0 },
            );
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

    /// Speed ramps: the reader runs at normal speed and we pick the source
    /// frame that matches each moment, skipping or repeating frames.
    fn ramp_frame(&mut self, project: &Project, clip: &Clip, index: u64, w: u32, h: u32) -> Result<Option<Frame>> {
        let fps = project.fps.as_f64();
        let t = clip.source_time(index, project.fps);
        let reusable = matches!(self.readers.get(&clip.id), Some(r) if r.clip == *clip && r.size == (w, h) && {
            let want = ((t - r.open_t) * fps).round();
            want >= 0.0 && want + 1.0 >= r.src_next as f64 && want <= (r.src_next + MAX_SKIP * 4) as f64
        });
        if !reusable {
            let file = self.proxies.get(&clip.source).filter(|p| p.exists()).unwrap_or(&clip.source);
            let reader = FrameReader::open(file, t, w, h, project.fps)?;
            self.readers.insert(
                clip.id.clone(),
                OpenReader { clip: clip.clone(), size: (w, h), next: 0, reader, last: None, open_t: t, src_next: 0 },
            );
        }
        let r = self.readers.get_mut(&clip.id).expect("reader opened above");
        let want = ((t - r.open_t) * fps).round().max(0.0) as u64;
        if want + 1 == r.src_next {
            return Ok(r.last.clone());
        }
        while r.src_next <= want {
            if let Some(f) = r.reader.next_frame()? {
                r.last = Some(f);
            }
            r.src_next += 1;
        }
        Ok(r.last.clone())
    }
}

fn read_gray(path: &Path) -> Option<Vec<u8>> {
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?));
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    (info.width as usize == MATTE_SIZE && info.height as usize == MATTE_SIZE && buf.len() >= MATTE_SIZE * MATTE_SIZE)
        .then(|| buf[..MATTE_SIZE * MATTE_SIZE].to_vec())
}

fn write_gray(path: &Path, mask: &[u8]) -> Result<()> {
    // Written beside, then renamed, so two exports never read half a file.
    let tmp = path.with_extension("tmp");
    {
        let f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        let mut enc = png::Encoder::new(f, MATTE_SIZE as u32, MATTE_SIZE as u32);
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| crate::Error::Model(e.to_string()))?;
        w.write_image_data(mask).map_err(|e| crate::Error::Model(e.to_string()))?;
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}

fn crop(f: &Frame, (x, y, w, h): (u32, u32, u32, u32)) -> Frame {
    let mut out = Frame::transparent(w, h);
    for row in 0..h {
        let src = (((y + row) * f.width + x) * 4) as usize;
        let dst = (row * w * 4) as usize;
        out.data[dst..dst + (w * 4) as usize].copy_from_slice(&f.data[src..src + (w * 4) as usize]);
    }
    out
}

/// The mask (MATTE_SIZE²) stretched over the picture area of the frame.
fn apply_matte_rect(f: &mut Frame, mask: &[u8], (rx, ry, rw, rh): (u32, u32, u32, u32)) {
    let n = MATTE_SIZE;
    let at = |x: usize, y: usize| mask[y.min(n - 1) * n + x.min(n - 1)] as f32;
    for y in 0..rh as usize {
        let fy = ((y as f32 + 0.5) * n as f32 / rh as f32 - 0.5).max(0.0);
        let (y0, ty) = (fy as usize, fy.fract());
        for x in 0..rw as usize {
            let fx = ((x as f32 + 0.5) * n as f32 / rw as f32 - 0.5).max(0.0);
            let (x0, tx) = (fx as usize, fx.fract());
            let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx;
            let bottom = at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx;
            let m = top * (1.0 - ty) + bottom * ty;
            let i = (((ry as usize + y) * f.width as usize) + rx as usize + x) * 4 + 3;
            f.data[i] = (f.data[i] as f32 * m / 255.0).round() as u8;
        }
    }
}

/// Green (or any colour) screen: pixels close to the key colour become
/// see-through, with a soft edge, and the key colour's spill is reduced.
pub fn apply_chroma(f: &mut Frame, k: &ChromaKey) {
    let chroma = |r: f32, g: f32, b: f32| {
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        ((b - y) / 1.8556, (r - y) / 1.5748)
    };
    let (kb, kr) = chroma(k.color[0] as f32, k.color[1] as f32, k.color[2] as f32);
    let lo = k.strength.clamp(0.0, 1.0) * 120.0;
    let hi = lo + k.softness.clamp(0.0, 1.0) * 80.0 + 1.0;
    let green_key = k.color[1] > k.color[0] && k.color[1] > k.color[2];
    for px in f.data.chunks_exact_mut(4) {
        let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
        let (cb, cr) = chroma(r, g, b);
        let d = ((cb - kb).powi(2) + (cr - kr).powi(2)).sqrt();
        let a = ((d - lo) / (hi - lo)).clamp(0.0, 1.0);
        px[3] = (px[3] as f32 * a).round() as u8;
        if green_key && a < 1.0 {
            // Pull green spill on the edges back to neutral.
            px[1] = g.min((r + b) / 2.0 + (g - (r + b) / 2.0) * a).round() as u8;
        }
    }
}

/// How much a point (shares of the frame) is inside the mask: 1 inside,
/// 0 outside, soft in between when feathered.
fn mask_weight(m: &Mask, cx: f32, cy: f32, px: f32, py: f32) -> f32 {
    let dx = (px - cx) / (m.w / 2.0).max(1e-4);
    let dy = (py - cy) / (m.h / 2.0).max(1e-4);
    let d = if m.shape == "ellipse" { (dx * dx + dy * dy).sqrt() } else { dx.abs().max(dy.abs()) };
    if m.feather <= 0.0 {
        return if d <= 1.0 { 1.0 } else { 0.0 };
    }
    ((1.0 + m.feather - d) / (2.0 * m.feather)).clamp(0.0, 1.0)
}

pub fn apply_mask(f: &mut Frame, m: &Mask, local: u64) {
    let (cx, cy) = m.centre_at(local);
    let (w, h) = (f.width as usize, f.height as usize);
    if m.mode == "blur" {
        // Blur only the box around the shape, then blend it in by the mask.
        let reach = m.feather.max(0.0) + 1.0;
        let x0 = (((cx - m.w / 2.0 * reach) * w as f32).floor().max(0.0)) as usize;
        let x1 = (((cx + m.w / 2.0 * reach) * w as f32).ceil() as usize).min(w);
        let y0 = (((cy - m.h / 2.0 * reach) * h as f32).floor().max(0.0)) as usize;
        let y1 = (((cy + m.h / 2.0 * reach) * h as f32).ceil() as usize).min(h);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let radius = ((m.blur * w as f32) as usize).max(1);
        let blurred = box_blur(f, (x0, y0, x1, y1), radius);
        let bw = x1 - x0;
        for y in y0..y1 {
            for x in x0..x1 {
                let a = mask_weight(m, cx, cy, (x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                if a <= 0.0 {
                    continue;
                }
                let i = (y * w + x) * 4;
                let j = ((y - y0) * bw + (x - x0)) * 3;
                for c in 0..3 {
                    f.data[i + c] = (f.data[i + c] as f32 * (1.0 - a) + blurred[j + c] * a).round() as u8;
                }
            }
        }
        return;
    }
    let keep = m.mode != "cut";
    for y in 0..h {
        for x in 0..w {
            let a = mask_weight(m, cx, cy, (x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let v = if keep { a } else { 1.0 - a };
            let i = (y * w + x) * 4 + 3;
            f.data[i] = (f.data[i] as f32 * v).round() as u8;
        }
    }
}

/// Strong blur of one area (three box passes ≈ a gaussian), sampling a bit
/// outside the area so its edges blur naturally.
fn box_blur(f: &Frame, (x0, y0, x1, y1): (usize, usize, usize, usize), r: usize) -> Vec<f32> {
    let (w, h) = (f.width as usize, f.height as usize);
    let (bw, bh) = (x1 - x0, y1 - y0);
    let mut buf: Vec<f32> = Vec::with_capacity(bw * bh * 3);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * w + x) * 4;
            buf.extend_from_slice(&[f.data[i] as f32, f.data[i + 1] as f32, f.data[i + 2] as f32]);
        }
    }
    let _ = h;
    for _ in 0..3 {
        // Horizontal.
        let mut tmp = vec![0f32; buf.len()];
        for y in 0..bh {
            for c in 0..3 {
                let mut acc = 0f32;
                let at = |x: isize| buf[(y * bw + x.clamp(0, bw as isize - 1) as usize) * 3 + c];
                for k in -(r as isize)..=(r as isize) {
                    acc += at(k);
                }
                for x in 0..bw {
                    tmp[(y * bw + x) * 3 + c] = acc / (2 * r + 1) as f32;
                    acc += at(x as isize + r as isize + 1) - at(x as isize - r as isize);
                }
            }
        }
        // Vertical.
        for x in 0..bw {
            for c in 0..3 {
                let mut acc = 0f32;
                let at = |y: isize| tmp[(y.clamp(0, bh as isize - 1) as usize * bw + x) * 3 + c];
                for k in -(r as isize)..=(r as isize) {
                    acc += at(k);
                }
                for y in 0..bh {
                    buf[(y * bw + x) * 3 + c] = acc / (2 * r + 1) as f32;
                    acc += at(y as isize + r as isize + 1) - at(y as isize - r as isize);
                }
            }
        }
    }
    buf
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

fn apply_vignette(f: &mut Frame, amount: f32) {
    let (w, h) = (f.width as f32, f.height as f32);
    for (n, px) in f.data.chunks_exact_mut(4).enumerate() {
        let x = (n % f.width as usize) as f32 / w - 0.5;
        let y = (n / f.width as usize) as f32 / h - 0.5;
        let d = (x * x + y * y) * 2.0;
        let k = (1.0 - amount * 0.7 * d * d).clamp(0.0, 1.6);
        for c in &mut px[..3] {
            *c = (*c as f32 * k).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Brightness, contrast, warmth and colour strength, applied in place.
pub fn apply_adjust(f: &mut Frame, a: &Adjust) {
    // One lookup table per channel for brightness, contrast and warmth.
    let mut lut = [[0u8; 256]; 3];
    let warm = [
        a.temperature * 0.12 + a.tint * 0.03,
        a.temperature * 0.03 - a.tint * 0.06,
        -a.temperature * 0.12 + a.tint * 0.03,
    ];
    for (ch, table) in lut.iter_mut().enumerate() {
        for (i, v) in table.iter_mut().enumerate() {
            let x = i as f32 / 255.0;
            let mut y = (x - 0.5) * (1.0 + a.contrast) + 0.5 + a.brightness * 0.35 + warm[ch];
            // Shadows move the dark end, highlights the bright end.
            y += a.shadows * 0.3 * (1.0 - x) * (1.0 - x);
            y += a.highlights * 0.3 * x * x;
            *v = (y.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    if a.vignette != 0.0 {
        apply_vignette(f, a.vignette);
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
