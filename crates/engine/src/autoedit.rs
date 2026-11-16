//! The automatic edit: from the user's idea and their shots and sounds to a
//! finished timeline they can change.
//!
//! 1. Every shot is looked at (`analyze`), music gets its beats (`beats`).
//! 2. A plan is made: which part of which shot, in what order, and what
//!    words appear on screen. The plan comes from simple rules (always
//!    available, free) or from a language model that reads the idea; the
//!    model only ever sees text (the idea, file names, numbers, what is
//!    said), never pictures or sound.
//! 3. The plan becomes a normal project (`build`).

use crate::analyze::ShotInfo;
use crate::beats::Beats;
use crate::project::{Clip, MediaKind, Project, TextAnim, TextLayer, Track, TrackKind, Transition};
use crate::time::Fps;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Brief {
    /// The idea, in the user's words.
    pub idea: String,
    pub width: u32,
    pub height: u32,
    /// Wanted length in seconds; 0 lets the plan decide.
    #[serde(default)]
    pub seconds: f64,
    /// "fast", "calm" or "" (decide from the idea).
    #[serde(default)]
    pub pace: String,
    pub font: String,
    /// Brand colour for boxes behind text.
    #[serde(default = "accent")]
    pub accent: [u8; 4],
}

fn accent() -> [u8; 4] {
    [20, 20, 20, 200]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanShot {
    /// Index into the list of shots.
    pub shot: usize,
    /// Seconds into the file, and how long.
    pub start: f64,
    pub length: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanText {
    pub text: String,
    /// Seconds from the start of the video.
    pub at: f64,
    pub length: f64,
    /// "title", "note" or "end".
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Plan {
    pub shots: Vec<PlanShot>,
    #[serde(default)]
    pub texts: Vec<PlanText>,
    /// Index of the music file among the shots, if any.
    #[serde(default)]
    pub music: Option<usize>,
    /// Index of a voice-over file, if any.
    #[serde(default)]
    pub voice: Option<usize>,
    /// "fast" or "calm".
    #[serde(default)]
    pub pace: String,
}

/// What the idea says about length, shape and pace, read with simple rules.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hints {
    pub seconds: Option<f64>,
    /// (width, height) when a platform or shape is named.
    pub size: Option<(u32, u32)>,
    pub pace: Option<&'static str>,
    pub phone: Option<String>,
    pub title: Option<String>,
}

/// Arabic-Indic and Persian digits as ASCII.
fn ascii_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '٠'..='٩' => char::from(b'0' + (c as u32 - '٠' as u32) as u8),
            '۰'..='۹' => char::from(b'0' + (c as u32 - '۰' as u32) as u8),
            _ => c,
        })
        .collect()
}

