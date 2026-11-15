//! Moving a timeline to and from other editors.
//!
//! - OpenTimelineIO (`.otio`): read and write. DaVinci Resolve, Avid, Hiero
//!   and many tools read it.
//! - Final Cut Pro 7 XML (`.xml`, "xmeml"): read and write. This is the XML
//!   that Premiere Pro and DaVinci Resolve import and export.
//! - CMX 3600 EDL (`.edl`): write only; the oldest common format, cuts only.
//!
//! What moves across: clips with their place on the timeline and the part of
//! the file they use, track order, and (OTIO, EDL) speed. Text, colour,
//! masks and other effects stay behind; `Exported::warnings` says so.

use crate::project::{Clip, MediaKind, Project, Track, TrackKind};
use crate::time::Fps;
use crate::{Error, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A written timeline, with what could not be carried over.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Exported {
    pub text: String,
    pub warnings: Vec<String>,
}

fn fps_f(f: Fps) -> f64 {
    f.num as f64 / f.den as f64
}

/// Notes about what other editors will not see.
fn warnings(p: &Project, speed_kept: bool) -> Vec<String> {
    let mut w = vec![];
    if !p.texts.is_empty() {
        w.push("texts".into());
    }
    let clips: Vec<&Clip> = p.all_clips().map(|(_, c)| c).collect();
    if clips.iter().any(|c| !c.adjust.is_neutral() || c.chroma.is_some() || c.remove_bg.is_some() || c.mask.is_some()) {
        w.push("effects".into());
    }
    if clips.iter().any(|c| !c.keyframes.is_empty() || c.transform != Default::default()) {
        w.push("motion".into());
    }
    if clips.iter().any(|c| !c.speed_ramp.is_empty()) || (!speed_kept && clips.iter().any(|c| (c.speed - 1.0).abs() > 1e-3)) {
        w.push("speed".into());
    }
    if clips.iter().any(|c| c.transition_in.is_some()) {
        w.push("transitions".into());
    }
    w
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Guesses what a file holds from its extension.
pub fn kind_of(path: &Path) -> MediaKind {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tif" | "tiff" | "heic" => MediaKind::Image,
        "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" | "opus" | "aif" | "aiff" => MediaKind::Audio,
        _ => MediaKind::Video,
    }
}

fn new_id(n: &mut usize) -> String {
    *n += 1;
    format!("imp{n}")
}

// ----------------------------------------------------------------- OTIO

fn rt(value: f64, rate: f64) -> Value {
    json!({ "OTIO_SCHEMA": "RationalTime.1", "value": value, "rate": rate })
}

fn range(start: f64, duration: f64, rate: f64) -> Value {
    json!({ "OTIO_SCHEMA": "TimeRange.1", "start_time": rt(start, rate), "duration": rt(duration, rate) })
}

/// An OpenTimelineIO file of the project.
pub fn to_otio(p: &Project) -> Exported {
    let rate = fps_f(p.fps);
    let tracks: Vec<Value> = p
        .tracks
        .iter()
        // OTIO stacks bottom to top, like we do.
        .map(|t| {
            let mut children = vec![];
            let mut at = 0u64;
            let mut clips: Vec<&Clip> = t.clips.iter().collect();
            clips.sort_by_key(|c| c.start);
            for c in clips {
                if c.start > at {
                    children.push(json!({ "OTIO_SCHEMA": "Gap.1", "name": "", "source_range": range(0.0, (c.start - at) as f64, rate) }));
                }
                let effects = if (c.speed - 1.0).abs() > 1e-4 {
                    vec![json!({ "OTIO_SCHEMA": "LinearTimeWarp.1", "name": "speed", "effect_name": "LinearTimeWarp", "time_scalar": c.speed })]
                } else {
                    vec![]
                };
                children.push(json!({
                    "OTIO_SCHEMA": "Clip.2",
                    "name": name_of(&c.source),
                    "source_range": range(c.source_in * rate, c.duration as f64, rate),
                    "effects": effects,
                    "media_references": { "DEFAULT_MEDIA": {
                        "OTIO_SCHEMA": "ExternalReference.1",
                        "name": name_of(&c.source),
                        "target_url": file_url(&c.source),
                    }},
                    "active_media_reference_key": "DEFAULT_MEDIA",
                    "metadata": { "arabic_video_editor": { "volume": c.volume, "kind": c.media } },
                }));
                at = c.start + c.duration;
            }
            json!({
                "OTIO_SCHEMA": "Track.1",
                "name": t.id,
                "kind": if t.kind == TrackKind::Video { "Video" } else { "Audio" },
                "enabled": !(t.hidden || t.muted),
                "children": children,
            })
        })
        .collect();
    let doc = json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": p.name,
        "global_start_time": rt(0.0, rate),
        "metadata": { "arabic_video_editor": { "width": p.width, "height": p.height, "fps": p.fps } },
        "tracks": { "OTIO_SCHEMA": "Stack.1", "name": "tracks", "children": tracks },
    });
    Exported { text: serde_json::to_string_pretty(&doc).unwrap_or_default(), warnings: warnings(p, true) }
}

