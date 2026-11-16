//! Phase 2: speed, keyframes, colour adjustments, transitions and text
//! animation. All of these go through the same `render` as export.

mod common;
use engine::export::{export, ExportSettings};
use engine::media::probe;
use engine::{Adjust, Clip, Compositor, Fps, Keyframe, MediaKind, Project, TextAnim, TextLayer, TrackKind, Transform, Transition};
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

fn near(a: [u8; 3], b: [u8; 3], tol: i32) -> bool {
    a.iter().zip(b).all(|(x, y)| (*x as i32 - y as i32).abs() <= tol)
}

fn two_images(dir: &std::path::Path, kind: &str) -> Project {
    let red = solid_png(dir, "red.png", "red");
    let blue = solid_png(dir, "blue.png", "blue");
    let mut p = Project::new("t", 320, 180, Fps::new(30, 1));
    let v = p.track_mut(TrackKind::Video);
    v.clips.push(Clip::new("a", MediaKind::Image, &red, 0, 30, 0.0));
    let mut b = Clip::new("b", MediaKind::Image, &blue, 30, 30, 0.0);
    b.transition_in = Some(Transition { kind: kind.into(), frames: 10 });
    v.clips.push(b);
    p
}

#[test]
fn double_speed_shows_every_other_source_frame() {
    let dir = tempfile::tempdir().unwrap();
    let src = common::make_test_video(dir.path());
    let fps = Fps::new(30, 1);
    let mut normal = Project::new("n", 320, 180, fps);
    normal.track_mut(TrackKind::Video).clips.push(Clip::new("c", MediaKind::Video, &src, 0, 90, 0.0));
    let mut fast = Project::new("f", 320, 180, fps);
    let mut c = Clip::new("c", MediaKind::Video, &src, 0, 45, 0.0);
    c.speed = 2.0;
    fast.track_mut(TrackKind::Video).clips.push(c);

    let a = Compositor::new(None).render(&normal, 40, 320, 180).unwrap();
    let b = Compositor::new(None).render(&fast, 20, 320, 180).unwrap();
    let wrong = Compositor::new(None).render(&normal, 20, 320, 180).unwrap();
    assert!(a.psnr_luma(&b) > 35.0, "speed 2 frame 20 should match source frame 40: {}", a.psnr_luma(&b));
    assert!(a.psnr_luma(&wrong) < a.psnr_luma(&b));

    // Exported length halves, and the sound is sped up to match.
    let out = dir.path().join("fast.mp4");
    export(&fast, &mut Compositor::new(None), &ExportSettings::for_project(&fast), &out, &AtomicBool::new(false), |_, _| {})
        .unwrap();
    let info = probe(&out).unwrap();
    assert!((info.duration - 1.5).abs() < 0.1, "duration {}", info.duration);
    assert!(info.has_audio);
}

#[test]
fn keyframes_move_smoothly_between_points() {
    let mut c = Clip::new("c", MediaKind::Image, "x.png", 100, 60, 0.0);
    c.keyframes = vec![
        Keyframe { frame: 0, transform: Transform { scale: 1.0, x: 0.0, y: 0.0, opacity: 1.0, rotation: 0.0 } },
        Keyframe { frame: 30, transform: Transform { scale: 1.5, x: 0.2, y: 0.0, opacity: 1.0, rotation: 0.0 } },
    ];
    assert_eq!(c.transform_at(90).scale, 1.0);
    assert_eq!(c.transform_at(100).scale, 1.0);
    let mid = c.transform_at(115);
    assert!((mid.scale - 1.25).abs() < 1e-4 && (mid.x - 0.1).abs() < 1e-4);
    // Eased: slow at the start.
    assert!(c.transform_at(103).scale - 1.0 < 0.05 * 0.5 * 3.0);
    assert_eq!(c.transform_at(150).scale, 1.5);
}

