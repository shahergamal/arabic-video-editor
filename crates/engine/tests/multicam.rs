//! Two cameras of the same moment are lined up by their sound.

use engine::multicam::audio_offset;
use std::path::Path;
use std::process::Command;

fn ffmpeg(args: &[&str], out: &Path) {
    assert!(Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).arg(out).status().unwrap().success());
}

#[test]
fn finds_how_far_apart_two_recordings_started() {
    let dir = tempfile::tempdir().unwrap();
    // The "room": 20 s of irregular claps over a low hum.
    let room = dir.path().join("room.wav");
    ffmpeg(
        &["-f", "lavfi", "-i", "anoisesrc=d=20:c=pink:a=0.02:seed=3", "-f", "lavfi", "-i", "sine=f=110:d=20",
          "-f", "lavfi", "-i", "anoisesrc=d=20:c=white:a=0.9:seed=9",
          "-filter_complex", "[2]volume='if(lt(mod(t*t*1.7+t*3.1,4.3),0.25),1,0)':eval=frame[claps];[0][1][claps]amix=inputs=3:normalize=0",
          "-ar", "48000"],
        &room,
    );
    // Camera A started at 0 s; camera B started 3.37 s later, quieter, as video.
    let a = dir.path().join("a.mp4");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=red:s=160x90:d=14", "-i", room.to_str().unwrap(), "-t", "14", "-c:v", "libx264", "-c:a", "aac", "-shortest"], &a);
    let b = dir.path().join("b.mov");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=blue:s=160x90:d=12", "-ss", "3.37", "-i", room.to_str().unwrap(), "-t", "12", "-af", "volume=0.4", "-c:v", "libx264", "-c:a", "aac"], &b);

    let (offset, confidence) = audio_offset(&a, &b).unwrap();
    assert!((offset - 3.37).abs() < 0.012, "offset {offset}");
    assert!(confidence > 0.3, "confidence {confidence}");
    // The other way round gives the opposite answer.
    let (back, _) = audio_offset(&b, &a).unwrap();
    assert!((back + 3.37).abs() < 0.012, "offset {back}");
}

#[test]
fn a_multicam_cut_exports_with_one_continuous_sound() {
    use engine::export::{export, ExportSettings};
    use engine::{Clip, Compositor, Fps, MediaKind, Project, TrackKind};
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.mp4");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=red:s=160x90:d=3:r=30", "-f", "lavfi", "-i", "sine=f=440:d=3", "-c:v", "libx264", "-c:a", "aac", "-shortest"], &a);
    let b = dir.path().join("b.mp4");
    ffmpeg(&["-f", "lavfi", "-i", "color=c=blue:s=160x90:d=3:r=30", "-an", "-c:v", "libx264"], &b);
    let mut p = Project::new("mc", 160, 90, Fps::new(30, 1));
    // Pictures switch from a to b at 1 s on a muted track; sound comes from a throughout.
    let v = p.track_mut(TrackKind::Video);
    v.muted = true;
    v.clips.push(Clip::new("1", MediaKind::Video, &a, 0, 30, 0.0));
    v.clips.push(Clip::new("2", MediaKind::Video, &b, 30, 30, 1.0));
    p.track_mut(TrackKind::Audio).clips.push(Clip::new("s", MediaKind::Audio, &a, 0, 60, 0.0));
    let out = dir.path().join("o.mp4");
    let mut c = Compositor::new(None);
    export(&p, &mut c, &ExportSettings::for_project(&p), &out, &std::sync::atomic::AtomicBool::new(false), |_, _| {}).unwrap();
    let info = engine::media::probe(&out).unwrap();
    assert!(info.has_audio);
    // Sound is there on both sides of the cut.
    let w = engine::media::waveform(&out, 10).unwrap();
    assert!(w[2] > 0.1 && w[7] > 0.1, "{w:?}");
}
