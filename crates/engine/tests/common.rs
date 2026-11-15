#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn fonts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts")
}

/// A 3-second 640x360 test video with a moving pattern and a tone.
pub fn make_test_video(dir: &Path) -> PathBuf {
    let out = dir.join("source.mp4");
    let status = Command::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(["-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30:duration=3"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=3"])
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(&out)
        .status()
        .expect("ffmpeg must be installed to run these tests");
    assert!(status.success());
    out
}
