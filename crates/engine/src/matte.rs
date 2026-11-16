//! Cutting a person or object out of the background, on this computer,
//! with a small neural network (U²-Net, Apache-2.0) run by `tract`, which is
//! pure Rust: nothing to install, same result on every system.

use crate::frame::Frame;
use crate::{Error, Result};
use std::path::Path;
use tract_onnx::prelude::*;

/// The network looks at a 320×320 picture and answers with a 320×320 mask.
pub const SIZE: usize = 320;

type Model = std::sync::Arc<TypedRunnableModel>;

pub struct Matter {
    model: Model,
}

fn model_err(e: impl std::fmt::Display) -> Error {
    Error::Model(e.to_string())
}

impl Matter {
    pub fn load(path: &Path) -> Result<Self> {
        let model = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(model_err)?
            .with_input_fact(0, f32::fact([1, 3, SIZE, SIZE]).into())
            .map_err(model_err)?
            .into_optimized()
            .map_err(model_err)?
            .into_runnable()
            .map_err(model_err)?;
        Ok(Self { model })
    }

    /// How much each pixel belongs to the subject (255) or the background
    /// (0), as a SIZE×SIZE mask.
    pub fn matte(&self, frame: &Frame) -> Result<Vec<u8>> {
        const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
        const STD: [f32; 3] = [0.229, 0.224, 0.225];
        let (w, h) = (frame.width as usize, frame.height as usize);
        let mut max = 1f32;
        for px in frame.data.chunks_exact(4) {
            max = max.max(px[0] as f32).max(px[1] as f32).max(px[2] as f32);
        }
        let input = tract_ndarray::Array4::from_shape_fn((1, 3, SIZE, SIZE), |(_, c, y, x)| {
            // Nearest pixel is enough at this size.
            let sx = (x * w / SIZE).min(w - 1);
            let sy = (y * h / SIZE).min(h - 1);
            let v = frame.data[(sy * w + sx) * 4 + c] as f32 / max;
            (v - MEAN[c]) / STD[c]
        });
        let out = self.model.run(tvec!(Tensor::from(input).into())).map_err(model_err)?;
        let pred = out[0].to_plain_array_view::<f32>().map_err(model_err)?;
        let vals: Vec<f32> = pred.iter().copied().collect();
        let (lo, hi) = vals.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        let span = (hi - lo).max(1e-6);
        Ok(vals.iter().map(|v| (((v - lo) / span) * 255.0).round() as u8).collect())
    }
}

/// Uses a SIZE×SIZE mask as the transparency of `frame` (smoothly enlarged).
/// Pixels that were already see-through stay see-through.
pub fn apply_matte(frame: &mut Frame, mask: &[u8]) {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let at = |x: usize, y: usize| mask[y.min(SIZE - 1) * SIZE + x.min(SIZE - 1)] as f32;
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * SIZE as f32 / h as f32 - 0.5).max(0.0);
        let (y0, ty) = (fy as usize, fy.fract());
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * SIZE as f32 / w as f32 - 0.5).max(0.0);
            let (x0, tx) = (fx as usize, fx.fract());
            let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx;
            let bottom = at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx;
            let m = top * (1.0 - ty) + bottom * ty;
            let a = &mut frame.data[(y * w + x) * 4 + 3];
            *a = ((*a as f32) * m / 255.0).round() as u8;
        }
    }
}
