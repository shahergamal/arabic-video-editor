//! Phase 4: several cameras, moving timelines to other editors, plugins.

use crate::commands::State;
use engine::interchange::{self, Exported};
use engine::{Fps, Project};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Component, Path, PathBuf};

type Res<T> = Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------- several cameras ----------

#[derive(Serialize)]
pub struct Synced {
    source: PathBuf,
    offset: f64,
    confidence: f32,
}

/// Lines every file up against the first one by sound.
#[tauri::command]
pub async fn sync_angles(paths: Vec<PathBuf>) -> Res<Vec<Synced>> {
    tauri::async_runtime::spawn_blocking(move || {
        let first = paths.first().ok_or("no files")?.clone();
        let mut out = vec![Synced { source: first.clone(), offset: 0.0, confidence: 1.0 }];
        for p in &paths[1..] {
            let (offset, confidence) = engine::multicam::audio_offset(&first, p).map_err(err)?;
            out.push(Synced { source: p.clone(), offset, confidence });
        }
        Ok(out)
    })
    .await
    .map_err(err)?
}

// ---------- other editors ----------

/// Writes the current timeline as OTIO, Final Cut Pro 7 XML (Premiere) or EDL.
#[tauri::command]
pub fn export_timeline(state: tauri::State<State>, path: PathBuf, format: String) -> Res<Vec<String>> {
    let project = state.project.lock().unwrap().clone().ok_or("no project")?;
    let Exported { text, warnings } = match format.as_str() {
        "otio" => interchange::to_otio(&project),
        "xml" => interchange::to_fcp7_xml(&project),
        "edl" => interchange::to_edl(&project),
        _ => return Err("unknown format".into()),
    };
    std::fs::write(&path, text).map_err(err)?;
    Ok(warnings)
}

/// Reads an OTIO or Final Cut Pro 7 XML timeline as a new project.
#[tauri::command]
pub fn import_timeline(path: PathBuf) -> Res<Project> {
    interchange::import_timeline(&path, (1080, 1920, Fps::new(30, 1))).map_err(err)
}

// ---------- plugins ----------
//
// A plugin is a folder with a `plugin.json` and the files it names. Plugins
// only carry data: colour looks (settings and .cube LUTs), text styles and
// fonts. They never run code, so installing one cannot harm the computer.

const MANIFEST: &str = "plugin.json";
const MAX_FILE: u64 = 64 * 1024 * 1024;
const ALLOWED: &[&str] = &["cube", "ttf", "otf", "png", "jpg", "jpeg", "md", "txt"];

#[derive(Deserialize, Serialize, Clone)]
pub struct Look {
    name: String,
    #[serde(default)]
    name_ar: Option<String>,
    #[serde(default)]
    adjust: serde_json::Map<String, Value>,
    #[serde(default)]
    lut: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct TextStyle {
    name: String,
    #[serde(default)]
    name_ar: Option<String>,
    style: serde_json::Map<String, Value>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Manifest {
    id: String,
    name: String,
    #[serde(default)]
    name_ar: Option<String>,
    #[serde(default)]
    version: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    looks: Vec<Look>,
    #[serde(default)]
    texts: Vec<TextStyle>,
    #[serde(default)]
    fonts: Vec<String>,
}

fn plugins_dir(state: &State) -> PathBuf {
    state.data_dir.join("plugins")
}

/// A file named inside a plugin: plain relative path, allowed kind, present.
fn inside(root: &Path, rel: &str) -> Res<PathBuf> {
    let p = Path::new(rel);
    if p.is_absolute() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(format!("\"{rel}\" must be a file inside the plugin folder"));
    }
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !ALLOWED.contains(&ext.as_str()) {
        return Err(format!("\"{rel}\": file type not allowed"));
    }
    let full = root.join(p);
    let meta = std::fs::symlink_metadata(&full).map_err(|_| format!("\"{rel}\" is missing"))?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Err(format!("\"{rel}\" is not a normal file or is too big"));
    }
    Ok(full)
}