/// The value of an OTIO RationalTime, converted to `rate`.
fn rt_at(v: &Value, rate: f64) -> f64 {
    let value = v["value"].as_f64().unwrap_or(0.0);
    let r = v["rate"].as_f64().filter(|r| *r > 0.0).unwrap_or(rate);
    value * rate / r
}

/// Reads an OpenTimelineIO file. Size and frame rate come from the file
/// when it says, otherwise from `fallback`.
pub fn from_otio(text: &str, fallback: (u32, u32, Fps)) -> Result<Project> {
    let doc: Value = serde_json::from_str(text)?;
    if !doc["OTIO_SCHEMA"].as_str().unwrap_or("").starts_with("Timeline") {
        return Err(Error::Interchange("not an OpenTimelineIO timeline".into()));
    }
    let meta = &doc["metadata"]["arabic_video_editor"];
    let (mut w, mut h, mut fps) = fallback;
    if let (Some(mw), Some(mh)) = (meta["width"].as_u64(), meta["height"].as_u64()) {
        (w, h) = (mw as u32, mh as u32);
    }
    if let Ok(f) = serde_json::from_value::<Fps>(meta["fps"].clone()) {
        fps = f;
    } else if let Some(r) = first_rate(&doc["tracks"]) {
        fps = fps_from_rate(r);
    }
    let rate = fps_f(fps);
    let mut p = Project::new(doc["name"].as_str().unwrap_or("OTIO"), w, h, fps);
    p.tracks.clear();
    let mut n = 0;
    for (ti, t) in doc["tracks"]["children"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let kind = if t["kind"].as_str() == Some("Audio") { TrackKind::Audio } else { TrackKind::Video };
        let mut track = Track {
            id: format!("{}{}", if kind == TrackKind::Video { "v" } else { "a" }, ti + 1),
            kind,
            muted: false,
            hidden: false,
            locked: false,
            duck: false,
            clips: vec![],
        };
        if t["enabled"] == false {
            if kind == TrackKind::Video {
                track.hidden = true
            } else {
                track.muted = true
            }
        }
        let mut at = 0f64;
        for item in t["children"].as_array().cloned().unwrap_or_default() {
            let schema = item["OTIO_SCHEMA"].as_str().unwrap_or("");
            let dur = rt_at(&item["source_range"]["duration"], rate);
            if schema.starts_with("Gap") {
                at += dur;
                continue;
            }
            if !schema.starts_with("Clip") {
                continue; // transitions and nested stacks are not supported
            }
            let reference = match item["active_media_reference_key"].as_str() {
                Some(k) => &item["media_references"][k],
                None => &item["media_reference"],
            };
            let Some(url) = reference["target_url"].as_str() else {
                at += dur;
                continue;
            };
            let source = path_from_url(url);
            let start_src = rt_at(&item["source_range"]["start_time"], rate) / rate;
            let speed = item["effects"]
                .as_array()
                .and_then(|e| e.iter().find_map(|e| e["time_scalar"].as_f64()))
                .unwrap_or(1.0) as f32;
            let media = if kind == TrackKind::Audio { MediaKind::Audio } else { kind_of(&source) };
            let mut c = Clip::new(&new_id(&mut n), media, source, at.round() as u64, dur.round().max(1.0) as u64, start_src);
            if speed > 0.0 {
                c.speed = speed;
            }
            if let Some(v) = item["metadata"]["arabic_video_editor"]["volume"].as_f64() {
                c.volume = v as f32;
            }
            track.clips.push(c);
            at += dur;
        }
        p.tracks.push(track);
    }
    if p.tracks.is_empty() {
        p.tracks = Project::new("", w, h, fps).tracks;
    }
    Ok(p)
}

fn first_rate(v: &Value) -> Option<f64> {
    match v {
        Value::Object(m) => {
            if m.get("OTIO_SCHEMA").and_then(Value::as_str) == Some("RationalTime.1") {
                return m.get("rate").and_then(Value::as_f64);
            }
            m.values().find_map(first_rate)
        }
        Value::Array(a) => a.iter().find_map(first_rate),
        _ => None,
    }
}

