//! What you see in the preview is what gets exported, frame by frame, with the
//! sound in sync, and the source file untouched.

mod common;
use engine::export::{export, ExportSettings};
use engine::media::{probe, FrameReader};
use engine::{Clip, Compositor, Fps, MediaKind, Project, TextLayer, TrackKind};
use std::sync::atomic::AtomicBool;

#[test]
fn exported_frames_match_preview_frames() {
    let dir = tempfile::tempdir().unwrap();
    let source = common::make_test_video(dir.path());
    let source_before = std::fs::read(&source).unwrap();

    let mut p = Project::new("match", 640, 360, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("c1", MediaKind::Video, source.clone(), 0, 60, 0.5));
    p.texts.push(TextLayer {
        id: "t1".into(),
        text: "شقة للبيع - ١٢٠ متر".into(),
        font_family: "Cairo".into(),
        size: 0.08,
        weight: 700,
        color: [255, 220, 0, 255],
        x: 0.5,
        y: 0.75,
        box_width: 0.9,
        shadow: true,
        outline: 0.0,
        outline_color: [0, 0, 0, 255],
        background: [0, 0, 0, 0],
        align: "center".into(),
        start: 10,
        duration: 40,
        anim_in: None,
        anim_out: None,
    });

    let mut comp = Compositor::new(Some(&common::fonts_dir()));
    let out = dir.path().join("out.mp4");
    let cancel = AtomicBool::new(false);
    export(&p, &mut comp, &ExportSettings::for_project(&p), &out, &cancel, |_, _| {}).unwrap();

    // Length and sound.
    let info = probe(&out).unwrap();
    assert!(info.has_audio, "exported file has no sound");
    assert!((info.duration - 2.0).abs() < 0.1, "duration was {}", info.duration);

    // Compare the preview drawing with the exported file, frame by frame.
    let mut preview = Compositor::new(Some(&common::fonts_dir()));
    let mut reader = FrameReader::open(&out, 0.0, 640, 360, p.fps).unwrap();
    let mut worst = f64::INFINITY;
    for i in 0..60u64 {
        let exported = reader.next_frame().unwrap().expect("exported file ended early");
        let shown = preview.render(&p, i, 640, 360).unwrap();
        let psnr = shown.psnr_luma(&exported);
        worst = worst.min(psnr);
        if i == 30 {
            let _ = std::fs::create_dir_all(env!("CARGO_TARGET_TMPDIR"));
            shown.save_png(&std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("preview_30.png")).unwrap();
            exported.save_png(&std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("export_30.png")).unwrap();
        }
    }
    assert!(reader.next_frame().unwrap().is_none(), "exported file has extra frames");
    // Video compression always changes pixels slightly; 35 dB means the
    // difference cannot be seen. A wrong frame or missing text scores far lower.
    assert!(worst > 35.0, "worst frame matched at only {worst:.1} dB");
    eprintln!("worst frame match: {worst:.1} dB");

    assert_eq!(std::fs::read(&source).unwrap(), source_before, "source file was changed");
}

#[test]
fn a_shifted_frame_would_be_caught() {
    // Guards the test above: neighbouring frames of moving video must score
    // well below the 35 dB bar, otherwise the check would prove nothing.
    let dir = tempfile::tempdir().unwrap();
    let source = common::make_test_video(dir.path());
    let mut r = FrameReader::open(&source, 0.0, 640, 360, Fps::new(30, 1)).unwrap();
    let a = r.next_frame().unwrap().unwrap();
    let b = r.next_frame().unwrap().unwrap();
    assert!(a.psnr_luma(&b) < 30.0, "neighbouring frames scored {:.1} dB", a.psnr_luma(&b));
}

#[test]
fn cancelled_export_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let source = common::make_test_video(dir.path());
    let mut p = Project::new("cancel", 320, 180, Fps::new(30, 1));
    p.track_mut(TrackKind::Video).clips.push(Clip::new("c", MediaKind::Video, source, 0, 90, 0.0));
    let mut comp = Compositor::new(None);
    let out = dir.path().join("out.mp4");
    let cancel = AtomicBool::new(false);
    let res = export(&p, &mut comp, &ExportSettings::for_project(&p), &out, &cancel, |done, _| {
        if done == 10 {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    assert!(matches!(res, Err(engine::Error::Cancelled)));
    assert!(!out.exists());
    assert!(!dir.path().join("out.mp4.partial").exists());
}
