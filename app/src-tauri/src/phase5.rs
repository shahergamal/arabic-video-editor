//! Phase 5: working copies (steady / reversed), beats, following the
//! subject, graphics-card export, voice recording, new-version notice and
//! the automatic edit.

use crate::captions::{download_to, Finished, Progress};
use crate::commands::State;
use engine::analyze::ShotInfo;
use engine::autoedit::{self, Brief, Plan};
use engine::{Clip, Project};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter};

fn find_clip(project: &Project, id: &str) -> Result<Clip, String> {
    project.all_clips().map(|(_, c)| c).find(|c| c.id == id).cloned().ok_or_else(|| "clip not found".into())
}

pub fn copies_dir(data: &Path) -> PathBuf {
    data.join("copies")
}

/// Makes the steadied and/or reversed copy of a clip's file in the
/// background. Events: "copy-progress" (0-1000), then "copy-finished".
#[tauri::command]
pub fn prepare_copy(app: AppHandle, state: tauri::State<State>, clip_id: String) -> Result<(), String> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let clip = find_clip(&project, &clip_id)?;
    if !engine::derive::needs_copy(&clip) {
        let _ = app.emit("copy-finished", Finished::<()> { ok: true, message: String::new(), result: None });
        return Ok(());
    }
    let dir = copies_dir(&state.data_dir);
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let progress_app = app.clone();
        let res = engine::derive::ensure_copy(&dir, &clip, &cancel, &mut |p| {
            let _ = progress_app.emit("copy-progress", Progress { done: (p * 1000.0) as u64, total: 1000 });
        });
        let msg = res.as_ref().err().map(|e| e.to_string()).unwrap_or_default();
        let _ = app.emit("copy-finished", Finished::<()> { ok: res.is_ok(), message: msg, result: None });
    });
    Ok(())
}

/// Whether a clip's working copy is ready.
#[tauri::command]
pub fn copy_ready(state: tauri::State<State>, clip_id: String) -> Result<bool, String> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let clip = find_clip(&project, &clip_id)?;
    Ok(!engine::derive::needs_copy(&clip) || engine::derive::copy_path(&copies_dir(&state.data_dir), &clip).exists())
}

/// Beats of a music file, in seconds from the start of the file.
#[tauri::command]
pub async fn detect_beats(path: PathBuf, start: f64, length: f64) -> Result<engine::beats::Beats, String> {
    tauri::async_runtime::spawn_blocking(move || engine::beats::detect(&path, start, length.clamp(1.0, 900.0)).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Graphics-card encoders that work on this computer.
#[tauri::command]
pub async fn encoders() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(engine::export::hardware_encoders).await.unwrap_or_default()
}

/// The edit in another shape, with full-frame videos following their
/// subject. Events: "reframe-progress", then "reframe-finished" with the
/// new project.
#[tauri::command]
pub fn auto_reframe(app: AppHandle, state: tauri::State<State>, width: u32, height: u32, model: String) -> Result<(), String> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let setup = crate::tools::matte_setup(&state.data_dir);
    let path = setup.models.get(&model).cloned().filter(|p| p.exists()).ok_or("model not downloaded")?;
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let res = (|| -> Result<Project, String> {
            let matter = engine::matte::Matter::load(&path).map_err(|e| e.to_string())?;
            let size = |p: &Path| engine::media::probe(p).ok().map(|i| (i.width, i.height));
            let mut out = project.reframed(width, height, true, size);
            let list = engine::reframe::candidates(&project);
            let total = list.iter().map(|c| c.duration).sum::<u64>().max(1);
            let mut done = 0u64;
            for clip in list {
                let Some(src) = size(&clip.source) else { continue };
                let papp = app.clone();
                let path = engine::reframe::subject_path(&project, &clip, &matter, &cancel, &mut |p| {
                    let _ = papp.emit("reframe-progress", Progress { done: done + (p * clip.duration as f32) as u64, total });
                })
                .map_err(|e| e.to_string())?;
                done += clip.duration;
                let keys = engine::reframe::follow_keyframes(&engine::reframe::calm(&path, 0.04), src, width, height, 1.0);
                for t in &mut out.tracks {
                    for c in t.clips.iter_mut().filter(|c| c.id == clip.id) {
                        if let Some(k) = keys.first() {
                            c.transform = k.transform;
                        }
                        c.keyframes = if keys.len() > 1 { keys.clone() } else { vec![] };
                    }
                }
            }
            Ok(out)
        })();
        let msg = res.as_ref().err().cloned().unwrap_or_default();
        let _ = app.emit("reframe-finished", Finished { ok: res.is_ok(), message: msg, result: res.ok() });
    });
    Ok(())
}

