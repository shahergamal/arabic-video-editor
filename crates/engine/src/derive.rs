//! Working copies of a source file: steadied (stabilised) and/or reversed.
//! They are made once, kept in a cache folder, and used by the preview and
//! the export alike. The original file is never touched.

use crate::media::{command, ffmpeg_path, probe, tonemap_filter};
use crate::project::Clip;
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

/// Length of the pieces a file is reversed in. Reversing needs the whole
/// piece in memory, so pieces stay short.
const PIECE: f64 = 2.0;

/// Stabilisation strength, rounded so small slider moves reuse a copy.
fn level(stabilize: f32) -> u32 {
    (stabilize.clamp(0.0, 1.0) * 10.0).round() as u32
}

/// Whether a clip needs a working copy.
pub fn needs_copy(clip: &Clip) -> bool {
    clip.is_file() && clip.media == crate::MediaKind::Video && (clip.reverse || level(clip.stabilize) > 0)
}

/// Where the working copy for this clip lives (it may not exist yet).
pub fn copy_path(cache: &Path, clip: &Clip) -> PathBuf {
    path_for(cache, &clip.source, level(clip.stabilize), clip.reverse)
}

fn path_for(cache: &Path, src: &Path, stab: u32, reverse: bool) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    if let Ok(m) = std::fs::metadata(src) {
        m.len().hash(&mut h);
        m.modified().ok().hash(&mut h);
    }
    let mut name = format!("{:016x}", h.finish());
    if stab > 0 {
        name += &format!("-s{stab}");
    }
    if reverse {
        name += "-r";
    }
    cache.join(name + ".mkv")
}

/// Makes the working copy if it is missing and returns its path.
/// `progress` gets a share from 0 to 1.
pub fn ensure_copy(cache: &Path, clip: &Clip, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<PathBuf> {
    let stab = level(clip.stabilize);
    let out = path_for(cache, &clip.source, stab, clip.reverse);
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(cache)?;
    let mut src = clip.source.clone();
    let both = stab > 0 && clip.reverse;
    if stab > 0 {
        let steady = path_for(cache, &clip.source, stab, false);
        if !steady.exists() {
            stabilize(&clip.source, &steady, stab, cancel, &mut |p| progress(if both { p * 0.5 } else { p }))?;
        }
        src = steady;
    }
    if clip.reverse {
        reverse(&src, &out, cancel, &mut |p| progress(if both { 0.5 + p * 0.5 } else { p }))?;
    }
    progress(1.0);
    Ok(out)
}

fn has_filter(name: &str) -> bool {
    command(ffmpeg_path())
        .args(["-hide_banner", "-filters"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().any(|w| w == name))
        .unwrap_or(false)
}

/// Codec settings for working copies: near-lossless, quick to seek.
fn copy_codec(cmd: &mut std::process::Command, has_audio: bool) {
    cmd.args(["-c:v", "libx264", "-preset", "fast", "-crf", "14", "-g", "15", "-pix_fmt", "yuv420p"]);
    if has_audio {
        cmd.args(["-c:a", "pcm_s16le"]);
    }
}

fn run(cmd: &mut std::process::Command, cancel: &AtomicBool) -> Result<()> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(Error::FfmpegMissing)?;
    let stderr = child.stderr.take().expect("piped");
    let errors = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut { stderr }, &mut s);
        s
    });
    loop {
        if let Some(status) = child.try_wait()? {
            let text = errors.join().unwrap_or_default();
            return if status.success() { Ok(()) } else { Err(Error::Ffmpeg(text)) };
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Cancelled);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn tagged(cmd: &mut std::process::Command, tonemap: &str) {
    if !tonemap.is_empty() {
        cmd.args(["-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709"]);
    }
}

/// Two passes with vid.stab: measure the shake, then move each picture
/// back along a smoothed path, zooming in just enough to hide the edges.
/// Falls back to ffmpeg's simpler `deshake` when vid.stab is missing.
fn stabilize(src: &Path, out: &Path, level: u32, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<()> {
    let info = probe(src)?;
    let tonemap = tonemap_filter(src);
    let tmp = out.with_extension("part.mkv");
    let smoothing = 4 + level * 5;
    let vf = if has_filter("vidstabdetect") {
        let trf = out.with_extension("trf");
        // The transforms file path goes inside a filter string: keep it plain.
        let trf_s = trf.to_string_lossy().replace('\\', "/").replace(':', "\\:");
        progress(0.05);
        run(
            command(ffmpeg_path())
                .args(["-v", "error", "-y", "-nostdin", "-i"])
                .arg(src)
                .args(["-vf", &format!("{tonemap}vidstabdetect=shakiness={}:accuracy=12:result='{trf_s}'", (level / 2 + 4).min(10))])
                .args(["-f", "null", "-"]),
            cancel,
        )?;
        progress(0.5);
        format!("{tonemap}vidstabtransform=input='{trf_s}':smoothing={smoothing}:optzoom=1:interpol=bicubic,unsharp=5:5:0.6:3:3:0.3")
    } else {
        format!("{tonemap}deshake=rx=48:ry=48")
    };
    let mut cmd = command(ffmpeg_path());
    cmd.args(["-v", "error", "-y", "-nostdin", "-i"]).arg(src).args(["-map", "0:v:0", "-map", "0:a:0?", "-vf", &vf]);
    copy_codec(&mut cmd, info.has_audio);
    tagged(&mut cmd, &tonemap);
    cmd.arg(&tmp);
    if let Err(e) = run(&mut cmd, cancel) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    let _ = std::fs::remove_file(out.with_extension("trf"));
    std::fs::rename(&tmp, out)?;
    progress(1.0);
    Ok(())
}

/// Reverses picture and sound. The file is cut into short pieces, each
/// piece is reversed, and the pieces are joined last to first.
fn reverse(src: &Path, out: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Result<()> {
    let info = probe(src)?;
    let tonemap = tonemap_filter(src);
    let dir = out.with_extension("pieces");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let count = (info.duration / PIECE).ceil().max(1.0) as usize;
    let mut list = String::new();
    let result = (|| -> Result<()> {
        for k in 0..count {
            let piece = dir.join(format!("{k:05}.mkv"));
            let mut cmd = command(ffmpeg_path());
            cmd.args(["-v", "error", "-y", "-nostdin", "-ss", &format!("{:.6}", k as f64 * PIECE), "-t", &format!("{PIECE:.6}"), "-i"])
                .arg(src)
                .args(["-map", "0:v:0", "-map", "0:a:0?", "-vf", &format!("{tonemap}reverse")]);
            if info.has_audio {
                cmd.args(["-af", "areverse"]);
            }
            copy_codec(&mut cmd, info.has_audio);
            tagged(&mut cmd, &tonemap);
            cmd.arg(&piece);
            run(&mut cmd, cancel)?;
            progress((k + 1) as f32 / (count + 1) as f32);
        }
        for k in (0..count).rev() {
            let name = format!("{k:05}.mkv");
            list += &format!("file '{name}'\n");
        }
        let list_file = dir.join("list.txt");
        std::fs::write(&list_file, &list)?;
        let tmp = out.with_extension("part.mkv");
        run(
            command(ffmpeg_path())
                .args(["-v", "error", "-y", "-nostdin", "-f", "concat", "-safe", "0", "-i"])
                .arg(&list_file)
                .args(["-c", "copy"])
                .arg(&tmp),
            cancel,
        )?;
        std::fs::rename(&tmp, out)?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