fn fps_from_rate(r: f64) -> Fps {
    for (num, den) in [(24000, 1001), (30000, 1001), (60000, 1001)] {
        if (r - num as f64 / den as f64).abs() < 0.01 {
            return Fps::new(num, den);
        }
    }
    Fps::new(r.round().max(1.0) as u32, 1)
}

// ------------------------------------------------------------ file URLs

/// `file://` address of a path, the way editors write it.
pub fn file_url(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let s = if s.starts_with('/') { s } else { format!("/{s}") };
    let mut out = String::from("file://localhost");
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The path in a `file://` address (also accepts plain paths).
pub fn path_from_url(url: &str) -> PathBuf {
    let rest = url.strip_prefix("file://localhost").or_else(|| url.strip_prefix("file://")).unwrap_or(url);
    let bytes = rest.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&out).into_owned();
    // "/C:/Users/..." is a Windows drive path.
    let b = s.as_bytes();
    if b.len() > 3 && b[0] == b'/' && b[2] == b':' && b[1].is_ascii_alphabetic() {
        return PathBuf::from(&s[1..]);
    }
    PathBuf::from(s)
}

// -------------------------------------------------- Final Cut Pro 7 XML

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Timebase and NTSC flag the way FCP 7 XML writes a frame rate.
fn timebase(f: Fps) -> (u32, bool) {
    let ntsc = f.den == 1001;
    ((fps_f(f)).round() as u32, ntsc)
}

/// A Final Cut Pro 7 XML file (Premiere Pro: File → Import; Resolve:
/// File → Import Timeline).
pub fn to_fcp7_xml(p: &Project) -> Exported {
    let (tb, ntsc) = timebase(p.fps);
    let rate = format!("<rate><timebase>{tb}</timebase><ntsc>{}</ntsc></rate>", if ntsc { "TRUE" } else { "FALSE" });
    let fps = fps_f(p.fps);
    let mut files: HashMap<PathBuf, String> = HashMap::new();
    let mut item = 0;
    let mut clipitem = |c: &Clip, files: &mut HashMap<PathBuf, String>, audio: bool| -> String {
        item += 1;
        let source_in = (c.source_in * fps).round() as u64;
        // Without a time-remap effect, the source part is as long as the clip.
        let source_out = source_in + c.duration;
        let file = match files.get(&c.source) {
            Some(id) => format!("<file id=\"{id}\"/>"),
            None => {
                let id = format!("file-{}", files.len() + 1);
                files.insert(c.source.clone(), id.clone());
                let media = if c.media == MediaKind::Audio {
                    "<media><audio/></media>".to_string()
                } else {
                    format!("<media><video><samplecharacteristics><width>{}</width><height>{}</height></samplecharacteristics></video></media>", p.width, p.height)
                };
                format!("<file id=\"{id}\"><name>{}</name><pathurl>{}</pathurl>{rate}{media}</file>", esc(&name_of(&c.source)), esc(&file_url(&c.source)))
            }
        };
        let level = if audio && (c.volume - 1.0).abs() > 1e-3 {
            let db = 20.0 * c.volume.max(1e-4).log10();
            format!("<filter><effect><name>Audio Levels</name><effectid>audiolevels</effectid><effectcategory>audiolevels</effectcategory><effecttype>audiolevels</effecttype><mediatype>audio</mediatype><parameter><parameterid>level</parameterid><name>Level</name><valuemin>0</valuemin><valuemax>3.98109</valuemax><value>{:.5}</value></parameter></effect></filter><!-- {db:.1} dB -->", c.volume)
        } else {
            String::new()
        };
        format!(
            "<clipitem id=\"clipitem-{item}\"><name>{}</name><enabled>TRUE</enabled><duration>{}</duration>{rate}<start>{}</start><end>{}</end><in>{source_in}</in><out>{source_out}</out>{file}{level}</clipitem>",
            esc(&name_of(&c.source)),
            source_out,
            c.start,
            c.start + c.duration,
        )
    };
    let mut video = String::new();
    let mut audio = String::new();
    for t in p.tracks.iter().filter(|t| t.kind == TrackKind::Video) {
        let items: String = t.clips.iter().map(|c| clipitem(c, &mut files, false)).collect();
        video += &format!("<track><enabled>{}</enabled>{items}</track>", if t.hidden { "FALSE" } else { "TRUE" });
    }
    // The sound of video clips, on audio tracks of their own.
    for t in p.tracks.iter().filter(|t| t.kind == TrackKind::Video) {
        let items: String = t.clips.iter().filter(|c| c.media == MediaKind::Video).map(|c| clipitem(c, &mut files, true)).collect();
        if !items.is_empty() {
            audio += &format!("<track><enabled>{}</enabled>{items}</track>", if t.muted { "FALSE" } else { "TRUE" });
        }
    }
    for t in p.tracks.iter().filter(|t| t.kind == TrackKind::Audio) {
        let items: String = t.clips.iter().map(|c| clipitem(c, &mut files, true)).collect();
        audio += &format!("<track><enabled>{}</enabled>{items}</track>", if t.muted { "FALSE" } else { "TRUE" });
    }
    let text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xmeml>\n<xmeml version=\"4\"><sequence id=\"sequence-1\"><name>{}</name><duration>{}</duration>{rate}\
         <timecode>{rate}<string>00:00:00:00</string><frame>0</frame><displayformat>NDF</displayformat></timecode>\
         <media><video><format><samplecharacteristics>{rate}<width>{}</width><height>{}</height><pixelaspectratio>square</pixelaspectratio></samplecharacteristics></format>{video}</video>\
         <audio>{audio}</audio></media></sequence></xmeml>\n",
        esc(&p.name),
        p.duration(),
        p.width,
        p.height,
    );
    Exported { text, warnings: warnings(p, false) }
}

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, tag: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(tag))
}
fn text_of(n: Option<roxmltree::Node>) -> String {
    n.and_then(|n| n.text()).map(str::trim).unwrap_or("").to_string()
}
fn num(n: roxmltree::Node, tag: &str) -> Option<i64> {
    text_of(child(n, tag)).parse::<i64>().ok()
}