// ---------- voice recording ----------

/// Saves a recording made in the window (raw bytes in the request body,
/// any format the browser records) as a WAV file in the app's own folder,
/// so it never depends on a temporary file. Returns the WAV's path.
#[tauri::command]
pub fn save_recording(state: tauri::State<State>, request: tauri::ipc::Request<'_>) -> Result<PathBuf, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw audio".into()) };
    if bytes.is_empty() {
        return Err("empty recording".into());
    }
    let dir = state.data_dir.join("recordings");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let raw = dir.join(format!("voice-{stamp}.rec"));
    std::fs::write(&raw, bytes).map_err(|e| e.to_string())?;
    let wav = dir.join(format!("voice-{stamp}.wav"));
    let out = std::process::Command::new(engine::media::ffmpeg_path())
        .args(["-v", "error", "-y", "-nostdin", "-i"])
        .arg(&raw)
        .args(["-vn", "-ac", "1", "-ar", "48000", "-c:a", "pcm_s16le"])
        .arg(&wav)
        .output()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&raw);
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(wav)
}

// ---------- new version ----------

const REPO: &str = "shahergamal/arabic-video-editor";

#[derive(Serialize)]
pub struct Update {
    version: String,
    url: String,
}

fn newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> { s.trim_start_matches('v').split(['.', '-']).map(|p| p.parse().unwrap_or(0)).take(3).collect() };
    parse(a) > parse(b)
}

