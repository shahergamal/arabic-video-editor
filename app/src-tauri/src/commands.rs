//! Everything the window can ask the engine to do.

use engine::export::{export, ExportSettings};
use engine::media::{probe, thumbnail as make_thumbnail, waveform as make_waveform, MediaInfo};
use engine::{Compositor, Project};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter};

pub struct State {
    bundled_fonts: Option<PathBuf>,
    user_fonts: PathBuf,
    data_dir: PathBuf,
    project: Mutex<Option<Project>>,
    preview: Mutex<Compositor>,
    cancel: Arc<AtomicBool>,
}

impl State {
    pub fn new(bundled_fonts: Option<PathBuf>, data_dir: PathBuf) -> Self {
        let user_fonts = data_dir.join("fonts");
        let _ = std::fs::create_dir_all(&user_fonts);
        let s = Self {
            preview: Mutex::new(Compositor::new(None)),
            bundled_fonts,
            user_fonts,
            data_dir,
            project: Mutex::new(None),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        *s.preview.lock().unwrap() = Compositor::with_font_dirs(&s.font_dirs());
        s
    }

    fn font_dirs(&self) -> Vec<PathBuf> {
        self.bundled_fonts.iter().cloned().chain([self.user_fonts.clone()]).collect()
    }

    fn autosave(&self) -> PathBuf {
        self.data_dir.join("autosave.ave")
    }

    fn recent_file(&self) -> PathBuf {
        self.data_dir.join("recent.json")
    }
}

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------- media ----------

#[tauri::command]
pub async fn probe_media(path: PathBuf) -> Res<MediaInfo> {
    tauri::async_runtime::spawn_blocking(move || probe(&path).map_err(err)).await.map_err(err)?
}

#[tauri::command]
pub async fn thumbnail(path: PathBuf) -> Res<Response> {
    tauri::async_runtime::spawn_blocking(move || make_thumbnail(&path, 240).map(Response::new).map_err(err))
        .await
        .map_err(err)?
}

#[tauri::command]
pub async fn waveform(path: PathBuf, buckets: usize) -> Res<Vec<f32>> {
    tauri::async_runtime::spawn_blocking(move || make_waveform(&path, buckets).map_err(err)).await.map_err(err)?
}

// ---------- fonts ----------

#[derive(Serialize)]
pub struct FontInfo {
    family: String,
    /// modern, kufi, naskh, ruqaa, nastaliq, display, handwritten, or "" for computer fonts.
    category: String,
    /// "bundled", "added" (by the user) or "system".
    origin: String,
    license: String,
    commercial_use: Option<bool>,
    /// Font files, so the window can show previews in this font.
    files: Vec<PathBuf>,
}

#[derive(Deserialize)]
struct Manifest {
    fonts: Vec<ManifestFont>,
}
#[derive(Deserialize)]
struct ManifestFont {
    family: String,
    category: String,
    license: String,
    commercial_use: bool,
}

#[tauri::command]
pub fn list_fonts(state: tauri::State<State>) -> Vec<FontInfo> {
    let manifest: Vec<ManifestFont> = state
        .bundled_fonts
        .as_ref()
        .and_then(|d| std::fs::read_to_string(d.join("manifest.json")).ok())
        .and_then(|s| serde_json::from_str::<Manifest>(&s).ok())
        .map(|m| m.fonts)
        .unwrap_or_default();
    let added = engine::text::TextRenderer::with_dirs([state.user_fonts.as_path()]).families();
    let (all, files) = {
        let p = state.preview.lock().unwrap();
        (p.text.families(), p.text.font_files())
    };
    let ours = |path: &Path| {
        state.bundled_fonts.as_ref().is_some_and(|b| path.starts_with(b)) || path.starts_with(&state.user_fonts)
    };
    let files_of = |fam: &str| -> Vec<PathBuf> {
        files.iter().filter(|(f, p)| f == fam && ours(p)).map(|(_, p)| p.clone()).collect()
    };
    let system_only = engine::text::TextRenderer::with_dirs(std::iter::empty::<&Path>()).families();
    let mut out: Vec<FontInfo> = manifest
        .iter()
        .map(|f| FontInfo {
            family: f.family.clone(),
            category: f.category.clone(),
            origin: "bundled".into(),
            license: f.license.clone(),
            commercial_use: Some(f.commercial_use),
            files: files_of(&f.family),
        })
        .collect();
    for fam in all {
        if out.iter().any(|f| f.family == fam) {
            continue;
        }
        let origin = if added.contains(&fam) && !system_only.contains(&fam) { "added" } else { "system" };
        let files = if origin == "added" { files_of(&fam) } else { vec![] };
        out.push(FontInfo { family: fam, category: String::new(), origin: origin.into(), license: String::new(), commercial_use: None, files });
    }
    out
}

/// Copies a font file the user picked into the app's own font folder, so it
/// keeps working even if the original file is moved.
#[tauri::command]
pub fn add_font(state: tauri::State<State>, path: PathBuf) -> Res<()> {
    let name = path.file_name().ok_or("bad file name")?;
    let dest = state.user_fonts.join(name);
    std::fs::copy(&path, &dest).map_err(err)?;
    state.preview.lock().unwrap().text.add_font_file(&dest).map_err(err)
}

// ---------- project ----------

/// The window sends the whole project after every change. It is kept in
/// memory for drawing and written to the autosave file straight away, so a
/// crash or power cut loses nothing.
#[tauri::command]
pub fn update_project(state: tauri::State<State>, project: Project) -> Res<()> {
    project.save(&state.autosave()).map_err(err)?;
    *state.project.lock().unwrap() = Some(project);
    Ok(())
}

/// After a crash, the last autosaved project (if any).
#[tauri::command]
pub fn recover_autosave(state: tauri::State<State>) -> Option<Project> {
    Project::load_or_recover(&state.autosave()).ok().map(|(p, _)| p)
}

/// Clears the autosave after the user chose to start fresh.
#[tauri::command]
pub fn discard_autosave(state: tauri::State<State>) {
    let _ = std::fs::remove_file(state.autosave());
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Recent {
    path: PathBuf,
    name: String,
    /// Seconds since 1970, for sorting and "last opened" text.
    opened: u64,
}

fn read_recent(state: &State) -> Vec<Recent> {
    std::fs::read_to_string(state.recent_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn remember(state: &State, path: &Path, name: &str) {
    let mut list = read_recent(state);
    list.retain(|r| r.path != path);
    let opened = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    list.insert(0, Recent { path: path.to_path_buf(), name: name.to_string(), opened });
    list.truncate(30);
    if let Ok(json) = serde_json::to_vec_pretty(&list) {
        let _ = std::fs::write(state.recent_file(), json);
    }
}

#[tauri::command]
pub fn list_recent(state: tauri::State<State>) -> Vec<Recent> {
    read_recent(&state).into_iter().filter(|r| r.path.exists()).collect()
}

#[tauri::command]
pub fn forget_recent(state: tauri::State<State>, path: PathBuf) {
    let mut list = read_recent(&state);
    list.retain(|r| r.path != path);
    if let Ok(json) = serde_json::to_vec_pretty(&list) {
        let _ = std::fs::write(state.recent_file(), json);
    }
}

#[tauri::command]
pub fn save_project(state: tauri::State<State>, project: Project, path: PathBuf) -> Res<()> {
    project.save(&path).map_err(err)?;
    remember(&state, &path, &project.name);
    Ok(())
}

#[tauri::command]
pub fn open_project(state: tauri::State<State>, path: PathBuf) -> Res<Project> {
    let p = Project::load_or_recover(&path).map(|(p, _)| p).map_err(err)?;
    remember(&state, &path, &p.name);
    Ok(p)
}

/// Files the project uses that are no longer where they were.
#[tauri::command]
pub fn missing_media(project: Project) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = project.all_clips().map(|(_, c)| c.source.clone()).filter(|p| !p.exists()).collect();
    out.sort();
    out.dedup();
    out
}

// ---------- preview ----------

/// One preview frame, as a JPEG. Passing raw pixels to the window turned out
/// to be the slowest step of playback (about 160 ms a frame on Linux), while
/// a JPEG is about 20 times smaller and takes a few ms to make. This is only
/// the on-screen preview: export always uses the full-quality pixels.
#[tauri::command]
pub async fn render_frame(state: tauri::State<'_, State>, index: u64, width: u32, height: u32) -> Res<Response> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let frame = state.preview.lock().unwrap().render(&project, index, width, height).map_err(err)?;
    let mut jpeg = Vec::with_capacity(frame.data.len() / 10);
    jpeg_encoder::Encoder::new(&mut jpeg, 90)
        .encode(&frame.data, width as u16, height as u16, jpeg_encoder::ColorType::Rgba)
        .map_err(err)?;
    Ok(Response::new(jpeg))
}

// ---------- export ----------

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
struct Finished {
    ok: bool,
    message: String,
}

#[derive(Deserialize)]
pub struct ExportRequest {
    path: PathBuf,
    /// 1.0 = project size, 0.5 = half (faster, smaller file).
    scale: f32,
    /// x264 quality, 16 (best) to 28 (smallest).
    crf: u8,
}

/// Exports in the background; progress arrives as "export-progress" events
/// and the result as one "export-finished" event.
#[tauri::command]
pub fn start_export(app: AppHandle, state: tauri::State<State>, request: ExportRequest) -> Res<()> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let dirs = state.font_dirs();
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let mut comp = Compositor::with_font_dirs(&dirs);
        let even = |v: f32| ((v / 2.0).round() as u32 * 2).max(2);
        let settings = ExportSettings {
            width: even(project.width as f32 * request.scale),
            height: even(project.height as f32 * request.scale),
            crf: request.crf.clamp(10, 35),
        };
        let mut last = std::time::Instant::now();
        let res = export(&project, &mut comp, &settings, &request.path, &cancel, |done, total| {
            if last.elapsed().as_millis() > 100 || done == total {
                last = std::time::Instant::now();
                let _ = app.emit("export-progress", Progress { done, total });
            }
        });
        let msg = match &res {
            Ok(()) => request.path.display().to_string(),
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