pub fn read_hints(idea: &str) -> Hints {
    let text = ascii_digits(idea);
    let low = text.to_lowercase();
    let mut h = Hints::default();
    // Length: "30 ثانية", "45 sec", "دقيقة", "دقيقتين", "1.5 دقيقة".
    let words: Vec<&str> = low.split(|c: char| c.is_whitespace() || c == '،' || c == ',').filter(|w| !w.is_empty()).collect();
    for (i, w) in words.iter().enumerate() {
        let num: Option<f64> = w.trim_matches(|c: char| !c.is_ascii_digit() && c != '.').parse().ok();
        let next = words.get(i + 1).copied().unwrap_or("");
        let unit = |u: &str| w.contains(u) || next.starts_with(u);
        if let Some(n) = num {
            let seconds = unit("ثاني") || unit("sec") || next == "ث" || w.ends_with('s') && w.len() > 1;
            if seconds {
                h.seconds = Some(n);
            } else if unit("دقيق") || unit("min") {
                h.seconds = Some(n * 60.0);
            }
        } else if *w == "دقيقة" && h.seconds.is_none() {
            h.seconds = Some(60.0);
        } else if *w == "دقيقتين" {
            h.seconds = Some(120.0);
        }
    }
    let has = |list: &[&str]| list.iter().any(|k| low.contains(k));
    if has(&["ريلز", "ريل", "تيك توك", "تيكتوك", "ستوري", "شورتس", "طولي", "reel", "tiktok", "story", "shorts", "vertical"]) {
        h.size = Some((1080, 1920));
    } else if has(&["يوتيوب", "عريض", "youtube", "wide", "16:9"]) {
        h.size = Some((1920, 1080));
    } else if has(&["مربع", "square", "1:1"]) {
        h.size = Some((1080, 1080));
    }
    if has(&["سريع", "حماس", "حماسي", "طاقة", "fast", "energetic", "dynamic", "قوي"]) {
        h.pace = Some("fast");
    } else if has(&["هادي", "هادئ", "راقي", "فخم", "ناعم", "calm", "slow", "elegant", "luxury"]) {
        h.pace = Some("calm");
    }
    // A phone number: 8 or more digits, maybe with + and spaces.
    let mut digits = String::new();
    let mut best = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() || (c == '+' && digits.is_empty()) {
            digits.push(c);
        } else if c == ' ' || c == '-' {
            if !digits.is_empty() {
                digits.push(c);
            }
        } else {
            if digits.chars().filter(|c| c.is_ascii_digit()).count() >= 8 {
                best = digits.trim().to_string();
            }
            digits.clear();
        }
    }
    if digits.chars().filter(|c| c.is_ascii_digit()).count() >= 8 {
        best = digits.trim().to_string();
    }
    if !best.is_empty() {
        h.phone = Some(best);
    }
    // A title in quotes: «...», "...", “...”.
    for (open, close) in [('«', '»'), ('"', '"'), ('“', '”')] {
        if let Some(a) = idea.find(open) {
            let rest = &idea[a + open.len_utf8()..];
            if let Some(b) = rest.find(close) {
                let t = rest[..b].trim();
                if !t.is_empty() && t.chars().count() <= 60 {
                    h.title = Some(t.to_string());
                    break;
                }
            }
        }
    }
    h
}