/// Asks GitHub (only the public releases list; nothing about the user is
/// sent) whether a newer version is out. Checked at most once a day.
#[tauri::command]
pub async fn check_update(state: tauri::State<'_, State>) -> Result<Option<Update>, String> {
    let stamp = state.data_dir.join("update-check");
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let last: u64 = std::fs::read_to_string(&stamp).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    if now.saturating_sub(last) < 24 * 3600 {
        return Ok(None);
    }
    let _ = std::fs::write(&stamp, now.to_string());
    tauri::async_runtime::spawn_blocking(move || {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let mut resp = ureq::get(&url).header("User-Agent", "arabic-video-editor").header("Accept", "application/vnd.github+json").call().map_err(|e| e.to_string())?;
        let v: serde_json::Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
        let tag = v["tag_name"].as_str().unwrap_or_default().to_string();
        let page = v["html_url"].as_str().unwrap_or_default().to_string();
        Ok((newer(&tag, env!("CARGO_PKG_VERSION")) && page.starts_with("https://github.com/")).then_some(Update { version: tag.trim_start_matches('v').into(), url: page }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens one of our own pages (the releases page) in the web browser.
#[tauri::command]
pub fn open_release_page(url: String) -> Result<(), String> {
    if !url.starts_with(&format!("https://github.com/{REPO}/releases")) {
        return Err("not our releases page".into());
    }
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &url]).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(&url).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

// ---------- automatic edit ----------

/// How the idea is understood: "rules" (free, built in), "local" (a small
/// free model on this computer) or "claude" (the user's own Claude API key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    #[serde(default = "rules")]
    pub planner: String,
    #[serde(default)]
    pub claude_key: String,
    /// Most the user wants to spend in a month, in US dollars.
    #[serde(default = "cap")]
    pub monthly_cap: f64,
    /// Spent so far this month, and which month ("2026-11").
    #[serde(default)]
    pub spent: f64,
    #[serde(default)]
    pub month: String,
}

fn rules() -> String {
    "rules".into()
}
fn cap() -> f64 {
    2.0
}

impl Default for AiSettings {
    fn default() -> Self {
        Self { planner: rules(), claude_key: String::new(), monthly_cap: cap(), spent: 0.0, month: String::new() }
    }
}

fn ai_file(state: &State) -> PathBuf {
    state.data_dir.join("ai.json")
}

fn this_month() -> String {
    // Days since 1970 to a year-month (proleptic Gregorian), no date library needed.
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86400).unwrap_or(0) as i64;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y}-{m:02}")
}

fn load_ai(state: &State) -> AiSettings {
    let mut s: AiSettings = std::fs::read_to_string(ai_file(state)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if s.month != this_month() {
        s.month = this_month();
        s.spent = 0.0;
    }
    s
}

fn save_ai(state: &State, s: &AiSettings) -> Result<(), String> {
    std::fs::write(ai_file(state), serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Settings as the window sees them: the key itself is never sent back,
/// only whether one is saved.
#[derive(Serialize)]
pub struct AiView {
    planner: String,
    has_key: bool,
    monthly_cap: f64,
    spent: f64,
    local_ready: bool,
    local_mb: u32,
}

#[tauri::command]
pub fn ai_settings(state: tauri::State<State>) -> AiView {
    let s = load_ai(&state);
    AiView {
        planner: s.planner,
        has_key: !s.claude_key.is_empty(),
        monthly_cap: s.monthly_cap,
        spent: s.spent,
        local_ready: local::ready(&state.data_dir),
        local_mb: local::MODEL_MB,
    }
}

#[tauri::command]
pub fn set_ai_settings(state: tauri::State<State>, planner: String, monthly_cap: f64, claude_key: Option<String>) -> Result<(), String> {
    let mut s = load_ai(&state);
    if !["rules", "local", "claude"].contains(&planner.as_str()) {
        return Err("unknown planner".into());
    }
    s.planner = planner;
    s.monthly_cap = monthly_cap.clamp(0.0, 1000.0);
    if let Some(k) = claude_key {
        s.claude_key = k.trim().to_string();
    }
    save_ai(&state, &s)
}

/// Looks at every file for the automatic edit. Events: "analyze-progress",
/// then "analyze-finished" with the shot list.
#[tauri::command]
pub fn analyze_shots(app: AppHandle, state: tauri::State<State>, paths: Vec<PathBuf>) -> Result<(), String> {
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    let whisper = crate::captions::best_model(&state.data_dir);
    std::thread::spawn(move || {
        let total = paths.len() as u64;
        let mut shots = vec![];
        let mut failure = None;
        for (i, p) in paths.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                failure = Some("cancelled".to_string());
                break;
            }
            let _ = app.emit("analyze-progress", Progress { done: i as u64, total });
            match engine::analyze::analyze(p) {
                Ok(mut s) => {
                    // What is said, when the speech model is already on this computer.
                    if s.has_audio && s.duration > 0.5 && s.duration < 600.0 {
                        if let Some(model) = &whisper {
                            s.speech = crate::captions::transcribe_file(model, p, &cancel).unwrap_or_default();
                        }
                    }
                    shots.push(s);
                }
                Err(e) => {
                    failure = Some(format!("{}: {e}", p.display()));
                    break;
                }
            }
        }
        let _ = app.emit("analyze-progress", Progress { done: total, total });
        let ok = failure.is_none();
        let _ = app.emit("analyze-finished", Finished { ok, message: failure.unwrap_or_default(), result: ok.then_some(shots) });
    });
    Ok(())
}

/// What a Claude plan would cost, roughly, before asking.
#[derive(Serialize)]
pub struct Estimate {
    dollars: f64,
    spent: f64,
    cap: f64,
}

/// Claude Haiku 5.5 rates, US dollars per million tokens (input, output).
/// Chosen because the user asked to keep the cost low.
const CLAUDE_MODEL: &str = "claude-haiku-5-5";
const RATE_IN: f64 = 0.10;
const RATE_OUT: f64 = 0.50;
const PLAN_MAX_TOKENS: u32 = 4000;

#[tauri::command]
pub fn estimate_plan(state: tauri::State<State>, brief: Brief, shots: Vec<ShotInfo>) -> Estimate {
    let s = load_ai(&state);
    let text = autoedit::describe(&brief, &shots);
    // About 2.5 characters per token for mixed Arabic and English, plus the
    // instructions and schema; thinking and the answer bounded by max_tokens.
    let input = (text.chars().count() + autoedit::PLAN_INSTRUCTIONS.len() + 1500) as f64 / 2.5;
    Estimate { dollars: input * RATE_IN / 1e6 + PLAN_MAX_TOKENS as f64 * RATE_OUT / 1e6, spent: s.spent, cap: s.monthly_cap }
}

#[derive(Serialize, Clone)]
pub struct PlanResult {
    project: Project,
    /// "rules", "local" or "claude": what actually made the plan.
    planner: String,
    /// Set when the chosen planner failed and rules were used instead.
    note: String,
    cost: f64,
}

/// Makes the plan and the project. Events: "plan-finished".
#[tauri::command]
pub fn make_edit(app: AppHandle, state: tauri::State<State>, brief: Brief, shots: Vec<ShotInfo>, name: String) -> Result<(), String> {
    let ai = load_ai(&state);
    let data = state.data_dir.clone();
    std::thread::spawn(move || {
        let beats_for = |plan: &Plan| plan.music.and_then(|m| engine::beats::detect(&shots[m].path, 0.0, shots[m].duration.min(600.0)).ok());
        // Rules first: they also find the music whose beats guide the cuts.
        let first = autoedit::rule_plan(&brief, &shots, None);
        let beats = beats_for(&first);
        let rules = autoedit::rule_plan(&brief, &shots, beats.as_ref());
        let (mut plan, mut used, mut note, mut cost) = (rules.clone(), "rules".to_string(), String::new(), 0.0);
        let asked = match ai.planner.as_str() {
            "claude" => Some(claude_plan(&ai, &brief, &shots).map(|(p, c)| (p, c, "claude"))),
            "local" => Some(local::plan(&data, &brief, &shots).map(|p| (p, 0.0, "local"))),
            _ => None,
        };
        match asked {
            Some(Ok((p, c, who))) => {
                plan = p;
                used = who.into();
                cost = c;
            }
            Some(Err(e)) => note = e,
            None => {}
        }
        if cost > 0.0 {
            let mut s: AiSettings = std::fs::read_to_string(data.join("ai.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            if s.month != this_month() {
                s.month = this_month();
                s.spent = 0.0;
            }
            s.spent += cost;
            let _ = std::fs::write(data.join("ai.json"), serde_json::to_vec_pretty(&s).unwrap_or_default());
        }
        // A model plan keeps the beat: snap its cuts like the rules do.
        if used != "rules" {
            if let (Some(b), Some(_)) = (beats_for(&plan).or(beats.clone()), plan.music) {
                snap_to_beats(&mut plan, &b);
            }
        }
        let project = autoedit::build(&plan, &brief, &shots, &name);
        let _ = app.emit("plan-finished", Finished { ok: true, message: String::new(), result: Some(PlanResult { project, planner: used, note, cost }) });
    });
    Ok(())
}

fn snap_to_beats(plan: &mut Plan, beats: &engine::beats::Beats) {
    let first = beats.times.first().copied().unwrap_or(0.0);
    let times: Vec<f64> = beats.times.iter().map(|t| t - first).collect();
    let mut at = 0.0;
    let mut snapped_end = 0.0;
    for p in plan.shots.iter_mut() {
        let end = at + p.length;
        let t = times.iter().copied().min_by(|a, b| (a - end).abs().total_cmp(&(b - end).abs())).unwrap_or(end);
        let new_end = if (t - end).abs() < p.length * 0.45 && t > snapped_end + 0.5 { t } else { end };
        p.length = (new_end - snapped_end).max(0.5);
        snapped_end += p.length;
        at = end;
    }
}

/// One small request to Claude with text only. Refused when it would go
/// over the user's monthly limit.
fn claude_plan(ai: &AiSettings, brief: &Brief, shots: &[ShotInfo]) -> Result<(Plan, f64), String> {
    if ai.claude_key.is_empty() {
        return Err("no Claude key".into());
    }
    let text = autoedit::describe(brief, shots);
    let guess = (text.chars().count() + 3000) as f64 / 2.5 * RATE_IN / 1e6 + PLAN_MAX_TOKENS as f64 * RATE_OUT / 1e6;
    if ai.spent + guess > ai.monthly_cap {
        return Err("monthly limit reached".into());
    }
    let body = serde_json::json!({
        "model": CLAUDE_MODEL,
        "max_tokens": PLAN_MAX_TOKENS,
        "system": autoedit::PLAN_INSTRUCTIONS,
        "messages": [{"role": "user", "content": text}],
        "output_config": {"effort": "low", "format": {"type": "json_schema", "schema": autoedit::plan_schema()}}
    });
    let mut resp = ureq::post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &ai.claude_key)
        .header("anthropic-version", "2026-10-21")
        .header("content-type", "application/json")
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(&body)
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let v: serde_json::Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
    if status != 200 {
        return Err(v["error"]["message"].as_str().unwrap_or("Claude request failed").to_string());
    }
    let input = v["usage"]["input_tokens"].as_f64().unwrap_or(0.0);
    let output = v["usage"]["output_tokens"].as_f64().unwrap_or(0.0);
    let cost = input * RATE_IN / 1e6 + output * RATE_OUT / 1e6;
    if v["stop_reason"] == "refusal" {
        return Err("Claude declined this request".into());
    }
    let answer: String = v["content"]
        .as_array()
        .map(|blocks| blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect())
        .unwrap_or_default();
    autoedit::parse_plan(&answer, shots).map(|p| (p, cost)).ok_or_else(|| "Claude's plan could not be used".into())
}

/// Downloads the free local model and its runner. Events: "llm-progress",
/// then "llm-finished".
#[tauri::command]
pub fn download_local_ai(app: AppHandle, state: tauri::State<State>) -> Result<(), String> {
    let data = state.data_dir.clone();
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let res = local::install(&app, &data, &cancel);
        let _ = app.emit("llm-finished", Finished::<()> { ok: res.is_ok(), message: res.err().unwrap_or_default(), result: None });
    });
    Ok(())
}

