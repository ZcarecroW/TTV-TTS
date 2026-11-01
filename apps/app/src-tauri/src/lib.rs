//! TTV-TTS: reads Twitch chat aloud with Voice Creator Pro voices, on this PC
//! or through TTV-TTS Proxy on a second PC.

mod audio;
mod classic;
mod commands;
mod config;
mod error;
mod host;
mod library;
mod logbuf;
mod monitor;
mod friends_cmds;
mod pack_cmds;
mod preset_cmds;
mod quality_cmds;
mod presets;
mod profile;
mod reader;
mod secrets;
mod speech;
mod state;
mod storage;
mod text;
mod twitch;
mod updater;

use tauri::Manager;

use crate::config::Config;
use crate::state::AppState;

pub fn run() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let mut builder = tauri::Builder::default();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }));
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let paths = app.path();
            let config_dir = paths.app_config_dir()?;
            let data_dir = paths.app_data_dir()?;
            logbuf::init(paths.app_log_dir().ok());
            log::info!("TTV-TTS {} starting", env!("CARGO_PKG_VERSION"));

            let cfg_path = config_dir.join("config.json");
            let mut cfg: Config = storage::read_json(&cfg_path).unwrap_or_default();
            cfg.normalize();
            storage::write_json(&cfg_path, &cfg)?;

            let state = AppState::new(cfg, cfg_path, data_dir);
            state.set_app(app.handle().clone());
            app.manage(state.clone());
            // Local engine (VCP on this PC): idle unless direct mode has jobs.
            state.library.host_changed(&state);
            tauri::async_runtime::spawn(state.library.local.worker());
            tauri::async_runtime::spawn(library::local_events(state.clone()));
            tauri::async_runtime::spawn(monitor::run(state.clone()));
            let st = state.clone();
            tauri::async_runtime::spawn(async move { reader::start(st) });
            tauri::async_runtime::spawn(updater::run(app.handle().clone(), state.clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::update_settings,
            commands::discover_proxies,
            commands::probe_proxy,
            commands::pair_proxy,
            commands::use_direct,
            commands::forget_host,
            commands::watch_host_system,
            commands::proxy_vcp_action,
            commands::wake_host,
            commands::proxy_update,
            commands::set_audio,
            commands::list_voices,
            commands::speak_test,
            commands::skip,
            commands::get_reader,
            commands::save_profile,
            commands::delete_profile,
            commands::set_active_profiles,
            commands::reader_skip,
            commands::reader_clear,
            commands::reader_pause,
            commands::test_message,
            commands::mute_viewer,
            commands::list_viewers,
            commands::set_viewer_role,
            commands::forget_viewer,
            commands::update_info,
            commands::check_update,
            commands::install_update,
            preset_cmds::list_presets,
            preset_cmds::get_preset,
            preset_cmds::save_preset,
            preset_cmds::duplicate_preset,
            preset_cmds::delete_preset,
            preset_cmds::parse_preset,
            preset_cmds::export_preset,
            preset_cmds::preset_prompt,
            preset_cmds::engine_status,
            preset_cmds::estimate,
            preset_cmds::benchmark,
            preset_cmds::compute,
            preset_cmds::job_control,
            preset_cmds::delete_voices,
            preset_cmds::verify_library,
            preset_cmds::listen,
            preset_cmds::redo_voice,
            friends_cmds::internet_status,
            friends_cmds::internet_set,
            friends_cmds::internet_invite,
            friends_cmds::internet_withdraw,
            friends_cmds::internet_revoke,
            friends_cmds::internet_test,
            friends_cmds::read_invite,
            friends_cmds::join_invite,
            pack_cmds::pick_pack,
            pack_cmds::pack_info,
            pack_cmds::import_pack,
            pack_cmds::export_pack,
            pack_cmds::pack_catalog,
            pack_cmds::download_packs,
            quality_cmds::quality_list,
            quality_cmds::keep_voices,
            quality_cmds::improve_voices,
            quality_cmds::check_quality,
            quality_cmds::play_reference,
            preset_cmds::detect_classic,
            preset_cmds::adopt_classic,
        ])
        .build(tauri::generate_context!())
        .expect("error while building TTV-TTS")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                reader::flush(&app.state::<std::sync::Arc<AppState>>());
            }
        });
}
