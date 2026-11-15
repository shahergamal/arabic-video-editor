//! Timelines go to other editors and come back the same.

use engine::interchange::*;
use engine::{Clip, Fps, MediaKind, Project, TrackKind};
use std::path::{Path, PathBuf};

fn sample() -> Project {
    let mut p = Project::new("عرض شقة", 1080, 1920, Fps::new(30, 1));
    let v = p.track_mut(TrackKind::Video);
    v.clips.push(Clip::new("a", MediaKind::Video, "/media/مشهد 1.mp4", 0, 90, 2.0));
    // A one-second gap, then a sped-up clip.
    let mut fast = Clip::new("b", MediaKind::Video, "/media/b&c.mov", 120, 60, 1.5);
    fast.speed = 2.0;
    v.clips.push(fast);
    p.tracks.push(engine::Track { id: "v2".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, duck: false, clips: vec![Clip::new("logo", MediaKind::Image, "/media/logo.png", 30, 45, 0.0)] });
    let mut music = Clip::new("m", MediaKind::Audio, "/media/music.m4a", 0, 180, 10.0);
    music.volume = 0.5;
    p.track_mut(TrackKind::Audio).clips.push(music);
    p
}

/// Place, length, file and file position of every clip, track by track.
fn shape(p: &Project) -> Vec<(TrackKind, Vec<(u64, u64, PathBuf, i64)>)> {
    let mut tracks: Vec<_> = p
        .tracks
        .iter()
        .filter(|t| !t.clips.is_empty())
        .map(|t| (t.kind, t.clips.iter().map(|c| (c.start, c.duration, c.source.clone(), (c.source_in * 1000.0).round() as i64)).collect::<Vec<_>>()))
        .collect();
    tracks.sort_by_key(|(k, _)| *k == TrackKind::Audio);
    tracks
}

#[test]
fn otio_round_trip_keeps_clips_gaps_and_speed() {
    let p = sample();
    let out = to_otio(&p);
    let back = from_otio(&out.text, (1, 1, Fps::new(25, 1))).unwrap();
    assert_eq!((back.width, back.height, back.fps), (1080, 1920, Fps::new(30, 1)));
    assert_eq!(shape(&back), shape(&p));
    let fast = back.all_clips().map(|(_, c)| c).find(|c| c.start == 120).unwrap();
    assert!((fast.speed - 2.0).abs() < 1e-6);
    let music = back.all_clips().map(|(_, c)| c).find(|c| c.media == MediaKind::Audio).unwrap();
    assert!((music.volume - 0.5).abs() < 1e-6);
    // The gap is written as a Gap of 30 frames.
    assert!(out.text.contains("Gap.1"));
}

#[test]
fn fcp7_xml_round_trip_keeps_the_timeline() {
    let mut p = sample();
    // FCP 7 XML has no speed without effects, so compare normal-speed clips.
    p.tracks[0].clips[1].speed = 1.0;
    let out = to_fcp7_xml(&p);
    assert!(out.text.starts_with("<?xml"));
    let back = from_fcp7_xml(&out.text).unwrap();
    assert_eq!((back.width, back.height, back.fps), (1080, 1920, Fps::new(30, 1)));
    assert_eq!(shape(&back), shape(&p));
    // The video clip's own sound is not duplicated on import.
    assert_eq!(back.tracks.iter().filter(|t| t.kind == TrackKind::Audio).flat_map(|t| &t.clips).count(), 1);
    // Special characters are escaped and file addresses encoded.
    assert!(out.text.contains("b&amp;c.mov"));
    assert!(out.text.contains("file://localhost/media/%D9%85"));
}

#[test]
fn reads_a_premiere_style_xml_with_ntsc_and_transitions() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xmeml version="4"><sequence id="s"><name>From Premiere</name>
<rate><timebase>30</timebase><ntsc>TRUE</ntsc></rate>
<media><video><format><samplecharacteristics><width>1920</width><height>1080</height></samplecharacteristics></format>
<track>
 <clipitem id="c1"><name>A</name><start>0</start><end>100</end><in>30</in><out>130</out>
  <file id="f1"><name>A.mp4</name><pathurl>file://localhost/C:/Users/me/Videos/A%20clip.mp4</pathurl></file></clipitem>
 <transitionitem><start>90</start><end>110</end></transitionitem>
 <clipitem id="c2"><name>A</name><start>-1</start><end>200</end><in>200</in><out>300</out><file id="f1"/></clipitem>
</track></video>
<audio><track><clipitem id="c3"><start>0</start><end>100</end><in>30</in><out>130</out><file id="f1"/></clipitem></track></audio>
</media></sequence></xmeml>"#;
    let p = from_fcp7_xml(xml).unwrap();
    assert_eq!(p.fps, Fps::new(30000, 1001));
    assert_eq!((p.width, p.height), (1920, 1080));
    let v = &p.tracks[0].clips;
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].source, PathBuf::from("C:/Users/me/Videos/A clip.mp4"));
    assert_eq!((v[1].start, v[1].duration), (100, 100));
    assert!((v[0].source_in - 30.0 * 1001.0 / 30000.0).abs() < 1e-6);
    // Linked audio of the video clip is not imported twice.
    assert!(p.tracks.iter().filter(|t| t.kind == TrackKind::Audio).all(|t| t.clips.is_empty()));
}

#[test]
fn edl_lists_cuts_with_timecodes_and_file_names() {
    let p = sample();
    let out = to_edl(&p);
    let lines: Vec<&str> = out.text.lines().collect();
    assert_eq!(lines[0], "TITLE: عرض شقة");
    // First event: video with sound, 2 s into the file, at the start.
    assert!(lines.iter().any(|l| l.starts_with("001  AX       B     C        00:00:02:00 00:00:05:00 00:00:00:00 00:00:03:00")), "{}", out.text);
    // The sped-up clip uses 4 s of the file over 2 s, with a speed line.
    assert!(out.text.contains("00:00:01:15 00:00:05:15 00:00:04:00 00:00:06:00"), "{}", out.text);
    assert!(out.text.lines().any(|l| l.split_whitespace().collect::<Vec<_>>() == ["M2", "AX", "060.0", "00:00:01:15"]), "{}", out.text);
    assert!(out.text.contains("* FROM CLIP NAME: مشهد 1.mp4"));
    assert!(out.warnings.contains(&"tracks".to_string()));
}

#[test]
fn file_addresses_survive_arabic_spaces_and_windows_drives() {
    for p in ["/home/me/فيديو جديد/a#1.mp4", "C:/Users/me/My Videos/x.mov"] {
        assert_eq!(path_from_url(&file_url(Path::new(p))), PathBuf::from(p));
    }
    assert_eq!(path_from_url("file:///Users/me/a%20b.mp4"), PathBuf::from("/Users/me/a b.mp4"));
}

#[test]
fn warns_about_what_stays_behind() {
    let mut p = sample();
    p.texts.push(serde_json::from_str(r#"{"id":"t","text":"x","font_family":"Cairo","size":0.05,"color":[255,255,255,255],"x":0.5,"y":0.5,"start":0,"duration":10}"#).unwrap());
    p.tracks[0].clips[0].adjust.brightness = 0.2;
    let w = to_otio(&p).warnings;
    assert!(w.contains(&"texts".to_string()) && w.contains(&"effects".to_string()));
}
