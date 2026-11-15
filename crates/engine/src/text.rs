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
        let mut fonts = FontSystem::new();
        if let Some(dir) = bundled_dir {
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

    pub fn has_family(&self, family: &str) -> bool {
        self.fonts.db().faces().any(|f| f.families.iter().any(|(n, _)| n == family))
    }

    fn layout(&mut self, text: &str, family: &str, weight: u16, px: f32, width: f32) -> &mut Buffer {
        let key = format!("{family}\u{1}{weight}\u{1}{px}\u{1}{width}\u{1}{text}");
        let fonts = &mut self.fonts;
        self.layouts.entry(key).or_insert_with(|| {
            let mut buf = Buffer::new(fonts, Metrics::new(px, px * 1.4));
            buf.set_size(Some(width), None);
            let attrs = Attrs::new().family(Family::Name(family)).weight(Weight(weight));
            buf.set_text(text, &attrs, Shaping::Advanced, Some(Align::Center));
            buf.shape_until_scroll(fonts, false);
            buf
        })
    }

    pub fn shape(&mut self, text: &str, family: &str, px: f32) -> Vec<ShapedGlyph> {
        let buf = self.layout(text, family, 400, px, 100_000.0);
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

        let buf = self.layout(&layer.text, &layer.font_family, layer.weight, px, box_w);
        let block_h = buf
            .layout_runs()
            .map(|r| r.line_top + r.line_height)
            .fold(0f32, f32::max);
        let ox = (cx - box_w / 2.0).round() as i32;
        let oy = (cy - block_h / 2.0).round() as i32;

        // Take the layout out of the map while drawing so we can borrow the
        // font system at the same time.
        let key = format!("{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}", layer.font_family, layer.weight, px, box_w, layer.text);
        let mut buf = self.layouts.remove(&key).expect("layout just created");
        let [r, g, b, a] = layer.color;
        if layer.shadow {
            let off = (px * 0.06).round().max(1.0) as i32;
            buf.draw(&mut self.fonts, &mut self.cache, Color::rgba(0, 0, 0, 160), |x, y, w, h, c| {
                fill(frame, ox + x + off, oy + y + off, w, h, [0, 0, 0, 255], c.a());
            });
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

fn fill(frame: &mut Frame, x: i32, y: i32, w: u32, h: u32, rgb: [u8; 4], coverage: u8) {
    for dy in 0..h as i32 {
        for dx in 0..w as i32 {
            frame.blend(x + dx, y + dy, rgb, coverage);
        }
    }
}
