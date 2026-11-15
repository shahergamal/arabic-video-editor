//! HDR video comes in with normal colours instead of grey and washed out.

use engine::media::{probe, FrameReader};
use engine::Fps;
use std::path::Path;
use std::process::Command;

/// A PQ (HDR10) clip of one flat grey level, `code` in 0..1 of the 10-bit range.
fn pq_clip(dir: &Path, name: &str, code: f32) -> std::path::PathBuf {
    let out = dir.join(name);
    let v = (code * 255.0).round() as u8;
    let ok = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("color=c=0x{v:02x}{v:02x}{v:02x}:s=320x180:d=0.5")])
        .args(["-vf", "format=yuv420p10le", "-c:v", "libx264", "-pix_fmt", "yuv420p10le"])
        .args(["-color_primaries", "bt2020", "-color_trc", "smpte2084", "-colorspace", "bt2020nc"])
        .arg(&out)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    out
}

fn centre(path: &Path) -> u8 {
    let mut r = FrameReader::open(path, 0.0, 320, 180, Fps::new(30, 1)).unwrap();
    let f = r.next_frame().unwrap().unwrap();
    f.data[((90 * 320 + 160) * 4) as usize]
}

#[test]
fn pq_video_is_detected_and_tone_mapped() {
    let dir = tempfile::tempdir().unwrap();
    // About 1000 nits: a bright highlight. Shown raw it is a dull grey.
    let bright = pq_clip(dir.path(), "bright.mp4", 0.75);
    let info = probe(&bright).unwrap();
    assert_eq!(info.hdr.as_deref(), Some("pq"));
    assert!(engine::media::wants_proxy(&info));
    let shown = centre(&bright);
    assert!(shown > 215, "a 1000-nit highlight should look nearly white, got {shown}");

    // About 5 nits: a dark shadow should stay dark.
    let dark = pq_clip(dir.path(), "dark.mp4", 0.3);
    let shown = centre(&dark);
    assert!(shown < 90, "a shadow should stay dark, got {shown}");
}

#[test]
fn normal_video_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let src = common::make_test_video(dir.path());
    assert!(probe(&src).unwrap().hdr.is_none());
    assert!(engine::media::tonemap_filter(&src).is_empty());
}

mod common;

#[test]
fn hdr_proxy_is_normal_colour_and_not_converted_twice() {
    let dir = tempfile::tempdir().unwrap();
    let bright = pq_clip(dir.path(), "bright.mp4", 0.75);
    let proxy = dir.path().join("p.mp4");
    engine::media::make_proxy(&bright, &proxy).unwrap();
    assert!(probe(&proxy).unwrap().hdr.is_none());
    let (a, b) = (centre(&bright), centre(&proxy));
    assert!((a as i32 - b as i32).abs() <= 8, "original {a}, proxy {b}");
}
