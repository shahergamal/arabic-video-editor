//! Video scopes: pictures that measure the picture. A waveform (brightness
//! across the frame), an RGB histogram and a vectorscope (colour and
//! strength), side by side, each 256 pixels tall.

use crate::frame::Frame;

const H: usize = 256;

fn luma(p: &[u8]) -> f32 {
    0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32
}

/// One image, 3×256 wide and 256 tall: waveform, histogram, vectorscope.
pub fn scopes(frame: &Frame) -> Frame {
    let mut out = Frame::new((H * 3) as u32, H as u32);
    let (w, h) = (frame.width as usize, frame.height as usize);
    let put = |out: &mut Frame, x: usize, y: usize, rgb: [u8; 3], add: u32| {
        let i = (y * H * 3 + x) * 4;
        for c in 0..3 {
            let v = out.data[i + c] as u32 + rgb[c] as u32 * add / 255;
            out.data[i + c] = v.min(255) as u8;
        }
    };
    // Guide lines at 0%, 25%, 50%, 75%, 100% brightness.
    for k in 0..5 {
        let y = ((H - 1) as f32 * k as f32 / 4.0) as usize;
        for x in 0..H {
            put(&mut out, x, y, [40, 40, 48], 255);
        }
    }

    // Waveform: for each column of the frame, where its pixels' brightness lies.
    let step = (w * h / 200_000).max(1);
    let mut counts = vec![0u32; H * H];
    let mut hist = [[0u32; 256]; 3];
    let mut vec_counts = vec![0u32; H * H];
    for (n, px) in frame.data.chunks_exact(4).enumerate().step_by(step) {
        let (x, _) = (n % w, n / w);
        let col = x * H / w;
        let l = luma(px).round() as usize;
        counts[(255 - l.min(255)) * H + col] += 1;
        for c in 0..3 {
            hist[c][px[c] as usize] += 1;
        }
        // Colour difference (BT.709), centred.
        let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let cb = (b - y) / 1.8556;
        let cr = (r - y) / 1.5748;
        let vx = (128.0 + cb).clamp(0.0, 255.0) as usize;
        let vy = (128.0 - cr).clamp(0.0, 255.0) as usize;
        vec_counts[vy * H + vx] += 1;
    }
    let scale = |c: u32, k: f32| ((c as f32 * k).sqrt() * 255.0).min(255.0) as u32;
    let k_w = 4.0 * H as f32 / (frame.width as f32 * frame.height as f32 / step as f32).max(1.0);
    for y in 0..H {
        for x in 0..H {
            let c = counts[y * H + x];
            if c > 0 {
                put(&mut out, x, y, [120, 255, 140], scale(c, k_w * 8.0));
            }
        }
    }

    // Histogram: red, green and blue added together, so overlaps look white.
    let peak = hist.iter().flat_map(|h| h.iter()).copied().max().unwrap_or(1).max(1) as f32;
    let colors = [[255, 70, 70], [70, 255, 70], [80, 120, 255]];
    for c in 0..3 {
        for v in 0..256 {
            let bar = ((hist[c][v] as f32 / peak).sqrt() * (H - 1) as f32) as usize;
            for y in (H - bar)..H {
                put(&mut out, H + v, y, colors[c], 110);
            }
        }
    }

    // Vectorscope: a circle for the most saturated colours, and a skin-tone line.
    let (cx, cy) = (2 * H + H / 2, H / 2);
    for a in 0..720 {
        let t = a as f32 / 720.0 * std::f32::consts::TAU;
        let (x, y) = ((cx as f32 + t.cos() * 100.0) as usize, (cy as f32 + t.sin() * 100.0) as usize);
        put(&mut out, x, y, [60, 60, 70], 255);
    }
    for r in 0..110 {
        let t = -123f32.to_radians();
        let (x, y) = ((cx as f32 + t.cos() * r as f32) as usize, (cy as f32 + t.sin() * r as f32) as usize);
        put(&mut out, x, y, [110, 80, 60], 255);
    }
    let k_v = 40.0 / (frame.width as f32 * frame.height as f32 / step as f32).max(1.0) * H as f32;
    for y in 0..H {
        for x in 0..H {
            let c = vec_counts[y * H + x];
            if c > 0 {
                put(&mut out, 2 * H + x, y, [230, 230, 120], scale(c, k_v));
            }
        }
    }
    out
}
