//! The project file. It only describes edits (which part of which file goes
//! where, which text appears when). Source videos are never modified.

use crate::time::Fps;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: Fps,
    /// Video clips on the main track, each placed by its own start frame.
    pub clips: Vec<Clip>,
    /// Text layers, drawn above the video in list order.
    pub texts: Vec<TextLayer>,
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
    pub start: u64,
    pub duration: u64,
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
            clips: vec![],
            texts: vec![],
        }
    }

    /// Total length in frames: the end of the last clip or text.
    pub fn duration(&self) -> u64 {
        let clips = self.clips.iter().map(|c| c.start + c.duration);
        let texts = self.texts.iter().map(|t| t.start + t.duration);
        clips.chain(texts).max().unwrap_or(0)
    }

    pub fn clip_at(&self, frame: u64) -> Option<&Clip> {
        // Later clips win where clips overlap.
        self.clips
            .iter()
            .rev()
            .find(|c| frame >= c.start && frame < c.start + c.duration)
    }

    pub fn load(path: &Path) -> crate::Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
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
