//! Phase 5: reverse, freeze, crop / flip / turn, blend modes, curves and
//! colour wheels, shapes, groups, beats, steadying, graphics-card export,
//! following the subject.

mod common;
use engine::compose::{curve_table, Blend};
use engine::export::{export, pick_encoder, ExportSettings};
use engine::media::probe;
use engine::{Clip, Compositor, Compound, Crop, Fps, MediaKind, Project, Shape, Track, TrackKind, Transform};
use std::process::Command;
use std::sync::atomic::AtomicBool;

fn ffmpeg(args: &[&str]) {
    assert!(Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().unwrap().success());
}

fn px(f: &engine::Frame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.data[i], f.data[i + 1], f.data[i + 2], f.data[i + 3]]
}

fn one(src: &std::path::Path, media: MediaKind, w: u32, h: u32, frames: u64) -> Project {
    let mut p = Project::new("t", w, h, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("c", media, src, 0, frames, 0.0));
    p
}

/// Left half red, right half blue, top a little lighter: every flip shows.
fn quadrants(dir: &std::path::Path) -> std::path::PathBuf {
    let out = dir.join("q.png");
    ffmpeg(&[
        "-f", "lavfi", "-i", "color=c=red:s=160x90",
        "-vf", "drawbox=x=80:y=0:w=80:h=90:c=blue:t=fill,drawbox=x=0:y=0:w=40:h=20:c=yellow:t=fill",
        "-frames:v", "1", out.to_str().unwrap(),
    ]);
    out
}