#[test]
fn colour_adjustments_change_the_picture() {
    let dir = tempfile::tempdir().unwrap();
    let src = solid_png(dir.path(), "c.png", "0x3070C0");
    let mut p = Project::new("c", 320, 180, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("c", MediaKind::Image, &src, 0, 30, 0.0));
    let mut comp = Compositor::new(None);
    let base = px(&comp.render(&p, 0, 320, 180).unwrap(), 160, 90);

    p.tracks[0].clips[0].adjust = Adjust { saturation: -1.0, ..Default::default() };
    let grey = px(&comp.render(&p, 0, 320, 180).unwrap(), 160, 90);
    assert!(grey[0].abs_diff(grey[1]) <= 1 && grey[1].abs_diff(grey[2]) <= 1, "{grey:?}");

    p.tracks[0].clips[0].adjust = Adjust { brightness: 0.4, ..Default::default() };
    let bright = px(&comp.render(&p, 0, 320, 180).unwrap(), 160, 90);
    assert!(bright.iter().zip(base).all(|(b, a)| *b > a), "{base:?} -> {bright:?}");

    p.tracks[0].clips[0].adjust = Adjust { temperature: 1.0, ..Default::default() };
    let warm = px(&comp.render(&p, 0, 320, 180).unwrap(), 160, 90);
    assert!(warm[0] > base[0] && warm[2] < base[2]);
}

#[test]
fn transitions_blend_the_two_clips() {
    let dir = tempfile::tempdir().unwrap();
    let mut comp = Compositor::new(None);

    let p = two_images(dir.path(), "dissolve");
    assert!(near(px(&comp.render(&p, 29, 320, 180).unwrap(), 160, 90), [255, 0, 0], 3));
    let mid = px(&comp.render(&p, 35, 320, 180).unwrap(), 160, 90);
    assert!(near(mid, [128, 0, 128], 8), "dissolve midpoint {mid:?}");
    assert!(near(px(&comp.render(&p, 40, 320, 180).unwrap(), 160, 90), [0, 0, 255], 3));

    let p = two_images(dir.path(), "black");
    assert!(near(px(&comp.render(&p, 35, 320, 180).unwrap(), 160, 90), [0, 0, 0], 3));

    // Wipe goes right to left: the new clip shows on the right first.
    let p = two_images(dir.path(), "wipe");
    let f = comp.render(&p, 35, 320, 180).unwrap();
    assert!(near(px(&f, 300, 90), [0, 0, 255], 3) && near(px(&f, 20, 90), [255, 0, 0], 3));

    // Slide comes in from the left.
    let p = two_images(dir.path(), "slide");
    let f = comp.render(&p, 33, 320, 180).unwrap();
    assert!(near(px(&f, 10, 90), [0, 0, 255], 3) && near(px(&f, 310, 90), [255, 0, 0], 3));
}

fn caption(anim: &str) -> TextLayer {
    TextLayer {
        id: "t".into(),
        text: "مرحبا بكم جميعا".into(),
        font_family: "Cairo".into(),
        size: 0.12,
        weight: 700,
        color: [255, 255, 255, 255],
        x: 0.5,
        y: 0.5,
        box_width: 0.95,
        shadow: false,
        outline: 0.0,
        outline_color: [0, 0, 0, 255],
        background: [0, 0, 0, 0],
        align: "center".into(),
        start: 0,
        duration: 60,
        anim_in: Some(TextAnim { kind: anim.into(), frames: 15 }),
        anim_out: None,
    }
}

/// Columns that have white text in them.
fn ink_columns(f: &engine::Frame) -> Vec<u32> {
    (0..f.width).filter(|&x| (0..f.height).any(|y| px(f, x, y)[0] > 128)).collect()
}

#[test]
fn words_appear_one_by_one_from_the_right() {
    let mut p = Project::new("w", 640, 360, Fps::new(30, 1));
    p.texts.push(caption("words"));
    let mut comp = Compositor::new(Some(&common::fonts_dir()));
    let early = ink_columns(&comp.render(&p, 4, 640, 360).unwrap());
    let full = ink_columns(&comp.render(&p, 30, 640, 360).unwrap());
    assert!(!early.is_empty(), "the first word should be showing");
    assert!(early.len() * 2 < full.len(), "only part of the text early on");
    // Arabic reads right to left, so the first word is on the right.
    let mid = (full[0] + full[full.len() - 1]) / 2;
    assert!(early.iter().all(|&x| x > mid), "first word must be on the right half");
}

