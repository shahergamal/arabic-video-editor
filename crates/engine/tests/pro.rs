//! Phase 3: colour files, green screen, masks and tracking, speed ramps,
//! scopes, sound clean-up and loudness, background removal.

mod common;
use engine::compose::{matte_dir, matte_key, MatteSetup};
use engine::export::{export, ExportSettings};
use engine::lut::Lut;
use engine::media::probe;
use engine::{AudioFx, ChromaKey, Clip, Compositor, Fps, Mask, MaskPoint, MediaKind, Project, SpeedPoint, TrackKind};
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::AtomicBool;

fn ffmpeg(args: &[&str]) {
    assert!(Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().unwrap().success());
}

fn px(f: &engine::Frame, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * f.width + x) * 4) as usize;
    [f.data[i], f.data[i + 1], f.data[i + 2], f.data[i + 3]]
}

fn solid(dir: &std::path::Path, name: &str, color: &str, size: &str) -> std::path::PathBuf {
    let out = dir.join(name);
    ffmpeg(&["-f", "lavfi", "-i", &format!("color=c={color}:s={size}"), "-frames:v", "1", out.to_str().unwrap()]);
    out
}

fn one_clip(src: &std::path::Path, media: MediaKind, w: u32, h: u32, frames: u64) -> Project {
    let mut p = Project::new("t", w, h, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("c", media, src, 0, frames, 0.0));
    p
}

#[test]
fn cube_files_load_and_change_colour() {
    // A 2×2×2 table that swaps red and blue.
    let text = "TITLE \"swap\"\nLUT_3D_SIZE 2\n0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1\n";
    let lut = Lut::parse(text).unwrap();
    let m = lut.map([1.0, 0.0, 0.0]);
    assert!((m[2] - 1.0).abs() < 1e-5 && m[0].abs() < 1e-5, "{m:?}");
    let dir = tempfile::tempdir().unwrap();
    let cube = dir.path().join("swap.cube");
    std::fs::write(&cube, text).unwrap();
    let red = solid(dir.path(), "red.png", "red", "64x64");
    let mut p = one_clip(&red, MediaKind::Image, 64, 64, 10);
    p.tracks[0].clips[0].adjust.lut = Some(cube);
    let f = Compositor::new(None).render(&p, 0, 64, 64).unwrap();
    let c = px(&f, 32, 32);
    assert!(c[2] > 240 && c[0] < 15, "{c:?}");
    assert!(Lut::parse("LUT_3D_SIZE 3\n0 0 0\n").is_err(), "short tables are refused");
}

#[test]
fn green_screen_becomes_see_through() {
    let dir = tempfile::tempdir().unwrap();
    // Green background with a red box in the middle.
    let src = dir.path().join("gs.png");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=0x00D040:s=160x90", "-vf", "drawbox=x=60:y=25:w=40:h=40:c=red:t=fill", "-frames:v", "1", src.to_str().unwrap()]);
    let blue = solid(dir.path(), "blue.png", "blue", "160x90");
    let mut p = Project::new("g", 160, 90, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("bg", MediaKind::Image, &blue, 0, 10, 0.0));
    let mut fg = Clip::new("fg", MediaKind::Image, &src, 0, 10, 0.0);
    fg.chroma = Some(ChromaKey { color: [0, 208, 64], strength: 0.3, softness: 0.2 });
    p.tracks.push(engine::Track { id: "v2".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, duck: false, clips: vec![fg] });
    let f = Compositor::new(None).render(&p, 0, 160, 90).unwrap();
    assert!(px(&f, 10, 10)[2] > 200, "green replaced by the blue below: {:?}", px(&f, 10, 10));
    let mid = px(&f, 80, 45);
    assert!(mid[0] > 200 && mid[2] < 60, "red subject kept: {mid:?}");
}

fn checker(dir: &std::path::Path) -> std::path::PathBuf {
    let src = dir.join("grid.png");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=gray:s=200x100", "-vf", "geq=lum='if(mod(floor(X/3)+floor(Y/3),2),230,30)':cb=128:cr=128", "-frames:v", "1", src.to_str().unwrap()]);
    src
}

fn masked(mode: &str) -> engine::Frame {
    let dir = tempfile::tempdir().unwrap();
    let src = checker(dir.path());
    let mut p = one_clip(&src, MediaKind::Image, 200, 100, 30);
    p.tracks[0].clips[0].mask = Some(Mask {
        shape: "ellipse".into(),
        mode: mode.into(),
        x: 0.5,
        y: 0.5,
        w: 0.4,
        h: 0.4,
        feather: 0.0,
        blur: 0.05,
        path: vec![],
    });
    Compositor::new(None).render(&p, 0, 200, 100).unwrap()
}

