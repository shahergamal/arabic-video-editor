//! Writing the final video. Every frame is drawn by the same `Compositor`
//! as the preview, then handed to ffmpeg to encode.

use crate::compose::Compositor;
use crate::media::{command, ffmpeg_path, probe};
use crate::project::Project;
use crate::{Error, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportSettings {
    pub width: u32,
    pub height: u32,
    /// x264 quality: 18 is visually lossless, 23 is the usual default.
    pub crf: u8,
}

impl ExportSettings {
    pub fn for_project(p: &Project) -> Self {
        Self { width: p.width, height: p.height, crf: 18 }
    }
}

/// The sound of the whole timeline: each clip trimmed, sped up or slowed,
/// faded and placed where it sits, then mixed. Inputs are numbered from
/// `first_input`; the result is labelled `[aout]`.
fn audio_graph(project: &Project, first_input: usize) -> (Vec<&crate::project::Clip>, String) {
    let audio_inputs: Vec<_> = project
        .audible_clips()
        .into_iter()
        .filter(|c| probe(&c.source).map(|i| i.has_audio).unwrap_or(false))
        .collect();
    let total_seconds = project.fps.frame_to_seconds(project.duration());
    let mut graph = String::new();
    let (mut music, mut speech) = (vec![], vec![]);
    for (i, clip) in audio_inputs.iter().enumerate() {
        let start = clip.source_in;
        let dur = project.fps.frame_to_seconds(clip.duration);
        let delay_ms = (project.fps.frame_to_seconds(clip.start) * 1000.0).round() as u64;
        let mut fades = String::new();
        if clip.fade_in > 0 {
            fades += &format!(",afade=t=in:st=0:d={:.6}", project.fps.frame_to_seconds(clip.fade_in));
        }
        if clip.fade_out > 0 {
            let d = project.fps.frame_to_seconds(clip.fade_out);
            fades += &format!(",afade=t=out:st={:.6}:d={d:.6}", (dur - d).max(0.0));
        }
        let mut clean = String::new();
        if clip.audio_fx.denoise {
            // Learns the steady noise and takes it out.
            clean += "afftdn=nr=18:nf=-30:tn=1,";
        }
        if clip.audio_fx.voice {
            // Cut rumble and hiss, bring the voice forward, even out loud and soft words.
            clean += "highpass=f=85,lowpass=f=14000,equalizer=f=250:t=q:w=1.2:g=-2,\
                      equalizer=f=3200:t=q:w=1.0:g=3,acompressor=threshold=-22dB:ratio=3:attack=8:release=120:makeup=3,";
        }
        let speed = clip.speed.clamp(0.1, 10.0) as f64;
        let src_dur = dur * speed;
        graph += &format!(
            "[{n}:a]atrim=start={start:.6}:duration={src_dur:.6},asetpts=PTS-STARTPTS,{tempo}\
             aresample=48000,{clean}volume={vol}{fades},adelay={delay_ms}:all=1[a{i}];",
            tempo = atempo(speed),
            n = i + first_input,
            vol = clip.volume
        );
        if project.is_ducked(&clip.id) {
            music.push(i);
        } else {
            speech.push(i);
        }
    }
    let mix = |ids: &[usize], out: &str| -> String {
        let ins: String = ids.iter().map(|i| format!("[a{i}]")).collect();
        format!("{ins}amix=inputs={}:normalize=0:duration=longest[{out}];", ids.len())
    };
    let pre = if !music.is_empty() && !speech.is_empty() {
        // Music gets quieter by itself whenever someone speaks.
        graph += &mix(&speech, "sp");
        graph += &mix(&music, "mu");
        graph += "[sp]asplit=2[sp1][sp2];[mu][sp2]sidechaincompress=threshold=0.02:ratio=10:attack=30:release=500:makeup=1[duck];";
        graph += "[sp1][duck]amix=inputs=2:normalize=0:duration=longest[mix];";
        "mix"
    } else {
        graph += &mix(if music.is_empty() { &speech } else { &music }, "mix");
        "mix"
    };
    let loud = project
        .loudness
        .map(|l| format!("loudnorm=I={l:.1}:TP=-1.5:LRA=11,aresample=48000,"))
        .unwrap_or_default();
    graph += &format!("[{pre}]{loud}apad,atrim=duration={total_seconds:.6}[aout]");
    (audio_inputs, graph)
}

/// The mixed sound of the whole timeline as mono samples (for captions).
/// Empty if nothing on the timeline has sound.
pub fn mix_audio(project: &Project, sample_rate: u32) -> Result<Vec<f32>> {
    let (inputs, graph) = audio_graph(project, 0);
    if inputs.is_empty() {
        return Ok(vec![]);
    }
    let mut cmd = command(ffmpeg_path());
    cmd.args(["-v", "error", "-nostdin"]);
    for c in &inputs {
        cmd.arg("-i").arg(&c.source);
    }
    let out = cmd
        .args(["-filter_complex", &graph, "-map", "[aout]"])
        .args(["-ac", "1", "-ar", &sample_rate.to_string(), "-f", "f32le", "-"])
        .stderr(Stdio::piped())
        .output()
        .map_err(Error::FfmpegMissing)?;
    if !out.status.success() {
        return Err(Error::Ffmpeg(String::from_utf8_lossy(&out.stderr).into()));
    }
    Ok(out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

pub fn export(
    project: &Project,
    compositor: &mut Compositor,
    settings: &ExportSettings,
    out: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<()> {
    let total = project.duration();
    let (w, h) = (settings.width, settings.height);
    // Write to a temporary name so a cancelled or failed export never leaves a
    // broken file with the final name.
    let partial = partial_path(out);

    let mut cmd = command(ffmpeg_path());
    cmd.args(["-v", "error", "-y", "-nostdin"])
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{w}x{h}")])
        .args(["-r", &project.fps.as_ffmpeg(), "-i", "-"]);

    let (audio_inputs, graph) = audio_graph(project, 1);
    for clip in &audio_inputs {
        cmd.arg("-i").arg(&clip.source);
    }
    if !audio_inputs.is_empty() {
        cmd.args(["-filter_complex", &graph, "-map", "0:v", "-map", "[aout]"])
            .args(["-c:a", "aac", "-b:a", "192k"]);
    } else {
        cmd.args(["-map", "0:v"]);
    }

    // Convert to video colours with the standard HD colour rules, and label
    // the file with them, so players show the same colours as the preview.
    cmd.args(["-vf", "scale=out_color_matrix=bt709:out_range=tv,format=yuv420p"])
        .args(["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709"])
        .args(["-c:v", "libx264", "-preset", "medium", "-crf", &settings.crf.to_string()])
        .args(["-movflags", "+faststart", "-f", "mp4"])
        .arg(&partial)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(Error::FfmpegMissing)?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let stderr = child.stderr.take().expect("piped stderr");
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut { stderr }, &mut s);
        s
    });

    let mut result = Ok(());
    for i in 0..total {
        if cancel.load(Ordering::Relaxed) {
            result = Err(Error::Cancelled);
            break;
        }
        let frame = compositor.render(project, i, w, h)?;
        if let Err(e) = stdin.write_all(&frame.data) {
            result = Err(Error::Io(e));
            break;
        }
        progress(i + 1, total);
    }
    drop(stdin);
    let status = child.wait()?;
    let errors = err_reader.join().unwrap_or_default();

    if result.is_ok() && !status.success() {
        result = Err(Error::Ffmpeg(errors));
    }
    match result {
        Ok(()) => {
            std::fs::rename(&partial, out)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn partial_path(out: &Path) -> PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    out.with_file_name(name)
}

/// ffmpeg's `atempo` takes 0.5 to 2 at a time, so bigger changes are chained.
/// Pitch stays natural.
fn atempo(speed: f64) -> String {
    if (speed - 1.0).abs() < 1e-4 {
        return String::new();
    }
    let mut out = String::new();
    let mut left = speed;
    while left > 2.0 {
        out += "atempo=2.0,";
        left /= 2.0;
    }
    while left < 0.5 {
        out += "atempo=0.5,";
        left /= 0.5;
    }
    out + &format!("atempo={left:.6},")
}
