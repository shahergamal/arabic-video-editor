//! Time on the timeline is counted in whole frames. A frame rate is a fraction
//! (30000/1001 for 29.97), so there is never floating point drift.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    /// Start time of a frame, in seconds.
    pub fn frame_to_seconds(self, frame: u64) -> f64 {
        frame as f64 * self.den as f64 / self.num as f64
    }

    /// The frame that is showing at a given time.
    pub fn seconds_to_frame(self, seconds: f64) -> u64 {
        // Small epsilon so 1.0s at 30fps is frame 30, not 29.
        ((seconds * self.num as f64 / self.den as f64) + 1e-6).floor().max(0.0) as u64
    }

    /// For ffmpeg arguments, e.g. "30000/1001".
    pub fn as_ffmpeg(self) -> String {
        format!("{}/{}", self.num, self.den)
    }

    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_seconds_land_on_exact_frames() {
        let fps = Fps::new(30, 1);
        assert_eq!(fps.seconds_to_frame(1.0), 30);
        assert_eq!(fps.frame_to_seconds(30), 1.0);
        let ntsc = Fps::new(30000, 1001);
        for f in [0u64, 1, 29, 30, 1799, 107892] {
            assert_eq!(ntsc.seconds_to_frame(ntsc.frame_to_seconds(f)), f);
        }
    }
}
