//! Writing the final video. Every frame is drawn by the same `Compositor`
//! as the preview, then handed to ffmpeg to encode.

use crate::compose::Compositor;
use crate::media::{command, ffmpeg_path, probe};
use crate::project::{Clip, Project};
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
    /// "auto" (graphics card when there is one that works), "cpu", or an
    /// ffmpeg encoder name such as "h264_nvenc".
    #[serde(default = "auto")]
    pub encoder: String,
}

fn auto() -> String {
    "auto".into()
}

impl ExportSettings {
    pub fn for_project(p: &Project) -> Self {
        Self { width: p.width, height: p.height, crf: 18, encoder: "cpu".into() }
    }
}

/// Graphics-card encoders to try, best first.
const HARDWARE: &[&str] = &["h264_videotoolbox", "h264_nvenc", "h264_qsv", "h264_amf"];

/// Hardware encoders that really work on this computer: each one is asked
/// to encode a tiny test clip (listing it is not enough, the card or its
/// driver may be missing). Checked once.
pub fn hardware_encoders() -> Vec<String> {
    static FOUND: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            let listed = command(ffmpeg_path())
                .args(["-hide_banner", "-encoders"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            HARDWARE
                .iter()
                .filter(|e| listed.split_whitespace().any(|w| w == **e))
                .filter(|e| {
                    command(ffmpeg_path())
                        .args(["-v", "error", "-nostdin", "-f", "lavfi", "-i", "color=c=gray:s=256x256:r=30:d=0.2"])
                        .args(["-pix_fmt", "yuv420p", "-c:v", e, "-f", "null", "-"])
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                })
                .map(|e| e.to_string())
                .collect()
        })
        .clone()
}

/// The encoder an export will use.
pub fn pick_encoder(wanted: &str) -> String {
    match wanted {
        "cpu" | "libx264" => "libx264".into(),
        "auto" => hardware_encoders().into_iter().next().unwrap_or_else(|| "libx264".into()),
        other if hardware_encoders().iter().any(|e| e == other) => other.into(),
        _ => "libx264".into(),
    }
}

/// Quality settings for each encoder, matched roughly to x264's `crf`.
fn codec_args(encoder: &str, crf: u8) -> Vec<String> {
    let q = crf.to_string();
    let v: Vec<&str> = match encoder {
        "h264_nvenc" => vec!["-c:v", "h264_nvenc", "-preset", "p5", "-rc", "vbr", "-cq", &q, "-b:v", "0", "-profile:v", "high"],
        "h264_qsv" => vec!["-c:v", "h264_qsv", "-global_quality", &q, "-look_ahead", "1"],
        "h264_amf" => vec!["-c:v", "h264_amf", "-rc", "cqp", "-qp_i", &q, "-qp_p", &q, "-quality", "quality"],
        "h264_videotoolbox" => {
            // 0-100, higher is better; crf 18 → about 65.
            let vt = (100.0 - crf as f32 * 1.95).clamp(20.0, 95.0).round().to_string();
            return ["-c:v", "h264_videotoolbox", "-q:v", &vt, "-allow_sw", "1"].iter().map(|s| s.to_string()).collect();
        }
        _ => vec!["-c:v", "libx264", "-preset", "medium", "-crf", &q],
    };
    v.into_iter().map(String::from).collect()
}

/// The sound of the whole timeline: each clip trimmed, sped up or slowed,
/// faded and placed where it sits, then mixed. Inputs are numbered from
/// `first_input`; the result is labelled `[aout]`.
fn audio_graph(project: &Project, first_input: usize, file: &mut dyn FnMut(&Clip) -> Result<PathBuf>) -> Result<(Vec<PathBuf>, String)> {
    let mut audio_inputs: Vec<Clip> = vec![];
    let mut files = vec![];
    for c in project.audible_clips() {
        let f = file(&c)?;
        if probe(&f).map(|i| i.has_audio).unwrap_or(false) {
            audio_inputs.push(c);
            files.push(f);
        }
    }
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
    graph += &format!("[{pre}]{loud}apad=whole_dur={total_seconds:.6},atrim=duration={total_seconds:.6}[aout]");
    Ok((files, graph))
}

/// The file whose sound a clip plays: its working copy (reversed or
/// steadied) when it has one, else the original.
fn sound_file(copies: Option<&Path>, c: &Clip) -> PathBuf {
    match copies {
        Some(dir) if crate::derive::needs_copy(c) => {
            let p = crate::derive::copy_path(dir, c);
            if p.exists() {
                p
            } else {
                c.source.clone()
            }
        }
        _ => c.source.clone(),
    }
}

/// The mixed sound of the whole timeline as mono samples (for captions).
/// Empty if nothing on the timeline has sound.
pub fn mix_audio(project: &Project, sample_rate: u32) -> Result<Vec<f32>> {
    mix_audio_with(project, sample_rate, None)
}

/// Like `mix_audio`, using working copies from `copies` where they exist.
pub fn mix_audio_with(project: &Project, sample_rate: u32, copies: Option<&Path>) -> Result<Vec<f32>> {
    let (inputs, graph) = audio_graph(project, 0, &mut |c| Ok(sound_file(copies, c)))?;
    if inputs.is_empty() {
        return Ok(vec![]);
    }
    let mut cmd = command(ffmpeg_path());
    cmd.args(["-v", "error", "-nostdin"]);
    for f in &inputs {
        cmd.arg("-i").arg(f);
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
    let encoder = pick_encoder(&settings.encoder);
    match export_with(project, compositor, settings, &encoder, out, cancel, &mut progress) {
        // A graphics card can still fail on a size or setting it does not
        // take: do it again on the processor rather than give up.
        Err(Error::Ffmpeg(_)) if encoder != "libx264" && !cancel.load(Ordering::Relaxed) => {
            export_with(project, compositor, settings, "libx264", out, cancel, &mut progress)
        }
        r => r,
    }
}

#[allow(clippy::too_many_arguments)]
fn export_with(
    project: &Project,
    compositor: &mut Compositor,
    settings: &ExportSettings,
    encoder: &str,
    out: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<()> {
    let total = project.duration();
    // Working copies are made before the first frame, not in the middle.
    if compositor.copies.is_some() {
        compositor.make_copies = true;
    }
    let (w, h) = (settings.width, settings.height);
    // Write to a temporary name so a cancelled or failed export never leaves a
    // broken file with the final name.
    let partial = partial_path(out);

    let mut cmd = command(ffmpeg_path());
    cmd.args(["-v", "error", "-y", "-nostdin"])
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{w}x{h}")])
        .args(["-r", &project.fps.as_ffmpeg(), "-i", "-"]);

    let (audio_inputs, graph) = audio_graph(project, 1, &mut |c| {
        if crate::derive::needs_copy(c) && compositor.copies.is_some() {
            // Makes the copy now if it is missing.
            let keep = std::mem::replace(&mut compositor.make_copies, true);
            let f = compositor.source_file(c);
            compositor.make_copies = keep;
            f
        } else {
            Ok(c.source.clone())
        }
    })?;
    for f in &audio_inputs {
        cmd.arg("-i").arg(f);
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
        .args(codec_args(encoder, settings.crf))
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