/// Reads a Final Cut Pro 7 XML file (as Premiere Pro and Resolve export).
pub fn from_fcp7_xml(text: &str) -> Result<Project> {
    // Premiere writes a <!DOCTYPE xmeml> line, so a DTD must be allowed.
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = roxmltree::Document::parse_with_options(text, opts).map_err(|e| Error::Interchange(e.to_string()))?;
    let seq = doc.descendants().find(|n| n.has_tag_name("sequence")).ok_or_else(|| Error::Interchange("no sequence in the file".into()))?;

    let rate = child(seq, "rate");
    let tb = rate.and_then(|r| num(r, "timebase")).unwrap_or(30).max(1) as u32;
    let ntsc = rate.map(|r| text_of(child(r, "ntsc")).eq_ignore_ascii_case("TRUE")).unwrap_or(false);
    let fps = if ntsc { Fps::new(tb * 1000, 1001) } else { Fps::new(tb, 1) };
    let sc = seq.descendants().find(|n| n.has_tag_name("samplecharacteristics") && child(*n, "width").is_some());
    let (w, h) = sc.map(|s| (num(s, "width").unwrap_or(1920) as u32, num(s, "height").unwrap_or(1080) as u32)).unwrap_or((1920, 1080));

    // Files are written in full once, then referred to by id.
    let mut paths: HashMap<String, PathBuf> = HashMap::new();
    for f in doc.descendants().filter(|n| n.has_tag_name("file")) {
        if let (Some(id), Some(url)) = (f.attribute("id"), child(f, "pathurl").and_then(|u| u.text())) {
            paths.insert(id.to_string(), path_from_url(url.trim()));
        }
    }

    let mut p = Project::new(&text_of(child(seq, "name")), w, h, fps);
    p.tracks.clear();
    let media = child(seq, "media").ok_or_else(|| Error::Interchange("no media in the sequence".into()))?;
    let mut n = 0;
    let mut placed_video: Vec<(PathBuf, u64)> = vec![];
    for (kind, tag) in [(TrackKind::Video, "video"), (TrackKind::Audio, "audio")] {
        let Some(group) = child(media, tag) else { continue };
        for (ti, t) in group.children().filter(|c| c.has_tag_name("track")).enumerate() {
            let enabled = !text_of(child(t, "enabled")).eq_ignore_ascii_case("FALSE");
            let mut track = Track {
                id: format!("{}{}", if kind == TrackKind::Video { "v" } else { "a" }, ti + 1),
                kind,
                muted: kind == TrackKind::Audio && !enabled,
                hidden: kind == TrackKind::Video && !enabled,
                locked: false,
                duck: false,
                clips: vec![],
            };
            let mut last_end = 0i64;
            for ci in t.children().filter(|c| c.has_tag_name("clipitem")) {
                let Some(file) = child(ci, "file") else { continue };
                let Some(source) = file.attribute("id").and_then(|id| paths.get(id)).cloned() else { continue };
                // -1 means "decided by a transition": use the neighbour's edge.
                let start = num(ci, "start").filter(|v| *v >= 0).unwrap_or(last_end);
                let end = num(ci, "end").filter(|v| *v >= 0).unwrap_or(start);
                let src_in = num(ci, "in").unwrap_or(0).max(0);
                let src_out = num(ci, "out").unwrap_or(src_in + end - start);
                last_end = end;
                if end <= start {
                    continue;
                }
                let (start, dur) = (start as u64, (end - start) as u64);
                // The sound of a video clip already comes with the clip here.
                if kind == TrackKind::Audio && placed_video.iter().any(|(s, at)| *s == source && *at == start) {
                    continue;
                }
                let m = if kind == TrackKind::Audio { MediaKind::Audio } else { kind_of(&source) };
                if kind == TrackKind::Video {
                    placed_video.push((source.clone(), start));
                }
                let mut c = Clip::new(&new_id(&mut n), m, source, start, dur, src_in as f64 / fps_f(fps));
                let used = (src_out - src_in).max(1) as f32;
                if m == MediaKind::Video && (used / dur as f32 - 1.0).abs() > 0.02 {
                    c.speed = used / dur as f32;
                }
                track.clips.push(c);
            }
            if !track.clips.is_empty() || kind == TrackKind::Video {
                p.tracks.push(track);
            }
        }
    }
    if !p.tracks.iter().any(|t| t.kind == TrackKind::Audio) {
        p.tracks.push(Track { id: "a1".into(), kind: TrackKind::Audio, muted: false, hidden: false, locked: false, duck: false, clips: vec![] });
    }
    Ok(p)
}

