//! The editing engine. Everything the app shows and exports goes through here,
//! so the preview and the exported file are drawn by the same code.

pub mod compose;
pub mod export;
pub mod frame;
pub mod lut;
pub mod matte;
pub mod media;
pub mod project;
pub mod scopes;
pub mod text;
pub mod time;
pub mod track;

pub use compose::Compositor;
pub use frame::Frame;
pub use project::{
    Adjust, AudioFx, ChromaKey, Clip, Keyframe, Layer, Mask, MaskPoint, MediaKind, Project, SpeedPoint, TextAnim, TextLayer, Track,
    TrackKind, Transform, Transition,
};
pub use time::Fps;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ffmpeg could not be started: {0}")]
    FfmpegMissing(std::io::Error),
    #[error("ffmpeg failed: {0}")]
    Ffmpeg(String),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("project file is damaged: {0}")]
    Json(#[from] serde_json::Error),
    #[error("export was cancelled")]
    Cancelled,
    #[error("model error: {0}")]
    Model(String),
}

pub type Result<T> = std::result::Result<T, Error>;