#[test]
fn masks_keep_cut_and_blur() {
    let keep = masked("keep");
    assert!(px(&keep, 5, 5)[..3] == [0, 0, 0], "outside is gone (black below)");
    assert!((95..105).any(|x| px(&keep, x, 50)[0] > 150), "inside stays");
    let cut = masked("cut");
    assert!((95..105).all(|x| px(&cut, x, 50)[..3] == [0, 0, 0]), "inside is gone");
    assert!((0..6).any(|x| px(&cut, x, 5)[0] > 150), "outside stays");
    // Blur: inside is smoother than the sharp original, outside unchanged.
    let blur = masked("blur");
    let plain = {
        let dir = tempfile::tempdir().unwrap();
        let src = checker(dir.path());
        Compositor::new(None).render(&one_clip(&src, MediaKind::Image, 200, 100, 30), 0, 200, 100).unwrap()
    };
    let detail = |f: &engine::Frame, x0: u32, x1: u32| -> u32 {
        (x0..x1).map(|x| (px(f, x, 50)[1] as i32 - px(f, x + 1, 50)[1] as i32).unsigned_abs()).sum()
    };
    assert!(detail(&blur, 85, 115) * 3 < detail(&plain, 85, 115), "middle is blurred");
    assert_eq!(px(&blur, 5, 5), px(&plain, 5, 5), "outside untouched");
}

#[test]
fn mask_follows_its_path() {
    let m = Mask {
        shape: "rect".into(),
        mode: "blur".into(),
        x: 0.0,
        y: 0.0,
        w: 0.1,
        h: 0.1,
        feather: 0.0,
        blur: 0.03,
        path: vec![MaskPoint { frame: 0, x: 0.2, y: 0.5 }, MaskPoint { frame: 10, x: 0.8, y: 0.5 }],
    };
    assert_eq!(m.centre_at(0), (0.2, 0.5));
    let (x, _) = m.centre_at(5);
    assert!((x - 0.5).abs() < 1e-5);
    assert_eq!(m.centre_at(99), (0.8, 0.5));
}

#[test]
fn tracker_follows_a_moving_box() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("move.mp4");
    // A white box moving right at 4 px per frame over a textured background.
    ffmpeg(&[
        "-f", "lavfi", "-i", "testsrc2=size=384x216:rate=30:duration=2",
        "-f", "lavfi", "-i", "color=c=white:s=30x30:rate=30:duration=2",
        "-filter_complex", "[0]trim=end_frame=1,loop=loop=-1:size=1,setpts=N/30/TB[bg];[bg][1]overlay=x='40+n*4':y=90:shortest=1",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "14", src.to_str().unwrap(),
    ]);
    let p = one_clip(&src, MediaKind::Video, 384, 216, 40);
    let clip = &p.tracks[0].clips[0];
    let start = ((40.0 + 15.0) / 384.0, (90.0 + 15.0) / 216.0);
    let pts = engine::track::track_box(clip, 384, 216, p.fps, 0, (start.0, start.1, 0.07, 0.12), &AtomicBool::new(false), |_, _| {}).unwrap();
    assert_eq!(pts.len(), 40);
    let last = pts[39];
    let expect = (40.0 + 39.0 * 4.0 + 15.0) / 384.0;
    assert!((last.x - expect).abs() < 0.02, "x {} vs {}", last.x, expect);
    assert!((last.y - start.1).abs() < 0.02);
}

#[test]
fn speed_ramp_speeds_up_smoothly() {
    let mut c = Clip::new("c", MediaKind::Video, "x.mp4", 0, 60, 0.0);
    c.speed_ramp = vec![SpeedPoint { frame: 0, speed: 1.0 }, SpeedPoint { frame: 30, speed: 3.0 }];
    let fps = Fps::new(30, 1);
    assert!((c.speed_at(15) - 2.0).abs() < 1e-5);
    // Covered source: 1 s at average speed 2, then 1 s at speed 3.
    assert!((c.source_time(30, fps) - 2.0).abs() < 0.01, "{}", c.source_time(30, fps));
    assert!((c.source_time(60, fps) - 5.0).abs() < 0.01);

    // Rendering picks the right source frames.
    let dir = tempfile::tempdir().unwrap();
    let src = common::make_test_video(dir.path());
    let normal = one_clip(&src, MediaKind::Video, 320, 180, 90);
    let mut ramp = one_clip(&src, MediaKind::Video, 320, 180, 40);
    ramp.tracks[0].clips[0].speed_ramp = c.speed_ramp.clone();
    let want = Compositor::new(None).render(&normal, 60, 320, 180).unwrap();
    let mut comp = Compositor::new(None);
    let mut got = None;
    for i in 0..=30 {
        got = Some(comp.render(&ramp, i, 320, 180).unwrap());
    }
    assert!(want.psnr_luma(&got.unwrap()) > 35.0);
}

#[test]
fn scopes_draw_three_panels() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("bars.png");
    ffmpeg(&["-f", "lavfi", "-i", "smptebars=size=320x180", "-frames:v", "1", src.to_str().unwrap()]);
    let f = Compositor::new(None).render(&one_clip(&src, MediaKind::Image, 320, 180, 1), 0, 320, 180).unwrap();
    let s = engine::scopes::scopes(&f);
    assert_eq!((s.width, s.height), (768, 256));
    let lit = |x0: u32, x1: u32| (x0..x1).flat_map(|x| (0..256).map(move |y| (x, y))).filter(|&(x, y)| px(&s, x, y)[1] > 100).count();
    assert!(lit(0, 256) > 100 && lit(256, 512) > 100 && lit(512, 768) > 20);
}

