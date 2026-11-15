//! Arabic-first text: shaping (joined letters, harakat, mixed Arabic/English
//! direction) is done by HarfBuzz rules through cosmic-text, never by hand.

use crate::frame::Frame;
use crate::project::TextLayer;
use cosmic_text::{Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use std::collections::HashMap;
use std::path::Path;

pub struct TextRenderer {
    fonts: FontSystem,
    cache: SwashCache,
    layouts: HashMap<String, Buffer>,
}

/// One shaped glyph, for tests and diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    /// Byte range of the source text this glyph came from.
    pub start: usize,
    pub end: usize,
    pub right_to_left: bool,
    pub x: f32,
}

impl TextRenderer {
    /// Loads the computer's own fonts plus every font in `bundled_dir`.
    pub fn new(bundled_dir: Option<&Path>) -> Self {
        Self::with_dirs(bundled_dir.into_iter())
    }

    /// Loads the computer's own fonts plus every font in each folder.
    pub fn with_dirs<'a>(dirs: impl IntoIterator<Item = &'a Path>) -> Self {
        let mut fonts = FontSystem::new();
        for dir in dirs {
            fonts.db_mut().load_fonts_dir(dir);
        }
        Self { fonts, cache: SwashCache::new(), layouts: HashMap::new() }
    }

    /// Adds a font file the user dropped in.
    pub fn add_font_file(&mut self, path: &Path) -> std::io::Result<()> {
        self.fonts.db_mut().load_font_file(path)
    }

    /// All font family names available, sorted, without duplicates.
    pub fn families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .fonts
            .db()
            .faces()
            .filter_map(|f| f.families.first().map(|(n, _)| n.clone()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// (family, file) for every font loaded from a file.
    pub fn font_files(&self) -> Vec<(String, std::path::PathBuf)> {
        self.fonts
            .db()
            .faces()
            .filter_map(|f| match &f.source {
                cosmic_text::fontdb::Source::File(p) => f.families.first().map(|(n, _)| (n.clone(), p.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn has_family(&self, family: &str) -> bool {
        self.fonts.db().faces().any(|f| f.families.iter().any(|(n, _)| n == family))
    }

    fn key(text: &str, family: &str, weight: u16, px: f32, width: f32, align: Align) -> String {
        format!("{family}\u{1}{weight}\u{1}{px}\u{1}{width}\u{1}{align:?}\u{1}{text}")
    }

    fn layout(&mut self, text: &str, family: &str, weight: u16, px: f32, width: f32, align: Align) -> &mut Buffer {
        let key = Self::key(text, family, weight, px, width, align);
        let fonts = &mut self.fonts;
        self.layouts.entry(key).or_insert_with(|| {
            let mut buf = Buffer::new(fonts, Metrics::new(px, px * 1.4));
            buf.set_size(Some(width), None);
            let attrs = Attrs::new().family(Family::Name(family)).weight(Weight(weight));
            buf.set_text(text, &attrs, Shaping::Advanced, Some(align));
            buf.shape_until_scroll(fonts, false);
            buf
        })
    }

    pub fn shape(&mut self, text: &str, family: &str, px: f32) -> Vec<ShapedGlyph> {
        let buf = self.layout(text, family, 400, px, 100_000.0, Align::Center);
        let mut out = vec![];
        for run in buf.layout_runs() {
            for g in run.glyphs {
                out.push(ShapedGlyph {
                    glyph_id: g.glyph_id,
                    start: g.start,
                    end: g.end,
                    right_to_left: g.level.is_rtl(),
                    x: g.x,
                });
            }
        }
        out
    }

    /// Draws one text layer onto a frame.
    pub fn draw(&mut self, frame: &mut Frame, layer: &TextLayer) {
        if layer.text.trim().is_empty() {
            return;
        }
        let (fw, fh) = (frame.width as f32, frame.height as f32);
        let px = (layer.size * fh).max(4.0);
        let box_w = (layer.box_width * fw).max(px);
        let cx = layer.x * fw;
        let cy = layer.y * fh;
        let align = match layer.align.as_str() {
            "right" => Align::Right,
            "left" => Align::Left,
            _ => Align::Center,
        };

        let buf = self.layout(&layer.text, &layer.font_family, layer.weight, px, box_w, align);
        let mut block_h = 0f32;
        let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
        for run in buf.layout_runs() {
            block_h = block_h.max(run.line_top + run.line_height);
            for g in run.glyphs.iter() {
                min_x = min_x.min(g.x);
                max_x = max_x.max(g.x + g.w);
            }
        }
        let ox = (cx - box_w / 2.0).round() as i32;
        let oy = (cy - block_h / 2.0).round() as i32;

        // A box behind the text, hugging the lines with some padding.
        if layer.background[3] > 0 && min_x <= max_x {
            let pad = px * 0.35;
            let [r, g, b, a] = layer.background;
            let x0 = (ox as f32 + min_x - pad).round() as i32;
            let x1 = (ox as f32 + max_x + pad).round() as i32;
            let y0 = (oy as f32 - pad * 0.4).round() as i32;
            let y1 = (oy as f32 + block_h + pad * 0.4).round() as i32;
            let radius = (pad * 0.6).round() as i32;
            for y in y0..y1 {
                for x in x0..x1 {
                    if inside_rounded(x, y, x0, y0, x1, y1, radius) {
                        frame.blend(x, y, [r, g, b, 255], a);
                    }
                }
            }
        }

        // Take the layout out of the map while drawing so we can borrow the
        // font system at the same time.
        let key = Self::key(&layer.text, &layer.font_family, layer.weight, px, box_w, align);
        let mut buf = self.layouts.remove(&key).expect("layout just created");
        let [r, g, b, a] = layer.color;
        if layer.shadow {
            let off = (px * 0.06).round().max(1.0) as i32;
            buf.draw(&mut self.fonts, &mut self.cache, Color::rgba(0, 0, 0, 160), |x, y, w, h, c| {
                fill(frame, ox + x + off, oy + y + off, w, h, [0, 0, 0, 255], c.a());
            });
        }
        if layer.outline > 0.0 {
            // Draw the letters several times around a circle to build the outline.
            let radius = (layer.outline * px).max(1.0);
            let steps = ((radius * 2.0).ceil() as i32 * 4).clamp(8, 32);
            let [orr, og, ob, oa] = layer.outline_color;
            for k in 0..steps {
                let ang = k as f32 / steps as f32 * std::f32::consts::TAU;
                let (dx, dy) = ((ang.cos() * radius).round() as i32, (ang.sin() * radius).round() as i32);
                buf.draw(&mut self.fonts, &mut self.cache, Color::rgba(orr, og, ob, oa), |x, y, w, h, c| {
                    fill(frame, ox + x + dx, oy + y + dy, w, h, [orr, og, ob, 255], c.a());
                });
            }
        }
        buf.draw(&mut self.fonts, &mut self.cache, Color::rgba(r, g, b, a), |x, y, w, h, c| {
            fill(frame, ox + x, oy + y, w, h, [r, g, b, 255], c.a());
        });
        self.layouts.insert(key, buf);

        // Keep memory bounded when many different texts are edited.
        if self.layouts.len() > 256 {
            self.layouts.clear();
        }
    }
}

fn inside_rounded(x: i32, y: i32, x0: i32, y0: i32, x1: i32, y1: i32, r: i32) -> bool {
    let cx = x.clamp(x0 + r, x1 - 1 - r);
    let cy = y.clamp(y0 + r, y1 - 1 - r);
    let (dx, dy) = (x - cx, y - cy);
    dx * dx + dy * dy <= r * r
}

fn fill(frame: &mut Frame, x: i32, y: i32, w: u32, h: u32, rgb: [u8; 4], coverage: u8) {
    for dy in 0..h as i32 {
        for dx in 0..w as i32 {
            frame.blend(x + dx, y + dy, rgb, coverage);
        }
    }
}
