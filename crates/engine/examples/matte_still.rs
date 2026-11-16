//! Usage: matte_still model.onnx in.png out.png — cuts the subject out of a picture.
use engine::matte::{apply_matte, Matter};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let m = Matter::load(Path::new(&a[1])).expect("model");
    let mut f = engine::media::read_still(Path::new(&a[2]), 640, 640).expect("image");
    let t = std::time::Instant::now();
    let mask = m.matte(&f).unwrap();
    eprintln!("matte took {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let _ = m.matte(&f).unwrap();
    eprintln!("second run {:?}", t.elapsed());
    apply_matte(&mut f, &mask);
    // Over a checker so the cut-out shows.
    let mut out = engine::Frame::new(f.width, f.height);
    for y in 0..out.height {
        for x in 0..out.width {
            let v = if (x / 20 + y / 20) % 2 == 0 { 90 } else { 160 };
            let i = ((y * out.width + x) * 4) as usize;
            out.data[i..i + 3].copy_from_slice(&[v, v, v]);
        }
    }
    engine::compose::draw_layer(&mut out, &f, &engine::Transform::default());
    out.save_png(Path::new(&a[3])).unwrap();
}
