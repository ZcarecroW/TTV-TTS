//! TTV-TTS: reads Twitch chat aloud with Voice Creator Pro voices, on this PC
//! or through TTV-TTS Proxy on a second PC.

mod audio;
mod commands;
mod config;
mod error;
mod host;
mod logbuf;
mod monitor;
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
            commands::update_info,
            commands::check_update,
            commands::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building TTV-TTS")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                reader::flush(&app.state::<std::sync::Arc<AppState>>());
            }
        });
}
