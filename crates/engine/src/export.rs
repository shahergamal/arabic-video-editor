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

    // Audio: each clip's own sound, trimmed and placed where the clip sits.
    let mut audio_inputs = vec![];
    for clip in &project.clips {
        if probe(&clip.source).map(|i| i.has_audio).unwrap_or(false) {
            cmd.arg("-i").arg(&clip.source);
            audio_inputs.push(clip);
        }
    }
    let total_seconds = project.fps.frame_to_seconds(total);
    if !audio_inputs.is_empty() {
        let mut graph = String::new();
        for (i, clip) in audio_inputs.iter().enumerate() {
            let start = clip.source_in;
            let dur = project.fps.frame_to_seconds(clip.duration);
            let delay_ms = (project.fps.frame_to_seconds(clip.start) * 1000.0).round() as u64;
            graph += &format!(
                "[{n}:a]atrim=start={start:.6}:duration={dur:.6},asetpts=PTS-STARTPTS,\
                 aresample=48000,volume={vol},adelay={delay_ms}:all=1[a{i}];",
                n = i + 1,
                vol = clip.volume
            );
        }
        for i in 0..audio_inputs.len() {
            graph += &format!("[a{i}]");
        }
        graph += &format!(
            "amix=inputs={}:normalize=0:duration=longest,apad,atrim=duration={total_seconds:.6}[aout]",
            audio_inputs.len()
        );
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
