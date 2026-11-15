// Hide the extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::path::PathBuf;
use tauri::Manager;

fn main() {
    use_bundled_ffmpeg();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let fonts = app
                .path()
                .resource_dir()
                .map(|d| d.join("fonts"))
                .ok()
                .filter(|d| d.exists())
                .or_else(|| Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts")));
            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data)?;
            app.manage(commands::State::new(fonts, data));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::probe_media,
            commands::list_fonts,
            commands::update_project,
            commands::recover_autosave,
            commands::save_project,
            commands::open_project,
            commands::render_frame,
            commands::start_export,
            commands::cancel_export,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the app");
}

/// The installer puts ffmpeg and ffprobe next to the app; use those copies.
fn use_bundled_ffmpeg() {
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(dir) = exe.parent() else { return };
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    for (name, var) in [("ffmpeg", "AVE_FFMPEG"), ("ffprobe", "AVE_FFPROBE")] {
        let p = dir.join(format!("{name}{suffix}"));
        if p.exists() && std::env::var_os(var).is_none() {
            std::env::set_var(var, p);
        }
    }
}
