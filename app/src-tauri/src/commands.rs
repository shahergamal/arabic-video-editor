//! Everything the window can ask the engine to do.

use engine::export::{export, ExportSettings};
use engine::media::{probe, MediaInfo};
use engine::{Compositor, Project};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter};

pub struct State {
    fonts: Option<PathBuf>,
    autosave: PathBuf,
    project: Mutex<Option<Project>>,
    preview: Mutex<Compositor>,
    cancel: Arc<AtomicBool>,
}

impl State {
    pub fn new(fonts: Option<PathBuf>, data_dir: PathBuf) -> Self {
        Self {
            preview: Mutex::new(Compositor::new(fonts.as_deref())),
            fonts,
            autosave: data_dir.join("autosave.ave"),
            project: Mutex::new(None),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn probe_media(path: PathBuf) -> Res<MediaInfo> {
    probe(&path).map_err(err)
}

#[tauri::command]
pub fn list_fonts(state: tauri::State<State>) -> Vec<String> {
    state.preview.lock().unwrap().text.families()
}

/// The window sends the whole project after every change. It is kept in
/// memory for drawing and written to the autosave file straight away, so a
/// crash or power cut loses nothing.
#[tauri::command]
pub fn update_project(state: tauri::State<State>, project: Project) -> Res<()> {
    project.save(&state.autosave).map_err(err)?;
    *state.project.lock().unwrap() = Some(project);
    Ok(())
}

/// After a crash, the last autosaved project (if any).
#[tauri::command]
pub fn recover_autosave(state: tauri::State<State>) -> Option<Project> {
    Project::load_or_recover(&state.autosave).ok().map(|(p, _)| p)
}

#[tauri::command]
pub fn save_project(project: Project, path: PathBuf) -> Res<()> {
    project.save(&path).map_err(err)
}

#[tauri::command]
pub fn open_project(path: PathBuf) -> Res<Project> {
    Project::load_or_recover(&path).map(|(p, _)| p).map_err(err)
}

/// One preview frame, as a JPEG. Passing raw pixels to the window turned out
/// to be the slowest step of playback (about 160 ms a frame on Linux), while
/// a JPEG is about 20 times smaller and takes a few ms to make. This is only
/// the on-screen preview: export always uses the full-quality pixels.
#[tauri::command]
pub fn render_frame(state: tauri::State<State>, index: u64, width: u32, height: u32) -> Res<Response> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let frame = state.preview.lock().unwrap().render(&project, index, width, height).map_err(err)?;
    let mut jpeg = Vec::with_capacity(frame.data.len() / 10);
    jpeg_encoder::Encoder::new(&mut jpeg, 90)
        .encode(&frame.data, width as u16, height as u16, jpeg_encoder::ColorType::Rgba)
        .map_err(err)?;
    Ok(Response::new(jpeg))
}

#[derive(Clone, serde::Serialize)]
struct Progress {
    done: u64,
    total: u64,
}

#[derive(Clone, serde::Serialize)]
struct Finished {
    ok: bool,
    message: String,
}

/// Exports in the background; progress arrives as "export-progress" events
/// and the result as one "export-finished" event.
#[tauri::command]
pub fn start_export(app: AppHandle, state: tauri::State<State>, path: PathBuf) -> Res<()> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let fonts = state.fonts.clone();
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let mut comp = Compositor::new(fonts.as_deref());
        let settings = ExportSettings::for_project(&project);
        let mut last = std::time::Instant::now();
        let res = export(&project, &mut comp, &settings, &path, &cancel, |done, total| {
            if last.elapsed().as_millis() > 100 || done == total {
                last = std::time::Instant::now();
                let _ = app.emit("export-progress", Progress { done, total });
            }
        });
        let msg = match &res {
            Ok(()) => path.display().to_string(),
            Err(e) => e.to_string(),
        };
        let _ = app.emit("export-finished", Finished { ok: res.is_ok(), message: msg });
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_export(state: tauri::State<State>) {
    state.cancel.store(true, Ordering::Relaxed);
}