/// Reads and checks a plugin folder.
pub fn check_plugin(root: &Path) -> Res<Manifest> {
    let text = std::fs::read_to_string(root.join(MANIFEST)).map_err(|_| "this folder has no plugin.json".to_string())?;
    let m: Manifest = serde_json::from_str(&text).map_err(|e| format!("plugin.json: {e}"))?;
    if m.id.is_empty() || m.id.len() > 64 || !m.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err("plugin id must be lowercase letters, digits and dashes".into());
    }
    for l in &m.looks {
        if let Some(lut) = &l.lut {
            let f = inside(root, lut)?;
            engine::lut::Lut::load(&f).map_err(|e| format!("{lut}: {e}"))?;
        }
        for (k, v) in &l.adjust {
            if !v.is_number() {
                return Err(format!("look \"{}\": {k} must be a number", l.name));
            }
        }
    }
    for f in &m.fonts {
        inside(root, f)?;
    }
    if m.looks.is_empty() && m.texts.is_empty() && m.fonts.is_empty() {
        return Err("the plugin has nothing in it".into());
    }
    Ok(m)
}

#[derive(Serialize)]
pub struct Installed {
    #[serde(flatten)]
    manifest: Manifest,
    /// Full paths of each look's LUT, in order (None when it has no LUT).
    lut_paths: Vec<Option<PathBuf>>,
}

#[tauri::command]
pub fn list_plugins(state: tauri::State<State>) -> Vec<Installed> {
    let dir = plugins_dir(&state);
    let mut out = vec![];
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let root = e.path();
        if let Ok(m) = check_plugin(&root) {
            let lut_paths = m.looks.iter().map(|l| l.lut.as_ref().map(|f| root.join(f))).collect();
            out.push(Installed { manifest: m, lut_paths });
        }
    }
    out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    out
}

/// Copies a checked plugin folder in, replacing an older version.
#[tauri::command]
pub fn install_plugin(state: tauri::State<State>, folder: PathBuf) -> Res<String> {
    let m = check_plugin(&folder)?;
    let dest = plugins_dir(&state).join(&m.id);
    let tmp = plugins_dir(&state).join(format!(".{}-new", m.id));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(err)?;
    let mut files: Vec<String> = m.looks.iter().filter_map(|l| l.lut.clone()).chain(m.fonts.iter().cloned()).collect();
    files.push(MANIFEST.into());
    for rel in files {
        let to = tmp.join(&rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(err)?;
        }
        std::fs::copy(folder.join(&rel), &to).map_err(err)?;
    }
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&tmp, &dest).map_err(err)?;
    // Fonts join the user's fonts so they work like any added font.
    for f in &m.fonts {
        let src = dest.join(f);
        if let Some(name) = src.file_name() {
            let to = state.user_fonts.join(name);
            if std::fs::copy(&src, &to).is_ok() {
                let _ = state.preview.lock().unwrap().text.add_font_file(&to);
            }
        }
    }
    Ok(m.id)
}

#[tauri::command]
pub fn remove_plugin(state: tauri::State<State>, id: String) -> Res<()> {
    if !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') || id.is_empty() {
        return Err("bad id".into());
    }
    std::fs::remove_dir_all(plugins_dir(&state).join(id)).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    const CUBE: &str = "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";

    #[test]
    fn a_good_plugin_passes() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "luts/warm.cube", CUBE);
        write(d.path(), MANIFEST, r#"{"id":"warm-pack","name":"Warm","looks":[{"name":"Warm","adjust":{"temperature":0.3},"lut":"luts/warm.cube"}]}"#);
        let m = check_plugin(d.path()).unwrap();
        assert_eq!(m.looks.len(), 1);
    }

    #[test]
    fn files_outside_the_folder_or_of_other_kinds_are_refused() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "x.cube", CUBE);
        for lut in ["../x.cube", "/etc/passwd", "run.exe", "a/../../x.cube"] {
            write(d.path(), MANIFEST, &format!(r#"{{"id":"p","name":"P","looks":[{{"name":"L","lut":"{lut}"}}]}}"#));
            assert!(check_plugin(d.path()).is_err(), "{lut}");
        }
        write(d.path(), MANIFEST, r#"{"id":"Bad Id","name":"P","looks":[{"name":"L"}]}"#);
        assert!(check_plugin(d.path()).is_err());
        write(d.path(), MANIFEST, r#"{"id":"p","name":"P","looks":[{"name":"L","adjust":{"brightness":"rm -rf"}}]}"#);
        assert!(check_plugin(d.path()).is_err());
    }

    #[test]
    fn the_example_plugin_in_the_docs_is_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/plugin-example");
        let m = check_plugin(&root).unwrap();
        assert_eq!(m.id, "example-looks");
        assert_eq!(m.looks.len(), 2);
    }
}