#[test]
fn pop_and_fade_animations_settle_on_the_plain_text() {
    let mut comp = Compositor::new(Some(&common::fonts_dir()));
    let mut plain = Project::new("p", 640, 360, Fps::new(30, 1));
    let mut t = caption("none");
    t.anim_in = None;
    plain.texts.push(t);
    let still = comp.render(&plain, 30, 640, 360).unwrap();
    for kind in ["pop", "fade", "rise", "zoom"] {
        let mut p = Project::new("p", 640, 360, Fps::new(30, 1));
        p.texts.push(caption(kind));
        let start = comp.render(&p, 1, 640, 360).unwrap();
        let settled = comp.render(&p, 30, 640, 360).unwrap();
        assert!(settled.psnr(&still) > 60.0, "{kind} should end exactly on the plain text");
        assert!(start.psnr(&still) < 40.0, "{kind} should look different while animating");
    }
    // Fading out at the end.
    let mut p = Project::new("p", 640, 360, Fps::new(30, 1));
    let mut t = caption("none");
    t.anim_in = None;
    t.anim_out = Some(TextAnim { kind: "fade".into(), frames: 10 });
    p.texts.push(t);
    let last = comp.render(&p, 59, 640, 360).unwrap();
    assert!(ink_columns(&last).is_empty(), "last frame of a fade-out is empty");
}

#[test]
fn reframing_a_reel_to_landscape_fills_the_new_frame() {
    let dir = tempfile::tempdir().unwrap();
    // A wide picture filling a tall frame (zoomed in), plus a small logo.
    let wide = solid_png(dir.path(), "wide.png", "red");
    let mut p = Project::new("r", 1080, 1920, Fps::new(30, 1));
    let mut bg = Clip::new("bg", MediaKind::Image, &wide, 0, 30, 0.0);
    let size = |_: &std::path::Path| Some((320u32, 180u32));
    let cover_tall = (1080.0f32 / 320.0).max(1920.0 / 180.0) / (1080.0f32 / 320.0).min(1920.0 / 180.0);
    bg.transform.scale = cover_tall;
    p.track_mut(TrackKind::Video).clips.push(bg);
    let mut logo = Clip::new("logo", MediaKind::Image, &wide, 0, 30, 0.0);
    logo.transform = Transform { scale: 0.2, x: 0.35, y: -0.4, opacity: 1.0, rotation: 0.0 };
    p.tracks.push(engine::Track { id: "v2".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, duck: false, clips: vec![logo] });

    let land = p.reframed(1920, 1080, true, size);
    assert_eq!((land.width, land.height), (1920, 1080));
    assert!((land.tracks[0].clips[0].transform.scale - 1.0).abs() < 1e-3, "16:9 picture exactly fills 16:9");
    assert_eq!(land.tracks[2].clips[0].transform.scale, 0.2, "logo untouched");
    let f = Compositor::new(None).render(&land, 0, 192, 108).unwrap();
    assert!(near(px(&f, 2, 2), [255, 0, 0], 6) && near(px(&f, 100, 100), [255, 0, 0], 6));

    // Letterboxed (scale 1) in a square becomes filled too.
    let sq = p.reframed(1080, 1080, true, size);
    let f = Compositor::new(None).render(&sq, 0, 108, 108).unwrap();
    assert!(near(px(&f, 54, 2), [255, 0, 0], 6), "top edge filled: {:?}", px(&f, 54, 2));
    // Without fill, the frame just changes shape.
    let plain = p.reframed(1080, 1080, false, size);
    assert_eq!(plain.tracks[0].clips[0].transform.scale, cover_tall);
}

#[test]
fn outgoing_video_holds_its_last_frame_during_a_transition() {
    let dir = tempfile::tempdir().unwrap();
    let src = common::make_test_video(dir.path());
    let blue = solid_png(dir.path(), "blue.png", "blue");
    let mut p = Project::new("h", 320, 180, Fps::new(30, 1));
    let v = p.track_mut(TrackKind::Video);
    // The whole 3-second file, so nothing is left after the clip's end.
    v.clips.push(Clip::new("a", MediaKind::Video, &src, 0, 90, 0.0));
    let mut b = Clip::new("b", MediaKind::Image, &blue, 90, 30, 0.0);
    b.transition_in = Some(Transition { kind: "wipe".into(), frames: 10 });
    v.clips.push(b);
    let mut comp = Compositor::new(None);
    let last = comp.render(&p, 89, 320, 180).unwrap();
    let during = comp.render(&p, 92, 320, 180).unwrap();
    // Left part still shows the outgoing clip's last frame, not black.
    let l = px(&during, 20, 90);
    assert!(l != [0, 0, 0] && near(l, px(&last, 20, 90), 4), "{l:?}");
    // Asked out of order (a jump), the result is the same.
    let again = Compositor::new(None).render(&p, 92, 320, 180).unwrap();
    assert!(near(px(&again, 20, 90), l, 2));
}
