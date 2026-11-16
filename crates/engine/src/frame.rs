//! A picture in memory: width x height pixels, 4 bytes each (red, green, blue, alpha).

#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Frame {
    pub fn new(width: u32, height: u32) -> Self {
        let mut data = vec![0u8; (width * height * 4) as usize];
        for px in data.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Self { width, height, data }
    }

    /// A fully see-through frame, for drawing a layer on its own.
    pub fn transparent(width: u32, height: u32) -> Self {
        Self { width, height, data: vec![0u8; (width * height * 4) as usize] }
    }

    pub fn byte_len(width: u32, height: u32) -> usize {
        (width * height * 4) as usize
    }

    /// Paints `color` over the pixel at (x, y) with the given coverage (0..=255),
    /// using normal "over" blending. Pixels outside the frame are ignored.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, rgba: [u8; 4], coverage: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let a = rgba[3] as u32 * coverage as u32 / 255;
        if a == 0 {
            return;
        }
        let i = ((y as u32 * self.width + x as u32) * 4) as usize;
        let px = &mut self.data[i..i + 4];
        let inv = 255 - a;
        if px[3] == 255 {
            for c in 0..3 {
                px[c] = ((rgba[c] as u32 * a + px[c] as u32 * inv + 127) / 255) as u8;
            }
            return;
        }
        // See-through pixel underneath: proper "over" with both alphas.
        let da = px[3] as u32 * inv / 255;
        let out = a + da;
        for c in 0..3 {
            px[c] = ((rgba[c] as u32 * a + px[c] as u32 * da + out / 2) / out) as u8;
        }
        px[3] = out.min(255) as u8;
    }

    pub fn save_png(&self, path: &std::path::Path) -> crate::Result<()> {
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut enc = png::Encoder::new(file, self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| crate::Error::Ffmpeg(e.to_string()))?;
        w.write_image_data(&self.data).map_err(|e| crate::Error::Ffmpeg(e.to_string()))?;
        Ok(())
    }

    /// Peak signal-to-noise ratio between two frames of the same size, in dB.
    /// Above ~35 dB the two look the same to the eye.
    pub fn psnr(&self, other: &Frame) -> f64 {
        assert_eq!((self.width, self.height), (other.width, other.height));
        let mut sum = 0f64;
        let mut n = 0f64;
        for (a, b) in self.data.chunks_exact(4).zip(other.data.chunks_exact(4)) {
            for c in 0..3 {
                let d = a[c] as f64 - b[c] as f64;
                sum += d * d;
                n += 1.0;
            }
        }
        let mse = sum / n;
        if mse == 0.0 {
            return f64::INFINITY;
        }
        10.0 * (255.0 * 255.0 / mse).log10()
    }

    /// Like `psnr`, but on brightness only (BT.709 luma). Delivery formats
    /// store colour at half resolution, so sharp coloured edges always soften
    /// a little in any exported MP4; brightness is what shows a wrong frame,
    /// a shifted picture or missing text.
    pub fn psnr_luma(&self, other: &Frame) -> f64 {
        assert_eq!((self.width, self.height), (other.width, other.height));
        let y = |p: &[u8]| 0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64;
        let mut sum = 0f64;
        let mut n = 0f64;
        for (a, b) in self.data.chunks_exact(4).zip(other.data.chunks_exact(4)) {
            let d = y(a) - y(b);
            sum += d * d;
            n += 1.0;
        }
        let mse = sum / n;
        if mse == 0.0 {
            return f64::INFINITY;
        }
        10.0 * (255.0 * 255.0 / mse).log10()
    }
}
