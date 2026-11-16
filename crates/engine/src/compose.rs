//! Builds the picture for one frame of the timeline. Preview and export both
//! call `Compositor::render`, which is how they stay identical.

use crate::frame::Frame;
use crate::media::FrameReader;
use crate::project::{Clip, Project};
use crate::text::TextRenderer;
use crate::Result;
use std::path::Path;

const MAX_SKIP: u64 = 15;

pub struct Compositor {
    pub text: TextRenderer,
    reader: Option<OpenReader>,
}

struct OpenReader {
    clip: Clip,
    size: (u32, u32),
    /// The timeline frame the reader will return next.
    next: u64,
    reader: FrameReader,
    last: Option<Frame>,
}

impl Compositor {
    pub fn new(bundled_fonts: Option<&Path>) -> Self {
        Self { text: TextRenderer::new(bundled_fonts), reader: None }
    }

    /// Renders timeline frame `index` of `project` at the given output size.
    /// Playing frames in order is fast; jumping around reopens the file.
    pub fn render(&mut self, project: &Project, index: u64, width: u32, height: u32) -> Result<Frame> {
        let mut frame = Frame::new(width, height);
        if let Some(clip) = project.clip_at(index) {
            if let Some(video) = self.video_frame(project, clip, index, width, height)? {
                frame = video;
            }
        }
        let visible = |t: &crate::TextLayer| index >= t.start && index < t.start + t.duration;
        for layer in project.texts.iter().filter(|t| visible(t)) {
            self.text.draw(&mut frame, layer);
        }
        Ok(frame)
    }

    fn video_frame(&mut self, project: &Project, clip: &Clip, index: u64, w: u32, h: u32) -> Result<Option<Frame>> {
        // A short jump forward (playback catching up after a slow frame) is
        // cheaper to read through than to reopen the file.
        let reusable = matches!(&self.reader, Some(r) if r.clip == *clip && r.size == (w, h)
            && (index + 1 == r.next || (index >= r.next && index - r.next <= MAX_SKIP)));
        if !reusable {
            let offset = index - clip.start;
            let seconds = clip.source_in + project.fps.frame_to_seconds(offset);
            let reader = FrameReader::open(&clip.source, seconds, w, h, project.fps)?;
            self.reader = Some(OpenReader { clip: clip.clone(), size: (w, h), next: index, reader, last: None });
        }
        let r = self.reader.as_mut().expect("reader opened above");
        if r.next == index + 1 {
            // Same frame asked again (e.g. a paused preview redrawing text).
            return Ok(r.last.clone());
        }
        while r.next < index {
            r.reader.next_frame()?;
            r.next += 1;
        }
        let f = r.reader.next_frame()?;
        r.next = index + 1;
        r.last = f.clone();
        Ok(f)
    }
}
