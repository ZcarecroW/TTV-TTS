//! TTV-TTS Proxy: runs on the PC with Voice Creator Pro (VCP) and makes VCP
//! available to TTV-TTS on other PCs in the network - encrypted, with its own
//! access control, and with extras VCP doesn't have (device pairing, LAN
//! discovery, starting/watching VCP, system monitor, firewall setup).

mod access;
mod api;
mod commands;
mod config;
mod discovery;
mod error;
mod firewall;
mod forward;
#[cfg(windows)]
mod gpu_procs;
mod logbuf;
mod secrets;
mod server;
mod shell;
mod state;
mod storage;
mod sysmon;
mod tls;
mod updater;
mod vcp_key;
mod vcp_watch;

use std::sync::Arc;

use tauri::{Manager, RunEvent, WindowEvent};

use crate::config::Config;
use crate::state::AppState;

/// Forwards the engine's job and library events to apps and the window.
async fn engine_events(state: Arc<AppState>) {
    use tauri::Emitter;
    let mut rx = state.engine.subscribe();
    loop {
        let ev = match rx.recv().await {
            Ok(ev) => ev,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => return,
        };
        let out = match ev {
            ttv_engine::EngineEvent::Job(j) => ttv_link::Event::Job(j),
            ttv_engine::EngineEvent::Library(c) => ttv_link::Event::Library(c),
        };
        if let Some(app) = state.app() {
            let _ = app.emit("proxy://engine", &out);
        }
        let _ = state.events.send(out);
    }
}

/// Enables or disables starting with the OS (no-op where unsupported).
pub fn set_autostart(app: &tauri::AppHandle, on: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let al = app.autolaunch();
    let r = if on { al.enable() } else { al.disable() };
    if let Err(e) = r {
        log::warn!("changing autostart failed: {e}");
    }
}

pub fn run() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let mut builder = tauri::Builder::default();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // A second start brings the window of the running proxy to the front.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| shell::open_window(app)));
        builder = builder
            .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
            .plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(commands::WindowWatch::default())
        .setup(|app| {
            let paths = app.path();
            let config_dir = paths.app_config_dir()?;
            let data_dir = paths.app_data_dir()?;
            logbuf::init(paths.app_log_dir().ok());
            log::info!("TTV-TTS Proxy {} starting", env!("CARGO_PKG_VERSION"));

            let cfg_path = config_dir.join("config.json");
            let first_run = !cfg_path.exists();
            let mut cfg: Config = storage::read_json(&cfg_path).unwrap_or_default();
            cfg.normalize();
            // Development: TTV_PROXY_VCP_URL points the proxy at another VCP.
            if let Ok(url) = std::env::var("TTV_PROXY_VCP_URL") {
                cfg.vcp_url = url.trim_end_matches('/').to_string();
            }
            if let Ok(port) = std::env::var("TTV_PROXY_PORT").map(|p| p.parse::<u16>()) {
                cfg.port = port.unwrap_or(cfg.port);
            }
            storage::write_json(&cfg_path, &cfg)?;

            let tls = tls::load_or_create(&data_dir, &cfg.display_name())?;
            let autostart = cfg.autostart;
            let state = AppState::new(cfg, cfg_path, data_dir, tls);
            state.set_app(app.handle().clone());
            app.manage(state.clone());

            // Debug builds (development PC) never register themselves for autostart.
            if first_run && autostart && !cfg!(debug_assertions) {
                set_autostart(app.handle(), true);
            }

            tauri::async_runtime::spawn(server::run(state.clone()));
            tauri::async_runtime::spawn(discovery::run(state.clone()));
            tauri::async_runtime::spawn(vcp_watch::run(state.clone()));
            tauri::async_runtime::spawn(updater::run(app.handle().clone(), state.clone()));
            sysmon::spawn(state.clone());
            tauri::async_runtime::spawn(state.engine.worker());
            tauri::async_runtime::spawn(engine_events(state.clone()));

            shell::setup_tray(app.handle(), &state)?;
            let minimized = std::env::args().any(|a| a == "--minimized");
            if !minimized {
                shell::open_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the proxy running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                let state = window.state::<commands::WindowWatch>();
                *state.0.lock() = None;
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_overview,
            commands::update_settings,
            commands::set_vcp_key,
            commands::clear_vcp_key,
            commands::create_vcp_key,
            commands::detect_vcp_key,
            commands::get_access_key,
            commands::new_access_key,
            commands::set_access_key,
            commands::open_pairing,
            commands::get_pairing,
            commands::close_pairing,
            commands::revoke_device,
            commands::vcp_action,
            commands::firewall_check,
            commands::firewall_fix,
            commands::get_logs,
            commands::watch_system,
            commands::check_update,
            commands::install_update,
            commands::reset_certificate,
            commands::diagnostics,
        ])
        .build(tauri::generate_context!())
        .expect("error while building TTV-TTS Proxy")
        .run(|_app, event| {
            // Keep running in the tray when no window is open.
            if let RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}

// Silence "unused" for state helpers only used on some platforms.
#[allow(dead_code)]
fn _assert_send_sync() {
    fn f<T: Send + Sync>() {}
    f::<Arc<AppState>>();
}
