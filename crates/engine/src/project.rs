//! The project file. It only describes edits (which part of which file goes
//! where, which text appears when). Source videos are never modified.

use crate::time::Fps;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const FORMAT_VERSION: u32 = 6;

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
    /// Make the whole video this loud (LUFS) on export, e.g. -14 for social
    /// media. `None` leaves the sound as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loudness: Option<f32>,
    /// Cameras of a multi-camera shoot, lined up by sound (see `multicam`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub angles: Vec<Angle>,
    /// Named points on the timeline, to find places again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker>,
    /// Shapes and arrows, drawn above the video and below the texts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shapes: Vec<Shape>,
    /// Groups of clips used as one clip (see `MediaKind::Compound`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compounds: Vec<Compound>,
    /// Version 1 files kept clips here; they are moved onto a track on load.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<Clip>,
}

/// One camera of a multi-camera shoot. `offset` is where this file's start
/// falls on the first camera's clock, in seconds (negative: it started
/// earlier). The same moment is at `t - offset` in this file when it is at
/// `t` in the first.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Angle {
    pub source: PathBuf,
    pub offset: f64,
    /// Length of the file in seconds.
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Marker {
    pub frame: u64,
    #[serde(default)]
    pub label: String,
    /// A CSS colour, e.g. "#f5a524".
    #[serde(default)]
    pub color: String,
}

/// A shape on top of the video. Sizes are shares of the frame: `w` of its
/// width, `h` of its height. Lines and arrows run across `w` through the
/// centre, turned by `rotation`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Shape {
    pub id: String,
    /// "rect", "ellipse", "line", "arrow" or "star".
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Degrees, clockwise.
    #[serde(default)]
    pub rotation: f32,
    /// Inside colour (alpha 0 = empty).
    pub fill: [u8; 4],
    /// Edge colour; lines and arrows are drawn in this colour.
    pub stroke: [u8; 4],
    /// Edge thickness as a share of the frame height.
    #[serde(default)]
    pub stroke_width: f32,
    /// Rounded corners for "rect", as a share of the shorter side (0 to 0.5).
    #[serde(default)]
    pub radius: f32,
    #[serde(default)]
    pub shadow: bool,
    pub start: u64,
    pub duration: u64,
    /// Same kinds as texts, plus "draw" (lines and arrows grow).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim_in: Option<TextAnim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim_out: Option<TextAnim>,
}

/// Clips grouped into one. A compound clip shows this group's tracks,
/// starting `source_in` seconds into the group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Compound {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub tracks: Vec<Track>,
}

impl Compound {
    pub fn duration(&self) -> u64 {
        self.tracks.iter().flat_map(|t| t.clips.iter().map(Clip::end)).max().unwrap_or(0)
    }
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
    /// Background music: goes quieter by itself while others speak.
    #[serde(default)]
    pub duck: bool,
    pub clips: Vec<Clip>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    #[default]
    Video,
    Image,
    Audio,
    /// A group of clips (`source` holds the group's id).
    Compound,
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
    /// Degrees, clockwise.
    #[serde(default)]
    pub rotation: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self { scale: 1.0, x: 0.0, y: 0.0, opacity: 1.0, rotation: 0.0 }
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
    /// Playback speed: 2.0 plays twice as fast, 0.5 in slow motion.
    #[serde(default = "one")]
    pub speed: f32,
    /// Colour adjustments for the picture.
    #[serde(default, skip_serializing_if = "Adjust::is_neutral")]
    pub adjust: Adjust,
    /// Movement over time. Empty = `transform` all the way through.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyframes: Vec<Keyframe>,
    /// How this clip comes in from the clip before it on the same track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition_in: Option<Transition>,
    /// Cut the subject out of the background with this model ("people" or
    /// "objects"). Runs on this computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_bg: Option<String>,
    /// Make one colour see-through (green screen).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chroma: Option<ChromaKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<Mask>,
    /// Speed changing smoothly over time; when set it replaces `speed`.
    /// The clip's own sound is left out while ramping.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speed_ramp: Vec<SpeedPoint>,
    #[serde(default, skip_serializing_if = "AudioFx::is_off")]
    pub audio_fx: AudioFx,
    /// Play backwards. `source_in` then counts from the end of the file
    /// (it is a position in the reversed file), so trimming and splitting
    /// work the same way as for normal clips.
    #[serde(default, skip_serializing_if = "is_false")]
    pub reverse: bool,
    /// Show one picture (at `source_in`) all the way through, silent.
    #[serde(default, skip_serializing_if = "is_false")]
    pub freeze: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_h: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_v: bool,
    #[serde(default, skip_serializing_if = "Crop::is_none")]
    pub crop: Crop,
    /// How the picture mixes with what is under it: "normal", "multiply",
    /// "screen", "overlay", "add", "darken" or "lighten".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blend: String,
    /// Steady a shaky video, 0 (off) to 1 (strongest).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub stabilize: f32,
}

