//! Colour look files in the common ".cube" format (3D tables), as made by
//! most colour tools and cameras.

use crate::frame::Frame;
use crate::{Error, Result};
use std::path::Path;

pub struct Lut {
    size: usize,
    /// size³ entries of RGB in 0..1, red changing fastest.
    table: Vec<[f32; 3]>,
    min: [f32; 3],
    max: [f32; 3],
}

impl Lut {
    pub fn load(path: &Path) -> Result<Self> {
        Self::parse(&std::fs::read_to_string(path)?)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let bad = |m: &str| Error::Model(format!("LUT file: {m}"));
        let (mut size, mut min, mut max) = (0usize, [0f32; 3], [1f32; 3]);
        let mut table = vec![];
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let mut parts = line.split_whitespace();
            let first = parts.next().unwrap_or("");
            let nums = |p: std::str::SplitWhitespace| -> Vec<f32> { p.filter_map(|v| v.parse().ok()).collect() };
            match first {
                "LUT_3D_SIZE" => size = parts.next().and_then(|v| v.parse().ok()).ok_or_else(|| bad("size"))?,
                "DOMAIN_MIN" => min.copy_from_slice(nums(parts).get(..3).ok_or_else(|| bad("domain"))?),
                "DOMAIN_MAX" => max.copy_from_slice(nums(parts).get(..3).ok_or_else(|| bad("domain"))?),
                "TITLE" | "LUT_1D_SIZE" | "LUT_1D_INPUT_RANGE" | "LUT_3D_INPUT_RANGE" => {
                    if first == "LUT_1D_SIZE" {
                        return Err(bad("1D tables are not supported"));
                    }
                }
                _ => {
                    let v: Vec<f32> = line.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    if v.len() == 3 {
                        table.push([v[0], v[1], v[2]]);
                    }
                }
            }
        }
        if size < 2 || table.len() != size * size * size {
            return Err(bad("table size does not match"));
        }
        Ok(Self { size, table, min, max })
    }

    /// Looks up one colour (0..1), blending the 8 nearest table entries.
    pub fn map(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size - 1;
        let mut i0 = [0usize; 3];
        let mut f = [0f32; 3];
        for c in 0..3 {
            let v = ((rgb[c] - self.min[c]) / (self.max[c] - self.min[c]).max(1e-6)).clamp(0.0, 1.0) * n as f32;
            i0[c] = (v.floor() as usize).min(n.saturating_sub(1));
            f[c] = v - i0[c] as f32;
        }
        let at = |r: usize, g: usize, b: usize| self.table[r + g * self.size + b * self.size * self.size];
        let mut out = [0f32; 3];
        for (dr, wr) in [(0, 1.0 - f[0]), (1, f[0])] {
            for (dg, wg) in [(0, 1.0 - f[1]), (1, f[1])] {
                for (db, wb) in [(0, 1.0 - f[2]), (1, f[2])] {
                    let v = at(i0[0] + dr, i0[1] + dg, i0[2] + db);
                    let w = wr * wg * wb;
                    for c in 0..3 {
                        out[c] += v[c] * w;
                    }
                }
            }
        }
        out
    }

    /// Applies the look to a frame; `strength` 0..1 mixes with the original.
    pub fn apply(&self, frame: &mut Frame, strength: f32) {
        let s = strength.clamp(0.0, 1.0);
        for px in frame.data.chunks_exact_mut(4) {
            if px[3] == 0 {
                continue;
            }
            let rgb = [px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0];
            let m = self.map(rgb);
            for c in 0..3 {
                px[c] = ((rgb[c] + (m[c] - rgb[c]) * s) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}
