//! Several tracks: pictures stack in order, hidden tracks are not drawn,
//! muted tracks are not heard, and old project files still open.

mod common;
use engine::export::{export, ExportSettings};
use engine::media::probe;
use engine::{Clip, Compositor, Fps, MediaKind, Project, TrackKind, Transform};
use std::process::Command;
use std::sync::atomic::AtomicBool;

fn solid_png(dir: &std::path::Path, name: &str, color: &str) -> std::path::PathBuf {
    let out = dir.join(name);
    let ok = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("color=c={color}:s=320x180"), "-frames:v", "1"])
        .arg(&out)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    out
}

fn px(f: &engine::Frame, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.data[i], f.data[i + 1], f.data[i + 2]]
}

fn close(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= 6)
}

#[test]
fn picture_in_picture_sits_on_top_where_placed() {
    let dir = tempfile::tempdir().unwrap();
    let red = solid_png(dir.path(), "red.png", "red");
    let blue = solid_png(dir.path(), "blue.png", "blue");
    let mut p = Project::new("pip", 320, 180, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("bg", MediaKind::Image, &red, 0, 30, 0.0));
    let mut small = Clip::new("pip", MediaKind::Image, &blue, 0, 30, 0.0);
    small.transform = Transform { scale: 0.25, x: 0.25, y: -0.25, opacity: 1.0 };
    p.tracks.push(engine::Track { id: "v2".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, clips: vec![small] });

    let mut c = Compositor::new(None);
    let f = c.render(&p, 0, 320, 180).unwrap();
    // Centre of the small picture: (0.5+0.25)*320, (0.5-0.25)*180
    assert!(close(px(&f, 240, 45), [0, 0, 255]), "{:?}", px(&f, 240, 45));
    assert!(close(px(&f, 40, 150), [255, 0, 0]), "{:?}", px(&f, 40, 150));

    // Half see-through: a mix of both.
    p.tracks[2].clips[0].transform.opacity = 0.5;
    let f = c.render(&p, 0, 320, 180).unwrap();
    let m = px(&f, 240, 45);
    assert!((100..156).contains(&m[0]) && (100..156).contains(&m[2]), "{m:?}");

    // Hidden track: not drawn.
    p.tracks[2].hidden = true;
    let f = c.render(&p, 0, 320, 180).unwrap();
    assert!(close(px(&f, 240, 45), [255, 0, 0]));
}

#[test]
fn muted_track_is_not_heard() {
    let dir = tempfile::tempdir().unwrap();
    let source = common::make_test_video(dir.path());
    let mut p = Project::new("mute", 320, 180, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("c", MediaKind::Video, &source, 0, 30, 0.0));
    p.track_mut(TrackKind::Video).muted = true;
    let out = dir.path().join("o.mp4");
    let mut c = Compositor::new(None);
    export(&p, &mut c, &ExportSettings::for_project(&p), &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(!probe(&out).unwrap().has_audio);
}

#[test]
fn music_on_audio_track_is_mixed_in_with_fades() {
    let dir = tempfile::tempdir().unwrap();
    let source = common::make_test_video(dir.path());
    let music = dir.path().join("music.m4a");
    assert!(Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=220:duration=5", "-c:a", "aac"])
        .arg(&music)
        .status()
        .unwrap()
        .success());
    let mut p = Project::new("music", 320, 180, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("v", MediaKind::Video, &source, 0, 60, 0.0));
    p.track_mut(TrackKind::Video).muted = true;
    let mut m = Clip::new("m", MediaKind::Audio, &music, 0, 60, 0.0);
    m.fade_in = 15;
    m.fade_out = 15;
    p.track_mut(TrackKind::Audio).clips.push(m);
    let out = dir.path().join("o.mp4");
    let mut c = Compositor::new(None);
    export(&p, &mut c, &ExportSettings::for_project(&p), &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let info = probe(&out).unwrap();
    assert!(info.has_audio);
    assert!((info.duration - 2.0).abs() < 0.1);
    // Fade in: the first 0.1 s is much quieter than the middle.
    let w = engine::media::waveform(&out, 20).unwrap();
    assert!(w[0] < w[10] * 0.5, "{w:?}");
}

#[test]
fn version_one_projects_open_with_clips_on_a_track() {
    let json = r#"{"version":1,"name":"old","width":1080,"height":1920,"fps":{"num":30,"den":1},
        "clips":[{"id":"c","source":"a.mp4","start":0,"duration":30,"source_in":0}],"texts":[]}"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.ave");
    std::fs::write(&path, json).unwrap();
    let p = Project::load(&path).unwrap();
    assert_eq!(p.version, engine::project::FORMAT_VERSION);
    assert!(p.clips.is_empty());
    assert_eq!(p.tracks[0].clips[0].id, "c");
    assert_eq!(p.duration(), 30);
}
