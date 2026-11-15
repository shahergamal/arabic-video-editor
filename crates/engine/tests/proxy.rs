//! Big videos get a small "proxy" copy for smooth preview. The preview made
//! from the proxy must look like the one made from the original.

use engine::media::{make_proxy, probe, wants_proxy};
use engine::{Clip, Compositor, Fps, MediaKind, Project, TrackKind};
use std::process::Command;

#[test]
fn proxy_preview_matches_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("big.mp4");
    let ok = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=2560x1440:rate=30:duration=2"])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"])
        .arg(&big)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    assert!(wants_proxy(&probe(&big).unwrap()));
    let proxy = dir.path().join("big.proxy.mp4");
    make_proxy(&big, &proxy).unwrap();
    let info = probe(&proxy).unwrap();
    assert_eq!((info.width, info.height), (960, 540));
    assert!((info.duration - 2.0).abs() < 0.1);

    let mut p = Project::new("p", 1920, 1080, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("c", MediaKind::Video, &big, 0, 60, 0.5));
    let original = Compositor::new(None).render(&p, 20, 480, 270).unwrap();
    let mut with_proxy = Compositor::new(None);
    with_proxy.proxies.insert(big.clone(), proxy);
    let fast = with_proxy.render(&p, 20, 480, 270).unwrap();
    let psnr = original.psnr_luma(&fast);
    assert!(psnr > 28.0, "proxy preview differs too much: {psnr:.1} dB");
}
