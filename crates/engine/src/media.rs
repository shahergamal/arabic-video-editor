//! Reading media files through ffmpeg, which runs as a separate program.
//! Keeping it separate means a broken file can never crash the editor.

use crate::frame::Frame;
use crate::time::Fps;
use crate::{Error, Result};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

/// Where to find ffmpeg / ffprobe. The app sets these to its bundled copies;
/// otherwise the ones on PATH are used.
pub fn ffmpeg_path() -> PathBuf {
    std::env::var_os("AVE_FFMPEG").map(PathBuf::from).unwrap_or_else(|| "ffmpeg".into())
}
pub fn ffprobe_path() -> PathBuf {
    std::env::var_os("AVE_FFPROBE").map(PathBuf::from).unwrap_or_else(|| "ffprobe".into())
}

pub(crate) fn command(program: PathBuf) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MediaInfo {
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub fps: Option<Fps>,
    pub has_audio: bool,
}

pub fn probe(path: &Path) -> Result<MediaInfo> {
    let out = command(ffprobe_path())
        .args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"])
        .arg(path)
        .output()
        .map_err(Error::FfmpegMissing)?;
    if !out.status.success() {
        return Err(Error::Ffmpeg(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let video = streams.iter().find(|s| s["codec_type"] == "video");
    let has_audio = streams.iter().any(|s| s["codec_type"] == "audio");
    let duration = v["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let (width, height, fps) = match video {
        Some(s) => {
            let (mut w, mut h) = (s["width"].as_u64().unwrap_or(0) as u32, s["height"].as_u64().unwrap_or(0) as u32);
            // Phone videos are often stored sideways with a rotation flag.
            let rotated = s["side_data_list"]
                .as_array()
                .map(|l| l.iter().any(|d| d["rotation"].as_f64().map(|r| (r.abs() as i64) % 180 == 90).unwrap_or(false)))
                .unwrap_or(false);
            if rotated {
                std::mem::swap(&mut w, &mut h);
            }
            let fps = s["avg_frame_rate"].as_str().and_then(parse_rate);
            (w, h, fps)
        }
        None => (0, 0, None),
    };
    Ok(MediaInfo { width, height, duration, fps, has_audio })
}

fn parse_rate(s: &str) -> Option<Fps> {
    let (n, d) = s.split_once('/')?;
    let (n, d): (u32, u32) = (n.parse().ok()?, d.parse().ok()?);
    (n > 0 && d > 0).then_some(Fps::new(n, d))
}

/// Reads frames of one file one after another, already scaled to the project
/// size and resampled to the project frame rate (this also fixes phone videos
/// whose frame rate changes as they record).
pub struct FrameReader {
    child: Child,
    stdout: ChildStdout,
    width: u32,
    height: u32,
}

impl FrameReader {
    pub fn open(path: &Path, start_seconds: f64, width: u32, height: u32, fps: Fps) -> Result<Self> {
        Self::open_at_speed(path, start_seconds, width, height, fps, 1.0)
    }

    /// Like `open`, playing the file `speed` times faster (0.5 = slow motion).
    pub fn open_at_speed(path: &Path, start_seconds: f64, width: u32, height: u32, fps: Fps, speed: f32) -> Result<Self> {
        // Fit the picture inside the frame; the rest stays see-through so
        // tracks underneath show (black shows if there is nothing below).
        let retime = if (speed - 1.0).abs() > 1e-4 { format!("setpts=PTS/{speed:.6},") } else { String::new() };
        let filter = format!("{retime}fps={fps},{}", fit_filter(width, height), fps = fps.as_ffmpeg());
        let mut child = command(ffmpeg_path())
            .args(["-v", "error", "-nostdin"])
            .args(["-ss", &format!("{start_seconds:.6}")])
            .arg("-i")
            .arg(path)
            .args(["-an", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(Error::FfmpegMissing)?;
        let stdout = child.stdout.take().expect("piped stdout");
        Ok(Self { child, stdout, width, height })
    }

    /// The next frame, or `None` at the end of the file.
    pub fn next_frame(&mut self) -> Result<Option<Frame>> {
        let mut data = vec![0u8; Frame::byte_len(self.width, self.height)];
        let mut filled = 0;
        while filled < data.len() {
            let n = self.stdout.read(&mut data[filled..])?;
            if n == 0 {
                return Ok(None);
            }
            filled += n;
        }
        Ok(Some(Frame { width: self.width, height: self.height, data }))
    }
}

fn fit_filter(width: u32, height: u32) -> String {
    format!(
        "scale={width}:{height}:force_original_aspect_ratio=decrease:flags=bicubic,format=rgba,\
         pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black@0"
    )
}

/// Reads a still image (PNG, JPEG, WebP...) fitted to the frame.
pub fn read_still(path: &Path, width: u32, height: u32) -> Result<Frame> {
    let out = command(ffmpeg_path())
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-vf", &fit_filter(width, height), "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .output()
        .map_err(Error::FfmpegMissing)?;
    if !out.status.success() || out.stdout.len() != Frame::byte_len(width, height) {
        return Err(Error::Ffmpeg(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    Ok(Frame { width, height, data: out.stdout })
}

/// Sound level of a file over time, as `buckets` peaks between 0 and 1,
/// for drawing the waveform on the timeline.
pub fn waveform(path: &Path, buckets: usize) -> Result<Vec<f32>> {
    let out = command(ffmpeg_path())
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", "4000", "-f", "f32le", "-"])
        .output()
        .map_err(Error::FfmpegMissing)?;
    if !out.status.success() {
        return Err(Error::Ffmpeg(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    let samples: Vec<f32> = out.stdout.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    if samples.is_empty() || buckets == 0 {
        return Ok(vec![]);
    }
    let per = samples.len().div_ceil(buckets);
    Ok(samples.chunks(per).map(|c| c.iter().fold(0f32, |m, v| m.max(v.abs())).min(1.0)).collect())
}

/// A small JPEG picture of the file, for the media library.
pub fn thumbnail(path: &Path, width: u32) -> Result<Vec<u8>> {
    let info = probe(path)?;
    let at = if info.duration > 2.0 { info.duration * 0.1 } else { 0.0 };
    let out = command(ffmpeg_path())
        .args(["-v", "error", "-nostdin", "-ss", &format!("{at:.3}"), "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-vf", &format!("scale={width}:-2"), "-f", "image2", "-c:v", "mjpeg", "-q:v", "5", "-"])
        .output()
        .map_err(Error::FfmpegMissing)?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(Error::Ffmpeg(String::from_utf8_lossy(&out.stderr).into_owned()));
    }
    Ok(out.stdout)
}

impl Drop for FrameReader {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Whether a video is big enough that editing is smoother with a proxy.
pub fn wants_proxy(info: &MediaInfo) -> bool {
    info.width.max(info.height) > 1920 || info.width.min(info.height) > 1080
}

/// Makes a small, quick-to-seek copy of a video for smooth preview: the
/// short side at 540 pixels, a key frame every 10 frames, no sound. Written
/// under a temporary name and renamed when complete.
pub fn make_proxy(src: &Path, out: &Path) -> Result<()> {
    let tmp = out.with_extension("part.mp4");
    let res = command(ffmpeg_path())
        .args(["-v", "error", "-y", "-nostdin", "-i"])
        .arg(src)
        .args(["-map", "0:v:0", "-an", "-vf", "scale='if(gt(iw,ih),-2,540)':'if(gt(iw,ih),540,-2)'"])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-g", "10", "-pix_fmt", "yuv420p"])
        .arg(&tmp)
        .output()
        .map_err(Error::FfmpegMissing)?;
    if !res.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::Ffmpeg(String::from_utf8_lossy(&res.stderr).into()));
    }
    std::fs::rename(&tmp, out)?;
    Ok(())
}
