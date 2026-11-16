//! Automatic captions, made on this computer with Whisper. The sound never
//! leaves the device; only the speech model is downloaded once, after the
//! user agrees.

use crate::commands::State;
use serde::Serialize;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

struct Model {
    id: &'static str,
    file: &'static str,
    /// Approximate download size, for the message before downloading.
    mb: u32,
}

/// "fast" is quick on any computer; "accurate" is much better for Arabic
/// (dialects included) and still runs well on a recent laptop.
const MODELS: &[Model] = &[
    Model { id: "fast", file: "ggml-base.bin", mb: 148 },
    Model { id: "accurate", file: "ggml-large-v3-turbo-q5_0.bin", mb: 574 },
];
const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

#[derive(Serialize)]
pub struct ModelInfo {
    id: String,
    mb: u32,
    downloaded: bool,
}

#[derive(Serialize, Clone)]
pub struct Segment {
    start: f64,
    end: f64,
    text: String,
}

#[derive(Serialize, Clone)]
struct Progress {
    done: u64,
    total: u64,
}

#[derive(Serialize, Clone)]
struct Finished<T: Serialize + Clone> {
    ok: bool,
    message: String,
    result: Option<T>,
}

fn model_path(state: &State, m: &Model) -> PathBuf {
    state.data_dir.join("models").join(m.file)
}

fn find(id: &str) -> Result<&'static Model, String> {
    MODELS.iter().find(|m| m.id == id).ok_or_else(|| format!("unknown model {id}"))
}

#[tauri::command]
pub fn caption_models(state: tauri::State<State>) -> Vec<ModelInfo> {
    MODELS
        .iter()
        .map(|m| ModelInfo { id: m.id.into(), mb: m.mb, downloaded: model_path(&state, m).exists() })
        .collect()
}

/// Downloads a speech model in the background ("model-progress", then
/// "model-finished"). Written to a temporary name first, so a broken
/// download is never mistaken for a model.
#[tauri::command]
pub fn download_model(app: AppHandle, state: tauri::State<State>, id: String) -> Result<(), String> {
    let m = find(&id)?;
    let dest = model_path(&state, m);
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let res = (|| -> Result<(), String> {
            std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
            let part = dest.with_extension("part");
            let resp = ureq::get(&format!("{BASE_URL}{}", m.file)).call().map_err(|e| e.to_string())?;
            let total: u64 = resp
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(m.mb as u64 * 1_000_000);
            let mut body = resp.into_body().into_reader();
            let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 1 << 16];
            let (mut done, mut last) = (0u64, std::time::Instant::now());
            loop {
                if cancel.load(Ordering::Relaxed) {
                    drop(out);
                    let _ = std::fs::remove_file(&part);
                    return Err("cancelled".into());
                }
                let n = body.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                done += n as u64;
                if last.elapsed().as_millis() > 200 {
                    last = std::time::Instant::now();
                    let _ = app.emit("model-progress", Progress { done, total });
                }
            }
            out.sync_all().map_err(|e| e.to_string())?;
            if done < total {
                let _ = std::fs::remove_file(&part);
                return Err(format!("download stopped early ({done} of {total} bytes)"));
            }
            std::fs::rename(&part, &dest).map_err(|e| e.to_string())
        })();
        let _ = app.emit(
            "model-finished",
            Finished::<()> { ok: res.is_ok(), message: res.err().unwrap_or_default(), result: None },
        );
    });
    Ok(())
}

/// Listens to the whole timeline and writes down what is said, with times.
/// Runs in the background ("caption-progress" in percent, then
/// "caption-finished" with the lines).
#[tauri::command]
pub fn transcribe(app: AppHandle, state: tauri::State<State>, model: String, language: String) -> Result<(), String> {
    let m = find(&model)?;
    let path = model_path(&state, m);
    if !path.exists() {
        return Err("model not downloaded".into());
    }
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let res = (|| -> Result<Vec<Segment>, String> {
            let samples = engine::export::mix_audio(&project, 16_000).map_err(|e| e.to_string())?;
            if samples.is_empty() {
                return Ok(vec![]);
            }
            let ctx = WhisperContext::new_with_params(&path, WhisperContextParameters::default())
                .map_err(|e| e.to_string())?;
            let mut st = ctx.create_state().map_err(|e| e.to_string())?;
            let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
            let lang = if language == "auto" { None } else { Some(language.as_str()) };
            params.set_language(lang);
            params.set_n_threads(std::thread::available_parallelism().map(|n| n.get().min(8) as i32).unwrap_or(4));
            // Short lines that break between words: ready to use as captions.
            params.set_token_timestamps(true);
            params.set_split_on_word(true);
            params.set_max_len(42);
            params.set_suppress_nst(true);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            let progress_app = app.clone();
            params.set_progress_callback_safe(move |p: i32| {
                let _ = progress_app.emit("caption-progress", Progress { done: p.max(0) as u64, total: 100 });
            });
            let stop = Arc::clone(&cancel);
            params.set_abort_callback_safe(move || stop.load(Ordering::Relaxed));
            st.full(params, &samples).map_err(|e| e.to_string())?;
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let mut out = vec![];
            for seg in st.as_iter() {
                let text = seg.to_str_lossy().map_err(|e| e.to_string())?.trim().to_string();
                if text.is_empty() || text.starts_with('[') || text.starts_with('(') {
                    continue;
                }
                out.push(Segment { start: seg.start_timestamp() as f64 / 100.0, end: seg.end_timestamp() as f64 / 100.0, text });
            }
            Ok(out)
        })();
        let msg = res.as_ref().err().cloned().unwrap_or_default();
        let _ = app.emit("caption-finished", Finished { ok: res.is_ok(), message: msg, result: res.ok() });
    });
    Ok(())
}