/// The free model that runs on this computer: llama.cpp's server with
/// Qwen2.5 1.5B Instruct (Apache-2.0). Both are downloaded once when the
/// user asks; the server only listens on this computer and is stopped
/// after each plan.
mod local {
    use super::*;
    use std::io::Read;

    pub const MODEL_MB: u32 = 1120;
    const MODEL_URL: &str = "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf";
    const MODEL_FILE: &str = "qwen2.5-1.5b-instruct-q4_k_m.gguf";

    fn dir(data: &Path) -> PathBuf {
        data.join("llm")
    }

    fn server_name() -> &'static str {
        if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        }
    }

    fn find_server(root: &Path) -> Option<PathBuf> {
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).ok()?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.file_name().is_some_and(|n| n == server_name()) {
                    return Some(p);
                }
            }
        }
        None
    }

    pub fn ready(data: &Path) -> bool {
        dir(data).join(MODEL_FILE).exists() && find_server(&dir(data).join("bin")).is_some()
    }

    /// The llama.cpp release file for this computer.
    fn asset_wanted(name: &str) -> bool {
        let n = name.to_lowercase();
        if !n.ends_with(".zip") || !n.contains("-bin-") {
            return false;
        }
        if cfg!(target_os = "windows") {
            n.contains("win-cpu-x64")
        } else if cfg!(target_os = "macos") {
            n.contains(if cfg!(target_arch = "aarch64") { "macos-arm64" } else { "macos-x64" })
        } else {
            n.contains("ubuntu-x64") && !n.contains("vulkan")
        }
    }

    pub fn install(app: &AppHandle, data: &Path, cancel: &std::sync::atomic::AtomicBool) -> Result<(), String> {
        let d = dir(data);
        std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        if find_server(&d.join("bin")).is_none() {
            let mut resp = ureq::get("https://api.github.com/repos/ggml-org/llama.cpp/releases/latest")
                .header("User-Agent", "arabic-video-editor")
                .call()
                .map_err(|e| e.to_string())?;
            let v: serde_json::Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
            let asset = v["assets"]
                .as_array()
                .and_then(|a| a.iter().find(|x| x["name"].as_str().is_some_and(asset_wanted)))
                .ok_or("no llama.cpp build for this computer")?;
            let url = asset["browser_download_url"].as_str().ok_or("bad release")?;
            if !url.starts_with("https://github.com/ggml-org/llama.cpp/releases/download/") {
                return Err("unexpected download address".into());
            }
            let zip_path = d.join("llama.zip");
            download_to(app, url, &zip_path, cancel, 40, "llm-progress")?;
            let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            let bin = d.join("bin");
            for i in 0..zip.len() {
                let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
                // Only plain relative paths, inside our folder.
                let Some(rel) = entry.enclosed_name() else { continue };
                let out = bin.join(rel);
                if entry.is_dir() {
                    let _ = std::fs::create_dir_all(&out);
                    continue;
                }
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut buf = vec![];
                entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                std::fs::write(&out, buf).map_err(|e| e.to_string())?;
                #[cfg(unix)]
                if let Some(mode) = entry.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
                }
            }
            let _ = std::fs::remove_file(&zip_path);
            #[cfg(unix)]
            if let Some(s) = find_server(&bin) {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(s, std::fs::Permissions::from_mode(0o755));
            }
        }
        let model = d.join(MODEL_FILE);
        if !model.exists() {
            download_to(app, MODEL_URL, &model, cancel, MODEL_MB, "llm-progress")?;
        }
        Ok(())
    }

    struct Server(std::process::Child);
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    pub fn plan(data: &Path, brief: &Brief, shots: &[ShotInfo]) -> Result<Plan, String> {
        if !ready(data) {
            return Err("local model not downloaded".into());
        }
        let server = find_server(&dir(data).join("bin")).ok_or("llama-server missing")?;
        let port = std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).map_err(|e| e.to_string())?;
        let mut cmd = std::process::Command::new(&server);
        cmd.arg("-m")
            .arg(dir(data).join(MODEL_FILE))
            .args(["--host", "127.0.0.1", "--port", &port.to_string(), "-c", "8192", "-ngl", "99"])
            .current_dir(server.parent().unwrap_or(Path::new(".")))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let _guard = Server(cmd.spawn().map_err(|e| e.to_string())?);
        let base = format!("http://127.0.0.1:{port}");
        let started = std::time::Instant::now();
        loop {
            if ureq::get(&format!("{base}/health")).call().map(|r| r.status() == 200).unwrap_or(false) {
                break;
            }
            if started.elapsed().as_secs() > 120 {
                return Err("local model did not start".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        let body = serde_json::json!({
            "messages": [
                {"role": "system", "content": autoedit::PLAN_INSTRUCTIONS},
                {"role": "user", "content": autoedit::describe(brief, shots)}
            ],
            "temperature": 0.2,
            "max_tokens": 2500,
            // The server keeps the answer to this shape.
            "response_format": {"type": "json_object", "schema": autoedit::plan_schema()}
        });
        let mut resp = ureq::post(&format!("{base}/v1/chat/completions")).send_json(&body).map_err(|e| e.to_string())?;
        let v: serde_json::Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
        let answer = v["choices"][0]["message"]["content"].as_str().unwrap_or_default();
        autoedit::parse_plan(answer, shots).ok_or_else(|| "the local model's plan could not be used".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions_compare() {
        assert!(super::newer("v0.2.0", "0.1.9"));
        assert!(!super::newer("v0.1.0", "0.1.0"));
        assert!(super::newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn month_is_formatted() {
        let m = super::this_month();
        assert_eq!(m.len(), 7);
        assert!(m.starts_with("20"));
    }

    #[test]
    fn beats_keep_cuts_apart() {
        let mut plan = engine::autoedit::Plan {
            shots: vec![
                engine::autoedit::PlanShot { shot: 0, start: 0.0, length: 2.1 },
                engine::autoedit::PlanShot { shot: 0, start: 3.0, length: 1.9 },
            ],
            ..Default::default()
        };
        let beats = engine::beats::Beats { tempo: 120.0, times: (0..20).map(|k| k as f64 * 0.5).collect() };
        super::snap_to_beats(&mut plan, &beats);
        assert!((plan.shots[0].length - 2.0).abs() < 1e-9);
        assert!((plan.shots[0].length + plan.shots[1].length - 4.0).abs() < 1e-9);
    }
}