fn stem(p: &std::path::Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// The plan from rules alone: shots in the order they are named in the
/// idea (by file name), else in the order given; the best part of each;
/// lengths fitted to the wanted length and, with music, to its beats.
pub fn rule_plan(brief: &Brief, shots: &[ShotInfo], beats: Option<&Beats>) -> Plan {
    let hints = read_hints(&brief.idea);
    let pace = if !brief.pace.is_empty() { brief.pace.clone() } else { hints.pace.unwrap_or("fast").to_string() };
    let pictures: Vec<usize> = (0..shots.len()).filter(|i| shots[*i].kind != MediaKind::Audio).collect();
    let sounds: Vec<usize> = (0..shots.len()).filter(|i| shots[*i].kind == MediaKind::Audio).collect();
    // Music: the sound with a clear beat; a voice-over: a sound with speech.
    let voice = sounds.iter().copied().find(|i| !shots[*i].speech.trim().is_empty() || stem(&shots[*i].path).contains("voice") || stem(&shots[*i].path).contains("صوت") || stem(&shots[*i].path).contains("تعليق"));
    let music = sounds.iter().copied().find(|i| Some(*i) != voice);

    // Order: by where each file's name (or its words) appears in the idea.
    let idea = brief.idea.to_lowercase();
    let mut order: Vec<(usize, usize)> = pictures
        .iter()
        .enumerate()
        .map(|(n, i)| {
            let name = stem(&shots[*i].path);
            let pos = name
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.chars().count() >= 3)
                .filter_map(|w| idea.find(w))
                .min()
                .unwrap_or(usize::MAX / 2 + n);
            (pos, *i)
        })
        .collect();
    order.sort();
    let order: Vec<usize> = order.into_iter().map(|(_, i)| i).collect();

    let per_shot = if pace == "calm" { 3.5 } else { 2.0 };
    let total = brief.seconds.max(0.0);
    let total = if total > 0.0 {
        total
    } else if let Some(s) = hints.seconds {
        s
    } else if let Some(v) = voice {
        shots[v].duration
    } else {
        (order.len() as f64 * per_shot).clamp(6.0, 60.0)
    };
    let n = order.len().max(1);
    // Enough shots to fill the length: repeat the best ones' other parts if needed.
    let mut each = (total / n as f64).clamp(1.0, if pace == "calm" { 6.0 } else { 4.0 });
    let mut list: Vec<usize> = order.clone();
    while !list.is_empty() && (list.len() as f64 * each) < total - 0.01 && list.len() < n * 3 {
        list.extend(order.iter().copied());
    }
    if !list.is_empty() {
        each = total / list.len() as f64;
    }
    // Cut points, on the beat when there is music.
    let mut cuts: Vec<f64> = (0..=list.len()).map(|k| k as f64 * each).collect();
    if let (Some(b), Some(_)) = (beats, music) {
        let times: Vec<f64> = b.times.iter().map(|t| t - b.times.first().copied().unwrap_or(0.0)).collect();
        for c in cuts.iter_mut().skip(1) {
            if let Some(t) = times.iter().copied().min_by(|x, y| (x - *c).abs().total_cmp(&(y - *c).abs())) {
                if (t - *c).abs() < each * 0.45 {
                    *c = t;
                }
            }
        }
        for k in 1..cuts.len() {
            if cuts[k] < cuts[k - 1] + 0.5 {
                cuts[k] = cuts[k - 1] + 0.5;
            }
        }
    }
    let mut used: Vec<Vec<(f64, f64)>> = vec![vec![]; shots.len()];
    let mut plan_shots = vec![];
    for (k, i) in list.iter().enumerate() {
        let length = cuts[k + 1] - cuts[k];
        let s = &shots[*i];
        let start = if s.kind == MediaKind::Image {
            0.0
        } else {
            // The best part not used yet.
            let mut best = s.best_part(length).0;
            if used[*i].iter().any(|(a, b)| best < *b && best + length > *a) {
                let mut alt = (0.0, f32::MIN);
                let mut t = 0.0;
                while t + length <= s.duration + 1e-6 {
                    let clash = used[*i].iter().any(|(a, b)| t < *b && t + length > *a);
                    if !clash {
                        let q = s.moments.iter().filter(|m| m.at >= t && m.at < t + length).map(ShotInfo::quality).sum::<f32>();
                        if q > alt.1 {
                            alt = (t, q);
                        }
                    }
                    t += 0.25;
                }
                if alt.1 > f32::MIN {
                    best = alt.0;
                }
            }
            best
        };
        let length = if s.kind == MediaKind::Video { length.min(s.duration - start).max(0.3) } else { length };
        used[*i].push((start, start + length));
        plan_shots.push(PlanShot { shot: *i, start, length });
    }
    let end: f64 = plan_shots.iter().map(|p| p.length).sum();
    let mut texts = vec![];
    if let Some(t) = hints.title {
        texts.push(PlanText { text: t, at: 0.0, length: 3.0f64.min(end), kind: "title".into() });
    }
    if let Some(p) = hints.phone {
        let d = 3.0f64.min(end);
        texts.push(PlanText { text: format!("للتواصل: {p}"), at: (end - d).max(0.0), length: d, kind: "end".into() });
    }
    Plan { shots: plan_shots, texts, music, voice, pace }
}

/// Text for a language model: the idea and a description of each shot.
/// Only text: file names, lengths, picture quality numbers, what is said.
pub fn describe(brief: &Brief, shots: &[ShotInfo]) -> String {
    let mut s = format!(
        "Idea (from the user, may be Arabic):\n{}\n\nFrame: {}x{}. Wanted length: {}.\n\nFiles:\n",
        brief.idea,
        brief.width,
        brief.height,
        if brief.seconds > 0.0 { format!("{:.0} s", brief.seconds) } else { "decide from the idea".into() }
    );
    for (i, sh) in shots.iter().enumerate() {
        let name = sh.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let kind = match sh.kind {
            MediaKind::Video => "video",
            MediaKind::Image => "photo",
            MediaKind::Audio => "sound",
            MediaKind::Compound => "group",
        };
        s += &format!("[{i}] {kind} \"{name}\"");
        if sh.kind != MediaKind::Image {
            s += &format!(", {:.1} s", sh.duration);
        }
        if sh.kind == MediaKind::Video && !sh.moments.is_empty() {
            // Quality per second, 0-9, so the model can pick good parts.
            let per: Vec<String> = sh
                .moments
                .chunks(4)
                .map(|c| ((c.iter().map(ShotInfo::quality).sum::<f32>() / c.len() as f32) * 9.0).round().to_string())
                .collect();
            s += &format!(", quality per second: {}", per.join(""));
            let motion = sh.moments.iter().map(|m| m.motion).sum::<f32>() / sh.moments.len() as f32;
            s += &format!(", movement {:.0}%", motion * 100.0);
        }
        if !sh.speech.trim().is_empty() {
            let said: String = sh.speech.chars().take(400).collect();
            s += &format!(", says: \"{said}\"");
        }
        s += "\n";
    }
    s
}