// ---------------------------------------------------------------- EDL

fn timecode(frames: u64, nominal: u64) -> String {
    let (f, s) = (frames % nominal, frames / nominal);
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60, f)
}

/// A CMX 3600 EDL: the main picture track and the first audio track, cuts
/// only. Clip names and full paths are written as comments, which Premiere
/// and Resolve use to find the files.
pub fn to_edl(p: &Project) -> Exported {
    let nominal = fps_f(p.fps).round().max(1.0) as u64;
    let mut out = format!("TITLE: {}\nFCM: NON-DROP FRAME\n\n", p.name.replace('\n', " "));
    let mut events: Vec<(&Clip, &str)> = vec![];
    if let Some(v) = p.tracks.iter().find(|t| t.kind == TrackKind::Video) {
        events.extend(v.clips.iter().map(|c| (c, if c.media == MediaKind::Video { "B" } else { "V" })));
    }
    if let Some(a) = p.tracks.iter().find(|t| t.kind == TrackKind::Audio) {
        events.extend(a.clips.iter().map(|c| (c, "A")));
    }
    events.sort_by_key(|(c, ch)| (c.start, *ch != "B" && *ch != "V"));
    for (i, (c, ch)) in events.iter().enumerate() {
        let src_in = (c.source_in * nominal as f64).round() as u64;
        let src_out = src_in + (c.duration as f64 * c.speed as f64).round() as u64;
        let reel = "AX";
        out += &format!(
            "{:03}  {reel:<8} {ch:<5} C        {} {} {} {}\n",
            i + 1,
            timecode(src_in, nominal),
            timecode(src_out, nominal),
            timecode(c.start, nominal),
            timecode(c.start + c.duration, nominal),
        );
        if (c.speed - 1.0).abs() > 1e-3 {
            out += &format!("M2   {reel:<8}       {:05.1}    {}\n", nominal as f32 * c.speed, timecode(src_in, nominal));
        }
        out += &format!("* FROM CLIP NAME: {}\n* SOURCE FILE: {}\n\n", name_of(&c.source), c.source.to_string_lossy());
    }
    let mut w = warnings(p, true);
    if p.tracks.iter().filter(|t| t.kind == TrackKind::Video && !t.clips.is_empty()).count() > 1 || p.tracks.iter().filter(|t| t.kind == TrackKind::Audio && !t.clips.is_empty()).count() > 1 {
        w.push("tracks".into());
    }
    Exported { text: out, warnings: w }
}

/// Picks the reader by file name.
pub fn import_timeline(path: &Path, fallback: (u32, u32, Fps)) -> Result<Project> {
    let text = std::fs::read_to_string(path)?;
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "otio" | "json" => from_otio(&text, fallback),
        "xml" | "fcpxml7" => from_fcp7_xml(&text),
        _ => Err(Error::Interchange("use an .otio or .xml (Final Cut Pro 7 XML) file".into())),
    }
}
