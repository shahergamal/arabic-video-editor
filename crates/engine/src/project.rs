//! The project file. It only describes edits (which part of which file goes
//! where, which text appears when). Source videos are never modified.

use crate::time::Fps;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: Fps,
    /// Tracks. Video tracks are stacked in list order: the first is at the
    /// bottom, later ones are drawn on top (picture-in-picture, logos).
    #[serde(default)]
    pub tracks: Vec<Track>,
    /// Text layers, drawn above all video in list order.
    pub texts: Vec<TextLayer>,
    /// Version 1 files kept clips here; they are moved onto a track on load.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<Clip>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Video,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub locked: bool,
    pub clips: Vec<Clip>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    #[default]
    Video,
    Image,
    Audio,
}

/// Where a picture sits in the frame. The default fills the frame the way
/// the source fits (no stretching).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Transform {
    /// 1.0 = fitted to the frame.
    pub scale: f32,
    /// Offset of the centre, as a share of the frame (0 = centred).
    pub x: f32,
    pub y: f32,
    pub opacity: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self { scale: 1.0, x: 0.0, y: 0.0, opacity: 1.0 }
    }
}

impl Transform {
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Clip {
    pub id: String,
    pub source: PathBuf,
    /// Where the clip starts on the timeline, in project frames.
    pub start: u64,
    /// How long it plays, in project frames.
    pub duration: u64,
    /// How far into the source file the clip begins, in seconds.
    pub source_in: f64,
    #[serde(default = "one")]
    pub volume: f32,
    #[serde(default)]
    pub media: MediaKind,
    #[serde(default)]
    pub transform: Transform,
    /// Sound fade in / out, in frames.
    #[serde(default)]
    pub fade_in: u64,
    #[serde(default)]
    pub fade_out: u64,
}

impl Clip {
    pub fn new(id: &str, media: MediaKind, source: impl Into<PathBuf>, start: u64, duration: u64, source_in: f64) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            start,
            duration,
            source_in,
            volume: 1.0,
            media,
            transform: Transform::default(),
            fade_in: 0,
            fade_out: 0,
        }
    }

    pub fn end(&self) -> u64 {
        self.start + self.duration
    }
    pub fn covers(&self, frame: u64) -> bool {
        frame >= self.start && frame < self.end()
    }
    pub fn has_picture(&self) -> bool {
        self.media != MediaKind::Audio
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextLayer {
    pub id: String,
    pub text: String,
    pub font_family: String,
    /// Font size as a share of the frame height (0.06 = 6% of the height),
    /// so the same project looks the same at any export size.
    pub size: f32,
    #[serde(default = "normal_weight")]
    pub weight: u16,
    pub color: [u8; 4],
    /// Centre of the text box, as a share of the frame (0.5, 0.5 = middle).
    pub x: f32,
    pub y: f32,
    /// Width of the text box as a share of the frame width; lines wrap inside it.
    #[serde(default = "default_box_width")]
    pub box_width: f32,
    #[serde(default)]
    pub shadow: bool,
    /// Outline around the letters, thickness as a share of the font size (0 = none).
    #[serde(default)]
    pub outline: f32,
    #[serde(default = "black")]
    pub outline_color: [u8; 4],
    /// A box behind the text (alpha 0 = no box).
    #[serde(default)]
    pub background: [u8; 4],
    /// Horizontal alignment: "center", "right" or "left".
    #[serde(default = "center")]
    pub align: String,
    pub start: u64,
    pub duration: u64,
}

fn black() -> [u8; 4] {
    [0, 0, 0, 255]
}
fn center() -> String {
    "center".into()
}
fn one() -> f32 {
    1.0
}
fn normal_weight() -> u16 {
    400
}
fn default_box_width() -> f32 {
    0.9
}

impl Project {
    pub fn new(name: &str, width: u32, height: u32, fps: Fps) -> Self {
        Self {
            version: FORMAT_VERSION,
            name: name.to_string(),
            width,
            height,
            fps,
            tracks: vec![
                Track { id: "v1".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, clips: vec![] },
                Track { id: "a1".into(), kind: TrackKind::Audio, muted: false, hidden: false, locked: false, clips: vec![] },
            ],
            texts: vec![],
            clips: vec![],
        }
    }

    /// Brings an older project file up to the current format.
    pub fn migrate(mut self) -> Self {
        if !self.clips.is_empty() {
            let clips = std::mem::take(&mut self.clips);
            match self.tracks.iter_mut().find(|t| t.kind == TrackKind::Video) {
                Some(t) => t.clips.extend(clips),
                None => self.tracks.insert(
                    0,
                    Track { id: "v1".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, clips },
                ),
            }
        }
        if self.tracks.is_empty() {
            self.tracks = Project::new("", 0, 0, self.fps).tracks;
        }
        self.version = FORMAT_VERSION;
        self
    }

    pub fn all_clips(&self) -> impl Iterator<Item = (&Track, &Clip)> {
        self.tracks.iter().flat_map(|t| t.clips.iter().map(move |c| (t, c)))
    }

    /// Total length in frames: the end of the last clip or text.
    pub fn duration(&self) -> u64 {
        let clips = self.all_clips().map(|(_, c)| c.end()).chain(self.clips.iter().map(Clip::end));
        let texts = self.texts.iter().map(|t| t.start + t.duration);
        clips.chain(texts).max().unwrap_or(0)
    }

    /// The picture clips showing at `frame`, bottom to top.
    pub fn pictures_at(&self, frame: u64) -> Vec<&Clip> {
        self.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video && !t.hidden)
            .filter_map(|t| t.clips.iter().rev().find(|c| c.has_picture() && c.covers(frame)))
            .collect()
    }

    /// Every clip whose sound is heard in the export.
    pub fn audible_clips(&self) -> Vec<&Clip> {
        self.tracks.iter().filter(|t| !t.muted).flat_map(|t| t.clips.iter()).filter(|c| c.media != MediaKind::Image).collect()
    }

    pub fn load(path: &Path) -> crate::Result<Self> {
        let text = fs::read_to_string(path)?;
        let p: Project = serde_json::from_str(&text)?;
        Ok(p.migrate())
    }

    /// Saves without ever leaving a half-written file behind:
    /// write a temporary file, flush it to disk, keep the previous version as
    /// `<name>.bak`, then swap the new file into place in one step.
    pub fn save(&self, path: &Path) -> crate::Result<()> {
        let json = serde_json::to_vec_pretty(self)?;
        let tmp = sibling(path, "tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        if path.exists() {
            fs::copy(path, sibling(path, "bak"))?;
        }
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Opens a project; if the main file is damaged, falls back to the backup.
    pub fn load_or_recover(path: &Path) -> crate::Result<(Self, bool)> {
        match Self::load(path) {
            Ok(p) => Ok((p, false)),
            Err(first) => match Self::load(&sibling(path, "bak")) {
                Ok(p) => Ok((p, true)),
                Err(_) => Err(first),
            },
        }
    }
}

fn sibling(path: &Path, ext: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(ext);
    path.with_file_name(name)
}

impl Project {
    pub fn track_mut(&mut self, kind: TrackKind) -> &mut Track {
        let i = match self.tracks.iter().position(|t| t.kind == kind) {
            Some(i) => i,
            None => {
                let id = format!("{}{}", if kind == TrackKind::Video { "v" } else { "a" }, self.tracks.len() + 1);
                self.tracks.push(Track { id, kind, muted: false, hidden: false, locked: false, clips: vec![] });
                self.tracks.len() - 1
            }
        };
        &mut self.tracks[i]
    }
}