fn is_false(v: &bool) -> bool {
    !*v
}
fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

/// Hides the edges of the picture, as shares of its width and height.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct Crop {
    #[serde(default)]
    pub left: f32,
    #[serde(default)]
    pub top: f32,
    #[serde(default)]
    pub right: f32,
    #[serde(default)]
    pub bottom: f32,
}

impl Crop {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ChromaKey {
    pub color: [u8; 3],
    /// How different a colour may be and still be removed (0 to 1).
    pub strength: f32,
    /// Width of the soft edge (0 to 1).
    pub softness: f32,
}

/// A shape on the picture: keep only inside it, cut it out, or blur it
/// (to hide a face or a car plate). Coordinates are shares of the frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mask {
    /// "rect" or "ellipse".
    pub shape: String,
    /// "keep", "cut" or "blur".
    pub mode: String,
    /// Centre, 0..1 across and down the frame.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Soft edge, as a share of the shape's size.
    #[serde(default)]
    pub feather: f32,
    /// Blur strength for "blur", as a share of the frame width.
    #[serde(default = "default_blur")]
    pub blur: f32,
    /// Where the centre is at given frames (from tracking), counted from the
    /// clip's start. Between points it moves in a straight line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<MaskPoint>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MaskPoint {
    pub frame: u64,
    pub x: f32,
    pub y: f32,
}

fn default_blur() -> f32 {
    0.03
}

impl Mask {
    /// Centre at `local` frames into the clip.
    pub fn centre_at(&self, local: u64) -> (f32, f32) {
        let p = &self.path;
        if p.is_empty() {
            return (self.x, self.y);
        }
        if local <= p[0].frame {
            return (p[0].x, p[0].y);
        }
        for w in p.windows(2) {
            if local <= w[1].frame {
                let t = (local - w[0].frame) as f32 / (w[1].frame - w[0].frame).max(1) as f32;
                return (w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t);
            }
        }
        let l = p[p.len() - 1];
        (l.x, l.y)
    }
}

/// Speed at a frame (counted from the clip's start).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SpeedPoint {
    pub frame: u64,
    pub speed: f32,
}

/// Clean-up for the clip's sound.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct AudioFx {
    /// Reduce steady background noise (fans, hum, street).
    #[serde(default)]
    pub denoise: bool,
    /// Make speech clearer and more even.
    #[serde(default)]
    pub voice: bool,
}

impl AudioFx {
    pub fn is_off(&self) -> bool {
        !self.denoise && !self.voice
    }
}

