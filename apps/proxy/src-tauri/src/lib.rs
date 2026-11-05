//! TTV-TTS Proxy: runs on the PC with Voice Creator Pro (VCP) and makes VCP
//! available to TTV-TTS on other PCs in the network - encrypted, with its own
//! access control, and with extras VCP doesn't have (device pairing, LAN
//! discovery, starting/watching VCP, system monitor, firewall setup).

mod access;
mod api;
mod awake;
mod commands;
mod config;
mod discovery;
mod error;
mod firewall;
mod forward;
#[cfg(windows)]
mod gpu_procs;
mod hosted;
mod internet;
mod logbuf;
mod plugins;
mod secrets;
mod server;
mod shell;
mod speak;
mod state;
mod storage;
mod sysmon;
mod tls;
mod updater;
mod vcp_key;
mod vcp_watch;
mod watchdog;

use std::sync::Arc;

use tauri::{Manager, RunEvent, WindowEvent};

use crate::config::Config;
use crate::state::AppState;

/// Set when building for the test loop on the voice host PC
/// (`TTV_DEV_BUILD=<commit>`): such a build says so and never updates itself.
pub const DEV_BUILD: Option<&str> = option_env!("TTV_DEV_BUILD");

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
            ttv_engine::EngineEvent::Slow(why) => {
                let st = state.clone();
                tokio::spawn(async move { restart_slow_vcp(&st, &why).await });
                continue;
            }
            ttv_engine::EngineEvent::Stuck(why) => {
                restart_stuck_vcp(&state, &why).await;
                continue;
            }
        };
        if let Some(app) = state.app() {
            let _ = app.emit("proxy://engine", &out);
        }
        let _ = state.events.send(out);
    }
}

/// VCP's speed dropped significantly while computing. Restarts it if that's
/// switched on: it waits for its turn, and what's computed so far stays.
async fn restart_slow_vcp(state: &Arc<AppState>, why: &str) {
    if !state.cfg.read().restart_when_slow {
        log::warn!("Voice Creator Pro has slowed down ({why}) – restarting it automatically is off");
        return;
    }
    if ttv_vcp::exclusive() || !state.vcp_state.read().reachable || !vcp_watch::auto_restart_due() {
        return;
    }
    let _turn = ttv_vcp::turn("/ttv/restart").await;
    log::warn!("Voice Creator Pro has slowed down ({why}) – restarting it");
    match vcp_watch::restart(state).await {
        Ok(m) => {
            ttv_vcp::perf::clear_recent();
            state.engine.speed_reset();
            log::info!("{m}");
        }
        Err(e) => log::warn!("restarting VCP failed: {e}"),
    }
}

/// VCP's API answers but its work hangs (requests time out): restart it,
/// at most every 30 minutes, so computing doesn't stall for hours.
async fn restart_stuck_vcp(state: &Arc<AppState>, why: &str) {
    if !state.vcp_state.read().reachable || !vcp_watch::auto_restart_due() {
        return;
    }
    log::warn!("Voice Creator Pro answers but its work hangs ({why}) – restarting it");
    match vcp_watch::restart(state).await {
        Ok(m) => {
            ttv_vcp::perf::clear_recent();
            log::info!("{m}");
        }
        Err(e) => log::warn!("restarting VCP failed: {e}"),
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
    // started again by the watchdog: the old process must be gone first
    watchdog::wait_for_predecessor();
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
            match DEV_BUILD {
                Some(b) => log::info!("TTV-TTS Proxy {} (dev build {b}) starting", env!("CARGO_PKG_VERSION")),
                None => log::info!("TTV-TTS Proxy {} starting", env!("CARGO_PKG_VERSION")),
            }

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
            tauri::async_runtime::spawn(internet::run(state.clone()));
            tauri::async_runtime::spawn(internet::duckdns(state.clone()));
            tauri::async_runtime::spawn(discovery::run(state.clone()));
            tauri::async_runtime::spawn(vcp_watch::run(state.clone()));
            tauri::async_runtime::spawn(awake::run(state.clone()));
            tauri::async_runtime::spawn(watchdog::run(app.handle().clone(), state.clone()));
            tauri::async_runtime::spawn(updater::run(app.handle().clone(), state.clone()));
            sysmon::spawn(state.clone());
            tauri::async_runtime::spawn(state.engine.worker());
            tauri::async_runtime::spawn(engine_events(state.clone()));
            // The stored OBS links start their chat readers, which start async
            // tasks: load them inside the async runtime. The setup hook runs
            // outside it, and a stored link ended every start with a panic.
            {
                let st = state.clone();
                tauri::async_runtime::block_on(async move { st.hosted.load(&st) });
            }
            tauri::async_runtime::spawn(hosted::serve(state.clone()));
            tauri::async_runtime::spawn(plugins::run(state.clone()));
            {
                let (a, b, c) = (Arc::downgrade(&state), Arc::downgrade(&state), Arc::downgrade(&state));
                tauri::async_runtime::spawn(state.upnp.clone().run(
                    move || a.upgrade().map(|s| internet::upnp_want(&s)).unwrap_or_default(),
                    move || c.upgrade().is_some_and(|s| internet::duckdns_login(&s).is_some()),
                    move || {
                        if let Some(s) = b.upgrade() {
                            s.publish_internet();
                        }
                    },
                ));
            }
            {
                let s = Arc::downgrade(&state);
                tauri::async_runtime::spawn(state.certs.clone().run(move || s.upgrade().and_then(|s| internet::https_want(&s))));
            }
            tauri::async_runtime::spawn(hosted::Hosted::supervise(state.clone()));

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
            commands::obs_links,
            commands::obs_remove,
            commands::plugins_view,
            commands::plugins_rescan,
            commands::plugins_open,
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
            commands::internet_status,
            commands::set_internet,
            commands::create_invite,
            commands::withdraw_invite,
            commands::revoke_guest,
            commands::set_guest_limit,
            commands::test_internet,
        ])
        .build(tauri::generate_context!())
        .expect("error while building TTV-TTS Proxy")
        .run(|app, event| match event {
            // Keep running in the tray when no window is open.
            RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
            RunEvent::Exit => {
                if let Some(s) = app.try_state::<Arc<AppState>>() {
                    s.hosted.flush();
                    // the router's ports close with the proxy
                    let u = s.upnp.clone();
                    tauri::async_runtime::block_on(async { tokio::time::timeout(std::time::Duration::from_secs(3), u.close()).await.ok() });
                }
            }
            _ => {}
        });
}

// Silence "unused" for state helpers only used on some platforms.
#[allow(dead_code)]
fn _assert_send_sync() {
    fn f<T: Send + Sync>() {}
    f::<Arc<AppState>>();
}