#[test]
fn reversed_clip_plays_backwards_with_its_sound() {
    let dir = tempfile::tempdir().unwrap();
    let src = common::make_test_video(dir.path());
    let fwd = one(&src, MediaKind::Video, 320, 180, 90);
    let mut rev = fwd.clone();
    rev.tracks[0].clips[0].reverse = true;
    let mut comp = Compositor::new(None);
    comp.copies = Some(dir.path().join("copies"));
    comp.make_copies = true;
    let last = Compositor::new(None).render(&fwd, 89, 320, 180).unwrap();
    let first = comp.render(&rev, 0, 320, 180).unwrap();
    assert!(first.psnr_luma(&last) > 28.0, "first reversed frame = last frame: {}", first.psnr_luma(&last));
    let mid_f = Compositor::new(None).render(&fwd, 30, 320, 180).unwrap();
    let mid_r = comp.render(&rev, 59, 320, 180).unwrap();
    assert!(mid_r.psnr_luma(&mid_f) > 28.0, "frame 59 reversed = frame 30: {}", mid_r.psnr_luma(&mid_f));
    let copy = engine::derive::copy_path(&dir.path().join("copies"), &rev.tracks[0].clips[0]);
    let info = probe(&copy).unwrap();
    assert!((info.duration - 3.0).abs() < 0.1 && info.has_audio, "{info:?}");
    // And the export uses it, sound included.
    let out = dir.path().join("rev.mp4");
    export(&rev, &mut comp, &ExportSettings { width: 320, height: 180, crf: 20, encoder: "cpu".into() }, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let i = probe(&out).unwrap();
    assert!(i.has_audio && (i.duration - 3.0).abs() < 0.15, "{i:?}");
}

#[test]
fn frozen_clip_holds_one_picture_and_is_silent() {
    let dir = tempfile::tempdir().unwrap();
    let src = common::make_test_video(dir.path());
    let mut p = one(&src, MediaKind::Video, 320, 180, 60);
    p.tracks[0].clips[0].freeze = true;
    p.tracks[0].clips[0].source_in = 1.0;
    let mut comp = Compositor::new(None);
    let a = comp.render(&p, 0, 320, 180).unwrap();
    let b = comp.render(&p, 45, 320, 180).unwrap();
    assert_eq!(a.data, b.data);
    let at1 = Compositor::new(None).render(&one(&src, MediaKind::Video, 320, 180, 90), 30, 320, 180).unwrap();
    assert!(a.psnr_luma(&at1) > 30.0);
    assert!(p.audible_clips().is_empty());
}

#[test]
fn flip_turn_and_crop() {
    let dir = tempfile::tempdir().unwrap();
    let q = quadrants(dir.path());
    let mut p = one(&q, MediaKind::Image, 160, 90, 10);
    let plain = Compositor::new(None).render(&p, 0, 160, 90).unwrap();
    assert!(px(&plain, 10, 80)[0] > 200 && px(&plain, 150, 80)[2] > 200);

    p.tracks[0].clips[0].flip_h = true;
    let f = Compositor::new(None).render(&p, 0, 160, 90).unwrap();
    assert!(px(&f, 10, 80)[2] > 200 && px(&f, 150, 80)[0] > 200, "mirrored left-right");
    assert!(px(&f, 150, 5)[1] > 200, "yellow corner moved to the right");

    p.tracks[0].clips[0].flip_h = false;
    p.tracks[0].clips[0].flip_v = true;
    let f = Compositor::new(None).render(&p, 0, 160, 90).unwrap();
    assert!(px(&f, 10, 85)[1] > 200, "yellow corner moved to the bottom: {:?}", px(&f, 10, 85));

    // Turning half a circle is the same as both flips.
    p.tracks[0].clips[0].flip_v = false;
    p.tracks[0].clips[0].transform = Transform { rotation: 180.0, ..Transform::default() };
    let turned = Compositor::new(None).render(&p, 0, 160, 90).unwrap();
    p.tracks[0].clips[0].transform = Transform::default();
    p.tracks[0].clips[0].flip_h = true;
    p.tracks[0].clips[0].flip_v = true;
    let flipped = Compositor::new(None).render(&p, 0, 160, 90).unwrap();
    assert!(turned.psnr(&flipped) > 35.0, "{}", turned.psnr(&flipped));

    // A quarter turn of a square picture keeps the corners where expected.
    let sq = dir.path().join("sq.png");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=red:s=100x100", "-vf", "drawbox=x=0:y=0:w=50:h=50:c=blue:t=fill", "-frames:v", "1", sq.to_str().unwrap()]);
    let mut s = one(&sq, MediaKind::Image, 100, 100, 10);
    s.tracks[0].clips[0].transform.rotation = 90.0;
    let f = Compositor::new(None).render(&s, 0, 100, 100).unwrap();
    assert!(px(&f, 80, 20)[2] > 200, "top-left blue turned to top-right: {:?}", px(&f, 80, 20));

    // Crop hides a quarter on the left; black shows through.
    let mut c = one(&q, MediaKind::Image, 160, 90, 10);
    c.tracks[0].clips[0].crop = Crop { left: 0.25, ..Crop::default() };
    let f = Compositor::new(None).render(&c, 0, 160, 90).unwrap();
    assert!(px(&f, 20, 60)[0] < 10, "cropped part is empty: {:?}", px(&f, 20, 60));
    assert!(px(&f, 60, 60)[0] > 200, "rest kept");
}

#[test]
fn blend_modes_mix_with_the_picture_below() {
    let dir = tempfile::tempdir().unwrap();
    let grey = dir.path().join("g.png");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=0x808080:s=32x32", "-frames:v", "1", grey.to_str().unwrap()]);
    let orange = dir.path().join("o.png");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=0xFF8000:s=32x32", "-frames:v", "1", orange.to_str().unwrap()]);
    let mk = |mode: &str| {
        let mut p = one(&grey, MediaKind::Image, 32, 32, 5);
        let mut top = Clip::new("top", MediaKind::Image, &orange, 0, 5, 0.0);
        top.blend = mode.into();
        p.tracks.push(Track { id: "v2".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, duck: false, clips: vec![top] });
        px(&Compositor::new(None).render(&p, 0, 32, 32).unwrap(), 16, 16)
    };
    let normal = mk("");
    assert!(normal[0] > 250 && (normal[1] as i32 - 128).abs() < 4);
    let mul = mk("multiply");
    assert!((mul[0] as i32 - 128).abs() < 4 && (mul[1] as i32 - 64).abs() < 4 && mul[2] < 4, "{mul:?}");
    let scr = mk("screen");
    assert!(scr[0] > 250 && (scr[1] as i32 - 192).abs() < 4 && (scr[2] as i32 - 128).abs() < 4, "{scr:?}");
    let dark = mk("darken");
    assert!((dark[0] as i32 - 128).abs() < 4 && dark[2] < 4, "{dark:?}");
    assert_eq!(Blend::parse("nonsense"), Blend::Normal);
}

#[test]
fn curves_and_wheels() {
    assert!(curve_table(&[]).is_none());
    assert!(curve_table(&[[0.3, 0.3], [0.7, 0.7]]).is_none(), "points on the diagonal change nothing");
    let t = curve_table(&[[0.5, 0.75]]).unwrap();
    assert!((t[128] - 0.75).abs() < 0.02 && t[0] < 0.01 && t[255] > 0.99);
    // Never goes down: a monotone curve.
    let s = curve_table(&[[0.25, 0.1], [0.5, 0.5], [0.75, 0.9]]).unwrap();
    assert!(s.windows(2).all(|w| w[1] >= w[0] - 1e-6));

    let dir = tempfile::tempdir().unwrap();
    let grey = dir.path().join("g.png");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=0x404040:s=32x32", "-frames:v", "1", grey.to_str().unwrap()]);
    let mut p = one(&grey, MediaKind::Image, 32, 32, 5);
    p.tracks[0].clips[0].adjust.curves.master = vec![[0.25, 0.5]];
    let c = px(&Compositor::new(None).render(&p, 0, 32, 32).unwrap(), 16, 16);
    assert!(c[0] > 110 && c[0] < 145, "dark grey lifted by the curve: {c:?}");
    let mut p = one(&grey, MediaKind::Image, 32, 32, 5);
    p.tracks[0].clips[0].adjust.lift = [0.0, 0.0, 0.8];
    let c = px(&Compositor::new(None).render(&p, 0, 32, 32).unwrap(), 16, 16);
    assert!(c[2] as i32 > c[0] as i32 + 20, "lift towards blue: {c:?}");
    assert!(!p.tracks[0].clips[0].adjust.is_neutral());
}

#[test]
fn shapes_are_drawn_with_smooth_edges() {
    let mut p = Project::new("s", 200, 100, Fps::new(30, 1));
    p.shapes.push(Shape {
        id: "r".into(),
        kind: "rect".into(),
        x: 0.5,
        y: 0.5,
        w: 0.4,
        h: 0.4,
        rotation: 0.0,
        fill: [255, 0, 0, 255],
        stroke: [255, 255, 255, 255],
        stroke_width: 0.04,
        radius: 0.2,
        shadow: false,
        start: 0,
        duration: 30,
        anim_in: None,
        anim_out: None,
    });
    let mut arrow = p.shapes[0].clone();
    arrow.id = "a".into();
    arrow.kind = "arrow".into();
    arrow.y = 0.1;
    arrow.w = 0.8;
    arrow.stroke = [0, 255, 0, 255];
    p.shapes.push(arrow);
    let f = Compositor::new(None).render(&p, 0, 200, 100).unwrap();
    assert_eq!(px(&f, 100, 50)[..3], [255, 0, 0], "inside is filled");
    assert!(px(&f, 100, 30)[1] > 200, "edge drawn in the stroke colour: {:?}", px(&f, 100, 30));
    assert_eq!(px(&f, 5, 50)[..3], [0, 0, 0], "outside untouched");
    assert!(px(&f, 100, 10)[1] > 200, "arrow shaft");
    assert!(px(&f, 26, 6)[1] > 100, "arrow head at the left end: {:?}", px(&f, 26, 6));
    // Soft edges: some pixel on the rounded corner is partly covered.
    let soft = (50..80).any(|x| {
        let r = px(&f, x, 32)[0];
        r > 20 && r < 235
    });
    assert!(soft);
    // Hidden before it starts, counted in the length.
    p.shapes[0].start = 10;
    p.shapes[0].duration = 50;
    assert_eq!(p.duration(), 60);
    let f = Compositor::new(None).render(&p, 0, 200, 100).unwrap();
    assert_eq!(px(&f, 100, 50)[..3], [0, 0, 0]);
}

#[test]
fn a_group_looks_and_sounds_like_its_clips() {
    let dir = tempfile::tempdir().unwrap();
    let src = common::make_test_video(dir.path());
    let plain = {
        let mut p = Project::new("g", 320, 180, Fps::new(30, 1));
        p.track_mut(TrackKind::Video).clips.push(Clip::new("a", MediaKind::Video, &src, 30, 60, 0.5));
        p
    };
    let mut grouped = Project::new("g", 320, 180, Fps::new(30, 1));
    let inner = Track { id: "i1".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, duck: false, clips: vec![Clip::new("a", MediaKind::Video, &src, 0, 60, 0.5)] };
    grouped.compounds.push(Compound { id: "grp".into(), name: "G".into(), tracks: vec![inner] });
    // Shows the second half of the group only.
    grouped.track_mut(TrackKind::Video).clips.push(Clip::new("g1", MediaKind::Compound, "grp", 30, 30, 1.0));
    let a = Compositor::new(None).render(&plain, 75, 320, 180).unwrap();
    let b = Compositor::new(None).render(&grouped, 45, 320, 180).unwrap();
    assert!(a.psnr(&b) > 40.0, "{}", a.psnr(&b));
    let heard = grouped.audible_clips();
    assert_eq!(heard.len(), 1);
    assert_eq!((heard[0].start, heard[0].duration), (30, 30));
    assert!((heard[0].source_in - 1.5).abs() < 1e-6);
    // For other editors the group is taken apart.
    let flat = grouped.flattened();
    assert!(flat.compounds.is_empty());
    let c = &flat.tracks[0].clips[0];
    assert_eq!((c.start, c.duration, c.media), (30, 30, MediaKind::Video));
    // Export works and has sound (the plain one too, to compare).
    let out = dir.path().join("p.mp4");
    export(&plain, &mut Compositor::new(None), &ExportSettings { width: 320, height: 180, crf: 22, encoder: "cpu".into() }, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    let out = dir.path().join("g.mp4");
    export(&grouped, &mut Compositor::new(None), &ExportSettings { width: 320, height: 180, crf: 22, encoder: "cpu".into() }, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(probe(&out).unwrap().has_audio);
}

#[test]
fn beats_of_a_click_track() {
    // 120 beats a minute: a short click every half second, plus a soft hum.
    let rate = 22050u32;
    let mut s = vec![0f32; rate as usize * 12];
    for (i, v) in s.iter_mut().enumerate() {
        *v = 0.05 * (i as f32 * 2.0 * std::f32::consts::PI * 110.0 / rate as f32).sin();
    }
    for b in 0..24 {
        let at = (0.25 + b as f64 * 0.5) * rate as f64;
        for k in 0..400 {
            let i = at as usize + k;
            s[i] += 0.8 * (-(k as f32) / 60.0).exp() * if k % 2 == 0 { 1.0 } else { -1.0 };
        }
    }
    let beats = engine::beats::detect_samples(&s, rate, 0.0);
    assert!((beats.tempo - 120.0).abs() < 3.0, "tempo {}", beats.tempo);
    assert!(beats.times.len() >= 20, "{:?}", beats.times);
    for t in &beats.times {
        let off = ((t - 0.25) / 0.5).round() * 0.5 + 0.25;
        assert!((t - off).abs() < 0.03, "beat at {t} is not on a click");
    }
}

#[test]
fn steadying_makes_a_working_copy() {
    let dir = tempfile::tempdir().unwrap();
    // A pattern that jumps around: shaky.
    let src = dir.path().join("shaky.mp4");
    ffmpeg(&[
        "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=30:duration=2",
        "-vf", "crop=280:160:x='20+16*sin(n*1.7)':y='10+9*cos(n*2.3)',scale=320:180",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", src.to_str().unwrap(),
    ]);
    let mut p = one(&src, MediaKind::Video, 320, 180, 60);
    p.tracks[0].clips[0].stabilize = 0.6;
    let clip = p.tracks[0].clips[0].clone();
    assert!(engine::derive::needs_copy(&clip));
    let cache = dir.path().join("copies");
    let path = engine::derive::ensure_copy(&cache, &clip, &AtomicBool::new(false), &mut |_| {}).unwrap();
    let info = probe(&path).unwrap();
    assert!((info.duration - 2.0).abs() < 0.1 && info.width == 320, "{info:?}");
    // Same copy found the second time, nothing made again.
    let again = engine::derive::ensure_copy(&cache, &clip, &AtomicBool::new(false), &mut |_| panic!("made twice")).unwrap();
    assert_eq!(path, again);
    let mut comp = Compositor::new(None);
    comp.copies = Some(cache);
    comp.render(&p, 10, 320, 180).unwrap();
}

#[test]
fn export_picks_a_working_encoder() {
    let e = pick_encoder("auto");
    assert!(e == "libx264" || e.starts_with("h264_"), "{e}");
    assert_eq!(pick_encoder("cpu"), "libx264");
    assert_eq!(pick_encoder("h264_made_up"), "libx264");
    let dir = tempfile::tempdir().unwrap();
    let src = common::make_test_video(dir.path());
    let p = one(&src, MediaKind::Video, 320, 180, 30);
    let out = dir.path().join("hw.mp4");
    export(&p, &mut Compositor::new(None), &ExportSettings { width: 320, height: 180, crf: 20, encoder: "auto".into() }, &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(probe(&out).unwrap().duration > 0.9);
}

#[test]
fn following_keeps_the_subject_in_a_tall_frame() {
    // A wide 1920×1080 source in a 1080×1920 frame, subject on the right.
    let path = [(0u64, 0.8f32, 0.5f32), (30, 0.8, 0.5)];
    let keys = engine::reframe::follow_keyframes(&path, (1920, 1080), 1080, 1920, 1.0);
    let t = keys[0].transform;
    assert!((t.scale - 1920.0 / 1080.0 * 1920.0 / 1080.0 / (1920.0 / 1080.0)).abs() < 0.01 || t.scale > 3.0, "fills the frame: {}", t.scale);
    // Subject at 0.8 across → picture moved left, inside the allowed room.
    let pw = 1920.0 / 1080.0 * (1080.0 / 1920.0) * t.scale * 1920.0 / 1080.0 / 1.0;
    let _ = pw;
    assert!(t.x < -0.5, "moved left: {}", t.x);
    let room = (t.scale * (1080.0 / 1920.0) * 1920.0 / 1080.0 * 1920.0 / 1080.0 - 1.0) / 2.0;
    assert!(t.x >= -room - 1e-4);
    // Jittery path becomes a few calm keyframes.
    let jitter: Vec<(u64, f32, f32)> = (0..40).map(|i| (i * 8, 0.5 + if i % 2 == 0 { 0.01 } else { -0.01 }, 0.5)).collect();
    assert!(engine::reframe::calm(&jitter, 0.03).len() <= 3);
}
