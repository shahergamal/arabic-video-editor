//! The automatic edit: reading the idea, looking at shots, planning, building.

mod common;
use engine::analyze::{analyze, Moment, ShotInfo};
use engine::autoedit::{build, describe, parse_plan, read_hints, rule_plan, Brief};
use engine::{MediaKind, TrackKind};
use std::process::Command;

fn ffmpeg(args: &[&str]) {
    assert!(Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().unwrap().success());
}

fn shot(name: &str, kind: MediaKind, duration: f64, good_from: f64) -> ShotInfo {
    let moments = (0..(duration * 4.0) as usize)
        .map(|i| {
            let at = i as f64 / 4.0;
            let good = at >= good_from && at < good_from + 2.0;
            Moment { at, sharp: if good { 1.0 } else { 0.2 }, bright: 0.5, motion: 0.1, shake: if good { 0.0 } else { 0.8 } }
        })
        .collect();
    ShotInfo { path: format!("/m/{name}").into(), kind, duration, width: 1920, height: 1080, has_audio: true, moments, speech: String::new() }
}

fn brief(idea: &str) -> Brief {
    Brief { idea: idea.into(), width: 1080, height: 1920, seconds: 0.0, pace: String::new(), font: "Cairo".into(), accent: [10, 60, 120, 220] }
}

#[test]
fn the_idea_says_length_shape_pace_and_contact() {
    let h = read_hints("فيديو ريلز ٣٠ ثانية لشقة للبيع، هادي وراقي، للتواصل 0100 123 4567 بعنوان «شقة الأحلام»");
    assert_eq!(h.seconds, Some(30.0));
    assert_eq!(h.size, Some((1080, 1920)));
    assert_eq!(h.pace, Some("calm"));
    assert_eq!(h.phone.as_deref(), Some("0100 123 4567"));
    assert_eq!(h.title.as_deref(), Some("شقة الأحلام"));
    let h = read_hints("youtube video, 2 min, fast and energetic");
    assert_eq!((h.seconds, h.size, h.pace), (Some(120.0), Some((1920, 1080)), Some("fast")));
    assert_eq!(read_hints("فيديو دقيقة").seconds, Some(60.0));
}

#[test]
fn rules_pick_good_parts_in_the_named_order() {
    let shots = vec![
        shot("kitchen.mp4", MediaKind::Video, 10.0, 6.0),
        shot("front.mp4", MediaKind::Video, 10.0, 1.0),
        shot("music.mp3", MediaKind::Audio, 60.0, 0.0),
    ];
    let b = Brief { seconds: 8.0, ..brief("نبدأ بالـ front وبعدين kitchen") };
    let plan = rule_plan(&b, &shots, None);
    assert_eq!(plan.shots[0].shot, 1, "front first, as the idea says");
    assert_eq!(plan.shots[1].shot, 0);
    let k = &plan.shots[1];
    assert!(k.start <= 6.0 + 1e-6 && k.start + k.length >= 8.0 - 1e-6, "covers the sharp, steady part of kitchen: {k:?}");
    let total: f64 = plan.shots.iter().map(|p| p.length).sum();
    assert!((total - 8.0).abs() < 0.05, "{total}");
    assert_eq!(plan.music, Some(2));
    // Repeated shots use a different part the second time.
    let twice: Vec<_> = plan.shots.iter().filter(|p| p.shot == 0).collect();
    if twice.len() > 1 {
        assert!((twice[0].start - twice[1].start).abs() >= twice[0].length - 1e-6);
    }
}

#[test]
fn cuts_land_on_the_beat() {
    let shots = vec![shot("a.mp4", MediaKind::Video, 20.0, 0.0), shot("b.mp4", MediaKind::Video, 20.0, 0.0), shot("song.mp3", MediaKind::Audio, 60.0, 0.0)];
    let beats = engine::beats::Beats { tempo: 100.0, times: (0..60).map(|k| 0.1 + k as f64 * 0.6).collect() };
    let plan = rule_plan(&Brief { seconds: 7.0, ..brief("سريع") }, &shots, Some(&beats));
    let mut t = 0.0;
    for p in &plan.shots[..plan.shots.len() - 1] {
        t += p.length;
        let k = t / 0.6;
        assert!((k - k.round()).abs() < 0.02, "cut at {t} is on a beat");
    }
}

