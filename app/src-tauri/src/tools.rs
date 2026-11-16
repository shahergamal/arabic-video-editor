//! Phase 3 tools: background removal, tracking, scopes, quiet parts.

use crate::captions::{download_to, Finished, ModelInfo, Progress};
use crate::commands::State;
use engine::compose::MatteSetup;
use engine::{Compositor, MaskPoint, Project};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter};

struct MatteModel {
    id: &'static str,
    file: &'static str,
    mb: u32,
}

/// U²-Net models (Apache-2.0), as published with the rembg project.
const MATTE_MODELS: &[MatteModel] = &[
    MatteModel { id: "people", file: "u2net_human_seg.onnx", mb: 176 },
    MatteModel { id: "objects", file: "u2netp.onnx", mb: 5 },
];
const MATTE_URL: &str = "https://github.com/danielgatis/rembg/releases/download/v0.0.0/";

pub fn matte_setup(data_dir: &std::path::Path) -> MatteSetup {
    let models = MATTE_MODELS.iter().map(|m| (m.id.to_string(), data_dir.join("models").join(m.file))).collect();
    MatteSetup::new(data_dir.join("mattes"), models)
}

#[tauri::command]
pub fn matte_models(state: tauri::State<State>) -> Vec<ModelInfo> {
    MATTE_MODELS
        .iter()
        .map(|m| ModelInfo { id: m.id.into(), mb: m.mb, downloaded: state.data_dir.join("models").join(m.file).exists() })
        .collect()
}

#[tauri::command]
pub fn download_matte_model(app: AppHandle, state: tauri::State<State>, id: String) -> Result<(), String> {
    let m = MATTE_MODELS.iter().find(|m| m.id == id).ok_or("unknown model")?;
    let dest = state.data_dir.join("models").join(m.file);
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let url = format!("{MATTE_URL}{}", m.file);
    std::thread::spawn(move || {
        let res = download_to(&app, &url, &dest, &cancel, m.mb, "model-progress");
        let _ = app.emit("model-finished", Finished::<()> { ok: res.is_ok(), message: res.err().unwrap_or_default(), result: None });
    });
    Ok(())
}

fn find_clip(project: &Project, id: &str) -> Result<engine::Clip, String> {
    project.all_clips().map(|(_, c)| c).find(|c| c.id == id).cloned().ok_or_else(|| "clip not found".into())
}

/// Works out the cut-out masks for every frame of a clip ahead of time,
/// using several threads, so playback stays smooth afterwards.
/// Events: "matte-progress", then "matte-finished".
#[tauri::command]
pub fn prepare_matte(app: AppHandle, state: tauri::State<State>, clip_id: String) -> Result<(), String> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let clip = find_clip(&project, &clip_id)?;
    if clip.remove_bg.is_none() {
        return Err("background removal is off for this clip".into());
    }
    // Only this clip, alone, so the work is just the mask.
    let mut solo = Project::new("", project.width, project.height, project.fps);
    solo.track_mut(engine::TrackKind::Video).clips.push(clip.clone());
    let data = state.data_dir.clone();
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8) as u64;
        let total = clip.duration;
        let chunk = total.div_ceil(threads);
        // Render at a modest size: the network only looks at 320×320.
        let k = 640.0 / solo.width.max(solo.height) as f32;
        let (w, h) = (((solo.width as f32 * k) as u32 / 2 * 2).max(2), ((solo.height as f32 * k) as u32 / 2 * 2).max(2));
        let done = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let mut handles = vec![];
        for t in 0..threads {
            let (solo, data, cancel, done, app) = (solo.clone(), data.clone(), cancel.clone(), done.clone(), app.clone());
            let (from, to) = (clip.start + t * chunk, (clip.start + (t + 1) * chunk).min(clip.end()));
            handles.push(std::thread::spawn(move || -> Result<(), String> {
                let mut comp = Compositor::new(None);
                comp.mattes = Some(matte_setup(&data));
                for i in from..to {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("cancelled".into());
                    }
                    comp.render(&solo, i, w, h).map_err(|e| e.to_string())?;
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 4 == 0 || n == total {
                        let _ = app.emit("matte-progress", Progress { done: n, total });
                    }
                }
                Ok(())
            }));
        }
        let res = handles.into_iter().map(|h| h.join().unwrap_or(Err("worker stopped".into()))).collect::<Result<Vec<_>, _>>();
        let _ = app.emit("matte-finished", Finished::<()> { ok: res.is_ok(), message: res.err().unwrap_or_default(), result: None });
    });
    Ok(())
}

/// Follows a box from the playhead to the end of the clip.
/// Events: "track-progress", then "track-finished" with the path.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn track_mask(app: AppHandle, state: tauri::State<State>, clip_id: String, from: u64, x: f32, y: f32, w: f32, h: f32) -> Result<(), String> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let clip = find_clip(&project, &clip_id)?;
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let progress_app = app.clone();
        let res = engine::track::track_box(&clip, project.width, project.height, project.fps, from, (x, y, w, h), &cancel, |done, total| {
            if done % 5 == 0 {
                let _ = progress_app.emit("track-progress", Progress { done, total });
            }
        })
        .map_err(|e| e.to_string());
        let msg = res.as_ref().err().cloned().unwrap_or_default();
        let _ = app.emit("track-finished", Finished::<Vec<MaskPoint>> { ok: res.is_ok(), message: msg, result: res.ok() });
    });
    Ok(())
}

/// Waveform, histogram and vectorscope of the frame under the playhead.
#[tauri::command]
pub async fn scopes(state: tauri::State<'_, State>, index: u64) -> Result<Response, String> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let k = 480.0 / project.width.max(project.height) as f32;
    let (w, h) = (((project.width as f32 * k) as u32 / 2 * 2).max(2), ((project.height as f32 * k) as u32 / 2 * 2).max(2));
    let frame = state.preview.lock().unwrap().render(&project, index, w, h).map_err(|e| e.to_string())?;
    let s = engine::scopes::scopes(&frame);
    let mut jpeg = vec![];
    jpeg_encoder::Encoder::new(&mut jpeg, 88)
        .encode(&s.data, s.width as u16, s.height as u16, jpeg_encoder::ColorType::Rgba)
        .map_err(|e| e.to_string())?;
    Ok(Response::new(jpeg))
}

/// Quiet stretches of a file, in seconds.
#[tauri::command]
pub async fn silences(path: PathBuf, threshold: f32, min_seconds: f32) -> Result<Vec<(f64, f64)>, String> {
    engine::media::silences(&path, threshold, min_seconds).map_err(|e| e.to_string())
}