fn loudness(path: &std::path::Path) -> f32 {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(path)
        .args(["-af", "ebur128", "-f", "null", "-"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stderr);
    let line = text.lines().rev().find(|l| l.trim_start().starts_with("I:")).expect("loudness summary");
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[test]
fn sound_clean_up_ducking_and_loudness_export() {
    let dir = tempfile::tempdir().unwrap();
    let voice = common::make_test_video(dir.path());
    let music = dir.path().join("music.m4a");
    ffmpeg(&["-f", "lavfi", "-i", "sine=frequency=220:duration=3", "-af", "volume=0.5", "-c:a", "aac", music.to_str().unwrap()]);
    let mut p = one_clip(&voice, MediaKind::Video, 320, 180, 90);
    p.tracks[0].clips[0].audio_fx = AudioFx { denoise: true, voice: true };
    let a = p.track_mut(TrackKind::Audio);
    a.duck = true;
    a.clips.push(Clip::new("m", MediaKind::Audio, &music, 0, 90, 0.0));
    p.loudness = Some(-14.0);
    let out = dir.path().join("mix.mp4");
    export(&p, &mut Compositor::new(None), &ExportSettings::for_project(&p), &out, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(probe(&out).unwrap().has_audio);
    let l = loudness(&out);
    assert!((l + 14.0).abs() < 2.0, "loudness {l} LUFS");
}

#[test]
fn ramped_clips_leave_out_their_own_sound() {
    let mut p = Project::new("r", 320, 180, Fps::new(30, 1));
    let mut c = Clip::new("c", MediaKind::Video, "x.mp4", 0, 30, 0.0);
    c.speed_ramp = vec![SpeedPoint { frame: 0, speed: 1.0 }, SpeedPoint { frame: 29, speed: 2.0 }];
    p.track_mut(TrackKind::Video).clips.push(c);
    assert!(p.audible_clips().is_empty());
}

/// Needs the small background-removal model; set AVE_TEST_MATTE_MODEL to
/// its path (u2netp.onnx) to run. Skipped otherwise.
#[test]
fn background_removal_masks_are_cached_and_reused() {
    let Ok(model) = std::env::var("AVE_TEST_MATTE_MODEL") else {
        eprintln!("skipped: AVE_TEST_MATTE_MODEL not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    // A bright disc (the "subject") on a plain dark background.
    let src = dir.path().join("subject.png");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=0x202830:s=320x320", "-vf", "geq=r='if(lt(hypot(X-160,Y-170),80),230,32)':g='if(lt(hypot(X-160,Y-170),80),160,40)':b='if(lt(hypot(X-160,Y-170),80),120,48)'", "-frames:v", "1", src.to_str().unwrap()]);
    let mut p = one_clip(&src, MediaKind::Image, 320, 320, 5);
    // Images aren't videos, so use the clip as a video source path too.
    p.tracks[0].clips[0].media = MediaKind::Video;
    let vid = dir.path().join("subject.mp4");
    ffmpeg(&["-loop", "1", "-i", src.to_str().unwrap(), "-t", "1", "-r", "30", "-pix_fmt", "yuv420p", vid.to_str().unwrap()]);
    p.tracks[0].clips[0].source = vid.clone();
    p.tracks[0].clips[0].remove_bg = Some("objects".into());

    let cache = dir.path().join("mattes");
    let mut comp = Compositor::new(None);
    comp.mattes = Some(MatteSetup::new(cache.clone(), HashMap::from([("objects".to_string(), model.into())])));
    let f = comp.render(&p, 0, 320, 320).unwrap();
    let stored = matte_dir(&cache, &vid, "objects").join(format!("{}.png", matte_key(0.0)));
    assert!(stored.exists(), "mask kept on disk");
    // Without the model, the stored mask gives the same picture.
    let mut again = Compositor::new(None);
    again.mattes = Some(MatteSetup::new(cache, HashMap::new()));
    let g = again.render(&p, 0, 320, 320).unwrap();
    assert_eq!(f.data, g.data);
}

#[test]
fn quiet_parts_are_found() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("talk.wav");
    // Tone 0–1 s, silence 1–2.5 s, tone 2.5–3.5 s.
    ffmpeg(&["-f", "lavfi", "-i", "sine=f=300:d=3.5", "-af", "volume='if(between(t,1,2.5),0,1)':eval=frame", src.to_str().unwrap()]);
    let s = engine::media::silences(&src, -40.0, 0.5).unwrap();
    assert_eq!(s.len(), 1, "{s:?}");
    assert!((s[0].0 - 1.0).abs() < 0.1 && (s[0].1 - 2.5).abs() < 0.1, "{s:?}");
}