#[test]
fn a_model_plan_is_checked_before_use() {
    let shots = vec![shot("a.mp4", MediaKind::Video, 5.0, 0.0), shot("m.mp3", MediaKind::Audio, 30.0, 0.0)];
    let reply = r#"Here: {"shots":[{"shot":0,"start":4.5,"length":3},{"shot":7,"start":0,"length":2},{"shot":1,"start":0,"length":2}],
        "texts":[{"text":"شقة للبيع","at":0,"length":99,"kind":"title"},{"text":"  ","at":1,"length":1,"kind":"note"}],
        "music":1,"voice":1,"pace":"slowish"}"#;
    let plan = parse_plan(reply, &shots).unwrap();
    assert_eq!(plan.shots.len(), 1, "unknown file and the sound dropped");
    assert!((plan.shots[0].start - 2.0).abs() < 1e-9, "start kept inside the file");
    assert_eq!(plan.texts.len(), 1);
    assert!(plan.texts[0].length <= 3.0 + 1e-9);
    assert_eq!((plan.music, plan.voice, plan.pace.as_str()), (Some(1), None, "fast"));
    assert!(parse_plan("no json here", &shots).is_none());
    // The description is text only and names every file.
    let d = describe(&brief("x"), &shots);
    assert!(d.contains("[0] video \"a.mp4\"") && d.contains("[1] sound \"m.mp3\""));
}

#[test]
fn the_plan_becomes_a_timeline() {
    let shots = vec![shot("a.mp4", MediaKind::Video, 10.0, 2.0), shot("p.jpg", MediaKind::Image, 0.0, 0.0), shot("music.mp3", MediaKind::Audio, 60.0, 0.0)];
    let b = Brief { seconds: 9.0, ..brief("ريلز هادي «بيت العيلة» واتصل 01234567890") };
    let plan = rule_plan(&b, &shots, None);
    let p = build(&plan, &b, &shots, "auto");
    let v = &p.tracks.iter().find(|t| t.kind == TrackKind::Video).unwrap().clips;
    assert!(v.len() >= 2);
    assert!(v.windows(2).all(|w| w[0].end() == w[1].start), "no gaps");
    assert!(v[0].transform.scale > 3.0, "wide picture fills the tall frame");
    assert!(v[1].transition_in.is_some(), "calm edit dissolves");
    let music = p.tracks.iter().find(|t| t.duck).expect("music track softens under speech");
    assert_eq!(music.clips[0].end(), p.tracks[0].clips.last().unwrap().end());
    assert!(p.texts.iter().any(|t| t.text == "بيت العيلة"));
    assert!(p.texts.iter().any(|t| t.text.contains("01234567890")));
    assert_eq!(p.loudness, Some(-14.0));
}

#[test]
fn looking_at_a_real_shot() {
    let dir = tempfile::tempdir().unwrap();
    // First second steady and sharp, then shaky and blurred.
    let src = dir.path().join("s.mp4");
    ffmpeg(&[
        "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=30:duration=4",
        "-vf", "crop=280:160:x='if(lt(t,2),20,20+18*sin(n*2.1))':y='if(lt(t,2),10,10+9*cos(n*2.9))',scale=320:180,boxblur=enable='gte(t,2)':luma_radius=6",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", src.to_str().unwrap(),
    ]);
    let s = analyze(&src).unwrap();
    assert_eq!(s.kind, MediaKind::Video);
    assert!(s.moments.len() >= 14, "{}", s.moments.len());
    let q = |from: f64, to: f64| {
        let m: Vec<f32> = s.moments.iter().filter(|m| m.at >= from && m.at < to).map(ShotInfo::quality).collect();
        m.iter().sum::<f32>() / m.len() as f32
    };
    assert!(q(0.25, 1.75) > q(2.25, 3.75) + 0.15, "good part scores higher: {} vs {}", q(0.25, 1.75), q(2.25, 3.75));
    assert!(s.best_part(1.5).0 < 1.0, "{:?}", s.best_part(1.5));
}