/// Colour adjustments. Every value runs from -1 to 1; 0 changes nothing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Adjust {
    /// Lift or lower the dark parts only.
    #[serde(default)]
    pub shadows: f32,
    /// Lift or lower the bright parts only.
    #[serde(default)]
    pub highlights: f32,
    /// Below 0 greener, above 0 more magenta.
    #[serde(default)]
    pub tint: f32,
    /// Darken (above 0) or lighten (below 0) the corners.
    #[serde(default)]
    pub vignette: f32,
    /// A colour look file (.cube) and how strongly to use it (0 to 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lut: Option<PathBuf>,
    #[serde(default = "one")]
    pub lut_strength: f32,
    #[serde(default)]
    pub brightness: f32,
    #[serde(default)]
    pub contrast: f32,
    #[serde(default)]
    pub saturation: f32,
    /// Below 0 is cooler (bluer), above 0 warmer (more orange).
    #[serde(default)]
    pub temperature: f32,
    /// Colour wheels: a push towards a colour for the dark parts (lift),
    /// the middle (gamma) and the bright parts (gain), per red, green, blue,
    /// each -1 to 1.
    #[serde(default, skip_serializing_if = "zero3")]
    pub lift: [f32; 3],
    #[serde(default, skip_serializing_if = "zero3")]
    pub gamma: [f32; 3],
    #[serde(default, skip_serializing_if = "zero3")]
    pub gain: [f32; 3],
    #[serde(default, skip_serializing_if = "Curves::is_flat")]
    pub curves: Curves,
}

fn zero3(v: &[f32; 3]) -> bool {
    v.iter().all(|x| *x == 0.0)
}

/// Tone curves as points (input, output), both 0 to 1. An empty list
/// changes nothing. `master` applies to all three colours.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Curves {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub master: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub r: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub g: Vec<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub b: Vec<[f32; 2]>,
}

impl Curves {
    pub fn is_flat(&self) -> bool {
        let flat = |c: &Vec<[f32; 2]>| c.iter().all(|p| (p[0] - p[1]).abs() < 1e-4);
        flat(&self.master) && flat(&self.r) && flat(&self.g) && flat(&self.b)
    }
}

impl Default for Adjust {
    fn default() -> Self {
        Self {
            shadows: 0.0,
            highlights: 0.0,
            tint: 0.0,
            vignette: 0.0,
            lut: None,
            lut_strength: 1.0,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            temperature: 0.0,
            lift: [0.0; 3],
            gamma: [0.0; 3],
            gain: [0.0; 3],
            curves: Curves::default(),
        }
    }
}

impl Adjust {
    pub fn is_neutral(&self) -> bool {
        let z = |v: f32| v == 0.0;
        z(self.brightness) && z(self.contrast) && z(self.saturation) && z(self.temperature)
            && z(self.shadows) && z(self.highlights) && z(self.tint) && z(self.vignette) && self.lut.is_none()
            && zero3(&self.lift) && zero3(&self.gamma) && zero3(&self.gain) && self.curves.is_flat()
    }
}

/// The clip's position and size at a moment, counted from the clip's start.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Keyframe {
    pub frame: u64,
    pub transform: Transform,
}

/// Kinds: "dissolve", "black" (through black), "slide", "wipe", "zoom".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transition {
    pub kind: String,
    pub frames: u64,
}

