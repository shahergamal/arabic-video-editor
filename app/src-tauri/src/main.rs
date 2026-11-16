// Hide the extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod captions;
mod commands;
mod phase4;
mod phase5;
mod publish;
mod tools;

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
            commands::thumbnail,
            commands::waveform,
            commands::list_fonts,
            commands::add_font,
            commands::update_project,
            commands::recover_autosave,
            commands::discard_autosave,
            commands::list_recent,
            commands::forget_recent,
            commands::save_project,
            commands::open_project,
            commands::missing_media,
            commands::render_frame,
            commands::start_export,
            commands::cancel_export,
            commands::save_template,
            commands::list_templates,
            commands::delete_template,
            commands::load_template,
            commands::get_brand,
            commands::set_brand,
            captions::caption_models,
            captions::download_model,
            captions::transcribe,
            tools::matte_models,
            tools::download_matte_model,
            tools::prepare_matte,
            tools::track_mask,
            tools::scopes,
            tools::silences,
            phase4::sync_angles,
            phase4::export_timeline,
            phase4::import_timeline,
            phase4::list_plugins,
            phase4::install_plugin,
            phase4::remove_plugin,
            phase5::prepare_copy,
            phase5::copy_ready,
            phase5::detect_beats,
            phase5::encoders,
            phase5::auto_reframe,
            phase5::save_recording,
            phase5::check_update,
            phase5::open_release_page,
            phase5::ai_settings,
            phase5::set_ai_settings,
            phase5::analyze_shots,
            phase5::estimate_plan,
            phase5::make_edit,
            phase5::download_local_ai,
            publish::social_status,
            publish::social_set_apps,
            publish::social_connect,
            publish::social_disconnect,
            publish::tiktok_privacy_options,
            publish::publish,
            publish::open_platform,
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