/// The instructions for the language model, and the JSON shape it fills.
pub const PLAN_INSTRUCTIONS: &str = "You are a video editor. Plan a short edit from the user's idea and files. \
Return JSON only. Use each video's best seconds (higher quality digits), keep shots between 1 and 6 seconds, \
order them to tell the idea, and fill the wanted length. Pick at most one music file and one voice-over file \
from the sounds (or none). Add a few short on-screen texts in the idea's language: a title at the start if it fits, \
short notes over shots when the idea asks for them, and an end card with any contact details in the idea. \
Texts are at most 8 words. Never invent prices, phone numbers or facts that are not in the idea. \
pace is \"fast\" or \"calm\".";

/// JSON schema of the plan, for structured output.
pub fn plan_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "shots": {"type": "array", "items": {"type": "object", "properties": {
                "shot": {"type": "integer"}, "start": {"type": "number"}, "length": {"type": "number"}
            }, "required": ["shot", "start", "length"], "additionalProperties": false}},
            "texts": {"type": "array", "items": {"type": "object", "properties": {
                "text": {"type": "string"}, "at": {"type": "number"}, "length": {"type": "number"},
                "kind": {"type": "string", "enum": ["title", "note", "end"]}
            }, "required": ["text", "at", "length", "kind"], "additionalProperties": false}},
            "music": {"type": ["integer", "null"]},
            "voice": {"type": ["integer", "null"]},
            "pace": {"type": "string", "enum": ["fast", "calm"]}
        },
        "required": ["shots", "texts", "music", "voice", "pace"],
        "additionalProperties": false
    })
}

/// Reads a model's plan and makes it safe: unknown files dropped, times
/// kept inside each file, lengths sensible. `None` when nothing usable.
pub fn parse_plan(text: &str, shots: &[ShotInfo]) -> Option<Plan> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let mut plan: Plan = serde_json::from_str(&text[start..=end]).ok()?;
    plan.shots.retain(|p| p.shot < shots.len() && shots[p.shot].kind != MediaKind::Audio && p.length.is_finite() && p.start.is_finite());
    for p in &mut plan.shots {
        let s = &shots[p.shot];
        p.length = p.length.clamp(0.5, 10.0);
        if s.kind == MediaKind::Video {
            p.length = p.length.min(s.duration.max(0.5));
            p.start = p.start.clamp(0.0, (s.duration - p.length).max(0.0));
        } else {
            p.start = 0.0;
        }
    }
    let ok_sound = |i: &Option<usize>| i.filter(|i| *i < shots.len() && shots[*i].kind == MediaKind::Audio);
    plan.music = ok_sound(&plan.music);
    plan.voice = ok_sound(&plan.voice).filter(|v| Some(*v) != plan.music);
    let total: f64 = plan.shots.iter().map(|p| p.length).sum();
    plan.texts.retain(|t| !t.text.trim().is_empty() && t.at.is_finite() && t.length.is_finite());
    for t in &mut plan.texts {
        t.at = t.at.clamp(0.0, total.max(0.0));
        t.length = t.length.clamp(0.5, (total - t.at).max(0.5));
        t.text = t.text.chars().take(80).collect();
    }
    if plan.pace != "calm" {
        plan.pace = "fast".into();
    }
    (!plan.shots.is_empty()).then_some(plan)
}