/// Text animation. Kinds: "fade", "rise", "pop", "zoom", "words".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextAnim {
    pub kind: String,
    pub frames: u64,
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
            speed: 1.0,
            adjust: Adjust::default(),
            keyframes: vec![],
            transition_in: None,
            remove_bg: None,
            chroma: None,
            mask: None,
            speed_ramp: vec![],
            audio_fx: AudioFx::default(),
            reverse: false,
            freeze: false,
            flip_h: false,
            flip_v: false,
            crop: Crop::default(),
            blend: String::new(),
            stabilize: 0.0,
        }
    }

    /// Speed at `local` frames into the clip.
    pub fn speed_at(&self, local: u64) -> f32 {
        let r = &self.speed_ramp;
        if r.is_empty() {
            return self.speed;
        }
        if local <= r[0].frame {
            return r[0].speed;
        }
        for w in r.windows(2) {
            if local <= w[1].frame {
                let t = (local - w[0].frame) as f32 / (w[1].frame - w[0].frame).max(1) as f32;
                return w[0].speed + (w[1].speed - w[0].speed) * t;
            }
        }
        r[r.len() - 1].speed
    }

    /// Seconds into the source file shown at timeline frame `frame`.
    pub fn source_time(&self, frame: u64, fps: crate::time::Fps) -> f64 {
        let local = frame.saturating_sub(self.start);
        if self.freeze {
            return self.source_in;
        }
        if self.speed_ramp.is_empty() {
            return self.source_in + fps.frame_to_seconds(local) * self.speed.max(0.01) as f64;
        }
        // Add up the time each frame covers (average speed over each frame).
        let per = fps.frame_to_seconds(1);
        let mut t = self.source_in;
        for k in 0..local {
            t += per * ((self.speed_at(k) + self.speed_at(k + 1)) as f64 / 2.0).max(0.01);
        }
        t
    }

    /// Position, size and opacity at timeline frame `frame`, following the
    /// keyframes with a gentle ease in and out.
    pub fn transform_at(&self, frame: u64) -> Transform {
        if self.keyframes.is_empty() {
            return self.transform;
        }
        let local = frame.saturating_sub(self.start);
        let mut keys = self.keyframes.clone();
        keys.sort_by_key(|k| k.frame);
        let first = keys[0];
        let last = *keys.last().unwrap();
        if local <= first.frame {
            return first.transform;
        }
        if local >= last.frame {
            return last.transform;
        }
        let i = keys.iter().position(|k| k.frame > local).unwrap();
        let (a, b) = (keys[i - 1], keys[i]);
        let t = (local - a.frame) as f32 / (b.frame - a.frame).max(1) as f32;
        let e = t * t * (3.0 - 2.0 * t);
        let mix = |x: f32, y: f32| x + (y - x) * e;
        Transform {
            scale: mix(a.transform.scale, b.transform.scale),
            x: mix(a.transform.x, b.transform.x),
            y: mix(a.transform.y, b.transform.y),
            opacity: mix(a.transform.opacity, b.transform.opacity),
            rotation: mix(a.transform.rotation, b.transform.rotation),
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
    /// A clip that reads a media file (not a group).
    pub fn is_file(&self) -> bool {
        self.media != MediaKind::Compound
    }
}

/// One picture to draw for a frame; see [`Project::layers_at`].
pub struct Layer<'a> {
    pub clip: &'a Clip,
    pub transition: Option<(&'a Transition, f32)>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim_in: Option<TextAnim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim_out: Option<TextAnim>,
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
                Track { id: "v1".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, duck: false, clips: vec![] },
                Track { id: "a1".into(), kind: TrackKind::Audio, muted: false, hidden: false, locked: false, duck: false, clips: vec![] },
            ],
            texts: vec![],
            loudness: None,
            angles: vec![],
            markers: vec![],
            shapes: vec![],
            compounds: vec![],
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
                    Track { id: "v1".into(), kind: TrackKind::Video, muted: false, hidden: false, locked: false, duck: false, clips },
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
        let shapes = self.shapes.iter().map(|s| s.start + s.duration);
        clips.chain(texts).chain(shapes).max().unwrap_or(0)
    }

    /// The picture clips showing at `frame`, bottom to top.
    pub fn pictures_at(&self, frame: u64) -> Vec<&Clip> {
        self.tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video && !t.hidden)
            .filter_map(|t| t.clips.iter().rev().find(|c| c.has_picture() && c.covers(frame)))
            .collect()
    }

    /// What to draw at `frame`, bottom to top. During a transition the
    /// outgoing clip is listed first (it keeps playing past its end), then
    /// the incoming clip with how far the transition has got (0 to 1).
    pub fn layers_at(&self, frame: u64) -> Vec<Layer<'_>> {
        let mut out = vec![];
        for t in self.tracks.iter().filter(|t| t.kind == TrackKind::Video && !t.hidden) {
            let Some(b) = t.clips.iter().rev().find(|c| c.has_picture() && c.covers(frame)) else { continue };
            let mut progress = None;
            if let Some(tr) = b.transition_in.as_ref().filter(|tr| tr.frames > 0 && frame < b.start + tr.frames) {
                let before = t.clips.iter().filter(|c| c.id != b.id && c.has_picture()).find(|c| c.end() == b.start);
                if let Some(a) = before {
                    out.push(Layer { clip: a, transition: None });
                }
                progress = Some((tr, (frame - b.start) as f32 / tr.frames as f32));
            }
            out.push(Layer { clip: b, transition: progress });
        }
        out
    }

    /// Whether a clip sits on a music track that ducks under speech.
    pub fn is_ducked(&self, clip_id: &str) -> bool {
        // Clips taken out of a group are named "<group clip>/<clip>".
        let top = clip_id.split('/').next().unwrap_or(clip_id);
        self.tracks.iter().any(|t| t.duck && t.clips.iter().any(|c| c.id == top))
    }

    pub fn compound(&self, id: &Path) -> Option<&Compound> {
        self.compounds.iter().find(|c| Path::new(&c.id) == id)
    }

    /// Every clip whose sound is heard in the export. Grouped clips are
    /// taken out of their group and placed where they are heard.
    pub fn audible_clips(&self) -> Vec<Clip> {
        let mut out = vec![];
        for c in self.tracks.iter().filter(|t| !t.muted).flat_map(|t| t.clips.iter()) {
            self.collect_audible(c, &mut out, 0);
        }
        out
    }

    fn collect_audible(&self, c: &Clip, out: &mut Vec<Clip>, depth: u32) {
        let heard = |c: &Clip| c.media != MediaKind::Image && c.speed_ramp.is_empty() && !c.freeze && c.volume > 0.0;
        if c.media != MediaKind::Compound {
            if heard(c) {
                out.push(c.clone());
            }
            return;
        }
        for (_, _, muted, n) in self.expand(c, depth) {
            if !muted {
                self.collect_audible(&n, out, depth + 1);
            }
        }
    }

    /// The clips inside a group clip, cut to the part it shows and placed
    /// where they play on the timeline: (track kind, track number in the
    /// group, track muted, clip). Inner groups are left as groups.
    pub fn expand(&self, c: &Clip, depth: u32) -> Vec<(TrackKind, usize, bool, Clip)> {
        let mut out = vec![];
        let Some(group) = self.compound(&c.source) else { return out };
        if depth > 8 {
            return out;
        }
        let offset = (c.source_in * self.fps.as_f64()).round().max(0.0) as u64;
        let (from, to) = (offset, offset + c.duration);
        for (ti, track) in group.tracks.iter().enumerate() {
            for inner in &track.clips {
                let (s, e) = (inner.start.max(from), inner.end().min(to));
                if s >= e {
                    continue;
                }
                let cut = s - inner.start;
                let mut n = inner.clone();
                n.id = format!("{}/{}", c.id, inner.id);
                n.start = c.start + (s - from);
                n.duration = e - s;
                let rate = if inner.media == MediaKind::Compound || inner.freeze { 1.0 } else { inner.speed.max(0.01) as f64 };
                if !inner.freeze {
                    n.source_in += self.fps.frame_to_seconds(cut) * rate;
                }
                n.volume *= c.volume;
                if cut > 0 {
                    n.fade_in = 0;
                    n.transition_in = None;
                }
                if e < inner.end() {
                    n.fade_out = 0;
                }
                if !n.keyframes.is_empty() {
                    let at = inner.transform_at(s);
                    n.keyframes.retain(|k| k.frame > cut);
                    for k in &mut n.keyframes {
                        k.frame -= cut;
                    }
                    n.keyframes.insert(0, Keyframe { frame: 0, transform: at });
                }
                out.push((track.kind, ti, track.muted, n));
            }
        }
        out
    }

    /// The same edit with every group clip replaced by the clips inside it
    /// (for other editors, which have no groups). Effects on the group clip
    /// itself are lost.
    pub fn flattened(&self) -> Self {
        let mut p = self.clone();
        for _ in 0..8 {
            if !p.all_clips().any(|(_, c)| c.media == MediaKind::Compound) {
                break;
            }
            let mut tracks: Vec<Track> = vec![];
            let mut extra_audio: Vec<Track> = vec![];
            for t in &p.tracks {
                let mut base = Track { clips: vec![], ..t.clone() };
                let mut above: Vec<Track> = vec![];
                for c in &t.clips {
                    if c.media != MediaKind::Compound {
                        base.clips.push(c.clone());
                        continue;
                    }
                    let mut first_video = None;
                    for (kind, ti, muted, n) in p.expand(c, 0) {
                        let dest = if kind == TrackKind::Audio {
                            while extra_audio.len() <= ti {
                                let id = format!("{}-a{}", t.id, extra_audio.len() + 1);
                                extra_audio.push(Track { id, kind: TrackKind::Audio, muted, hidden: false, locked: false, duck: false, clips: vec![] });
                            }
                            &mut extra_audio[ti]
                        } else {
                            let first = *first_video.get_or_insert(ti);
                            if ti == first {
                                &mut base
                            } else {
                                let k = ti - first - 1;
                                while above.len() <= k {
                                    let id = format!("{}-v{}", t.id, above.len() + 2);
                                    above.push(Track { id, kind: TrackKind::Video, muted: false, hidden: false, locked: false, duck: false, clips: vec![] });
                                }
                                &mut above[k]
                            }
                        };
                        dest.clips.push(n);
                    }
                }
                base.clips.sort_by_key(|c| c.start);
                tracks.push(base);
                for mut a in above {
                    a.clips.sort_by_key(|c| c.start);
                    tracks.push(a);
                }
            }
            for mut a in extra_audio {
                a.clips.sort_by_key(|c| c.start);
                tracks.push(a);
            }
            p.tracks = tracks;
        }
        p.compounds.clear();
        p
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
                self.tracks.push(Track { id, kind, muted: false, hidden: false, locked: false, duck: false, clips: vec![] });
                self.tracks.len() - 1
            }
        };
        &mut self.tracks[i]
    }
}

impl Project {
    /// The same edit in another shape (e.g. a 9:16 reel as a 16:9 video).
    /// Texts keep their place as a share of the frame. With `fill`, pictures
    /// that filled the old frame are enlarged to fill the new one; small
    /// pictures (picture-in-picture) are left as they are.
    /// `source_size` tells the size of each source file.
    pub fn reframed(&self, width: u32, height: u32, fill: bool, source_size: impl Fn(&Path) -> Option<(u32, u32)>) -> Self {
        let mut p = self.clone();
        let cover = |src: (u32, u32), w: u32, h: u32| {
            let (sw, sh) = (src.0.max(1) as f32, src.1.max(1) as f32);
            let fit = (w as f32 / sw).min(h as f32 / sh);
            let fill = (w as f32 / sw).max(h as f32 / sh);
            fill / fit
        };
        if fill {
            for t in &mut p.tracks {
                for c in t.clips.iter_mut().filter(|c| c.has_picture()) {
                    let Some(src) = source_size(&c.source) else { continue };
                    let (old, new) = (cover(src, self.width, self.height), cover(src, width, height));
                    let fix = |tr: &mut Transform| {
                        if tr.x.abs() < 0.01 && tr.y.abs() < 0.01 && tr.scale >= 0.99 {
                            tr.scale = new * (tr.scale / old).max(1.0);
                        }
                    };
                    fix(&mut c.transform);
                    for k in &mut c.keyframes {
                        fix(&mut k.transform);
                    }
                }
            }
        }
        p.width = width;
        p.height = height;
        p
    }
}