fn text_layer(brief: &Brief, t: &PlanText, fps: Fps) -> TextLayer {
    let f = |s: f64| (s * fps.as_f64()).round().max(0.0) as u64;
    let (size, y, weight, background) = match t.kind.as_str() {
        "title" => (0.075, 0.22, 800, [0, 0, 0, 0]),
        "end" => (0.06, 0.5, 700, brief.accent),
        _ => (0.05, 0.8, 700, [0, 0, 0, 0]),
    };
    let anim = |kind: &str| Some(TextAnim { kind: kind.into(), frames: f(0.4).max(1) });
    TextLayer {
        id: format!("auto-{}-{}", t.kind, f(t.at)),
        text: t.text.clone(),
        font_family: brief.font.clone(),
        size,
        weight,
        color: [255, 255, 255, 255],
        x: 0.5,
        y,
        box_width: 0.86,
        shadow: background[3] == 0,
        outline: 0.0,
        outline_color: [0, 0, 0, 255],
        background,
        align: "center".into(),
        start: f(t.at),
        duration: f(t.length).max(1),
        anim_in: if t.kind == "title" { anim("rise") } else { anim("pop") },
        anim_out: anim("fade"),
    }
}

/// The plan as a project. Pictures fill the frame; music is softened under
/// speech and fades out at the end.
pub fn build(plan: &Plan, brief: &Brief, shots: &[ShotInfo], name: &str) -> Project {
    let fps = Fps::new(30, 1);
    let mut p = Project::new(name, brief.width, brief.height, fps);
    let f = |s: f64| (s * fps.as_f64()).round().max(0.0) as u64;
    let mut at = 0u64;
    let calm = plan.pace == "calm";
    let mut clips = vec![];
    for (k, ps) in plan.shots.iter().enumerate() {
        let s = &shots[ps.shot];
        let dur = f(ps.length).max(1);
        let mut c = Clip::new(&format!("auto{k}"), s.kind, &s.path, at, dur, ps.start);
        if s.width > 0 && s.height > 0 {
            let (sw, sh) = (s.width as f32, s.height as f32);
            let (w, h) = (brief.width as f32, brief.height as f32);
            c.transform.scale = (w / sw).max(h / sh) / (w / sw).min(h / sh);
        }
        if s.kind == MediaKind::Image {
            // A slow push in keeps photos alive.
            let base = c.transform;
            c.keyframes = vec![
                crate::project::Keyframe { frame: 0, transform: base },
                crate::project::Keyframe { frame: dur.saturating_sub(1), transform: crate::Transform { scale: base.scale * 1.08, ..base } },
            ];
        }
        if calm && k > 0 {
            c.transition_in = Some(Transition { kind: "dissolve".into(), frames: f(0.4).min(dur / 3) });
        }
        // Their own sound stays only when there is no voice-over.
        if plan.voice.is_some() {
            c.volume = 0.0;
        }
        clips.push(c);
        at += dur;
    }
    let total = at;
    p.tracks[0].clips = clips;
    let audio_clip = |id: &str, i: usize, len: u64| -> Clip {
        let mut c = Clip::new(id, MediaKind::Audio, &shots[i].path, 0, len, 0.0);
        c.media = MediaKind::Audio;
        c
    };
    p.tracks.retain(|t| t.kind == TrackKind::Video);
    if let Some(v) = plan.voice {
        let len = f(shots[v].duration).min(total).max(1);
        p.tracks.push(Track { id: "a1".into(), kind: TrackKind::Audio, muted: false, hidden: false, locked: false, duck: false, clips: vec![audio_clip("voice", v, len)] });
    }
    if let Some(m) = plan.music {
        let len = f(shots[m].duration).min(total).max(1);
        let mut c = audio_clip("music", m, len);
        c.volume = 0.8;
        c.fade_in = f(0.3);
        c.fade_out = f(1.5).min(len / 2);
        p.tracks.push(Track { id: "a2".into(), kind: TrackKind::Audio, muted: false, hidden: false, locked: false, duck: true, clips: vec![c] });
    }
    if !p.tracks.iter().any(|t| t.kind == TrackKind::Audio) {
        p.tracks.push(Track { id: "a1".into(), kind: TrackKind::Audio, muted: false, hidden: false, locked: false, duck: false, clips: vec![] });
    }
    p.texts = plan.texts.iter().map(|t| text_layer(brief, t, fps)).collect();
    p.loudness = Some(-14.0);
    p
}
