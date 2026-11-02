//! TTV-TTS: reads Twitch chat aloud with Voice Creator Pro voices, on this PC
//! or through TTV-TTS Proxy on a second PC.

mod audio;
mod classic;
mod commands;
mod config;
mod error;
mod host;
mod hosted_cmds;
mod library;
mod logbuf;
mod monitor;
mod obs;
mod friends_cmds;
mod bench;
#[cfg(desktop)]
mod hotkeys;
mod os_voice;
mod plugin_cmds;
mod twitch_cmds;
mod remote;
mod pack_cmds;
mod preset_cmds;
mod quality_cmds;
mod presets;
mod reader;
mod secrets;
mod speech;
mod state;
mod storage;
mod twitch;
mod updater;

// The chat reader parts shared with the proxy, under their old names.
pub use ttv_reader::{announce, fx, profile, text, translate};

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
        builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());
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
            tauri::async_runtime::spawn(async move { reader::start(&st) });
            tauri::async_runtime::spawn(updater::run(app.handle().clone(), state.clone()));
            let st = state.clone();
            tauri::async_runtime::spawn(async move { remote::apply(&st).await });
            let st = state.clone();
            tauri::async_runtime::spawn(async move { obs::apply(&st).await });
            tauri::async_runtime::spawn(obs::duckdns(state.clone()));
            {
                let (s, s2) = (std::sync::Arc::downgrade(&state), std::sync::Arc::downgrade(&state));
                tauri::async_runtime::spawn(state.upnp.clone().run(move || s.upgrade().map(|s| obs::upnp_want(&s)).unwrap_or_default(), move || s2.upgrade().is_some_and(|s| obs::wants_duckdns(&s)), || {}));
            }
            {
                let s = std::sync::Arc::downgrade(&state);
                tauri::async_runtime::spawn(state.certs.clone().run(move || s.upgrade().and_then(|s| obs::https_want(&s))));
            }
            // audio plugins: load the chain once the output device runs, keep
            // the settings the plugin thread hands over
            let st = state.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                let any = { let c = st.cfg.read(); !c.plugins.is_empty() || !c.viewer_plugins.is_empty() };
                if any {
                    plugin_cmds::apply(&st).await;
                }
            });
            if let Some(rx) = state.plugin_states.lock().take() {
                let st = state.clone();
                std::thread::Builder::new()
                    .name("ttv-plugin-states".into())
                    .spawn(move || {
                        while let Ok((id, data)) = rx.recv() {
                            plugin_cmds::keep_state(&st, &id, &data);
                        }
                    })
                    .ok();
            }
            #[cfg(desktop)]
            for a in hotkeys::apply(app.handle(), &state) {
                log::warn!("hotkey for {a} not available (taken by another program?)");
            }
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
            commands::try_translate,
            commands::set_deepl_key,
            commands::debug_vcp_get,
            commands::debug_vcp_low_vram,
            commands::debug_fail_vcp,
            commands::skip,
            commands::get_reader,
            commands::save_profile,
            commands::delete_profile,
            commands::set_active_profiles,
            commands::reader_skip,
            commands::reader_clear,
            commands::reader_pause,
            commands::reader_approve,
            commands::reader_reject,
            commands::reader_replay,
            commands::set_hotkey,
            commands::set_remote,
            commands::obs_status,
            commands::set_obs,
            commands::obs_new_link,
            hosted_cmds::hosted_list,
            hosted_cmds::hosted_history,
            hosted_cmds::hosted_control,
            hosted_cmds::hosted_test,
            hosted_cmds::hosted_viewers,
            hosted_cmds::hosted_viewer,
            twitch_cmds::twitch_status,
            twitch_cmds::twitch_login,
            twitch_cmds::twitch_cancel_login,
            twitch_cmds::twitch_logout,
            twitch_cmds::set_twitch_client_id,
            plugin_cmds::plugins_list,
            plugin_cmds::plugins_scan,
            plugin_cmds::plugins_pick,
            plugin_cmds::plugins_add,
            plugin_cmds::plugins_remove,
            plugin_cmds::plugins_toggle,
            plugin_cmds::plugins_move,
            plugin_cmds::plugins_edit,
            plugin_cmds::plugins_edit_on_host,
            plugin_cmds::plugins_host,
            plugin_cmds::plugins_host_scan,
            plugin_cmds::viewer_plugins_add,
            plugin_cmds::viewer_plugins_set,
            plugin_cmds::viewer_plugins_remove,
            plugin_cmds::set_viewer_plugin,
            plugin_cmds::debug_plugin_file,
            plugin_cmds::debug_plugins_probe,
            plugin_cmds::debug_plugin_editor,
            commands::generation_info,
            commands::measure_generation,
            commands::remote_status,
            commands::test_message,
            commands::test_event,
            commands::mute_viewer,
            commands::list_viewers,
            commands::set_viewer_role,
            commands::set_viewer_fx,
            commands::os_voices,
            commands::set_os_voices,
            commands::try_os_voice,
            commands::install_os_voices,
            commands::open_os_voice_settings,
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
            preset_cmds::debug_sample,
            pack_cmds::debug_proxy_fetch,
            preset_cmds::list_takes,
            preset_cmds::play_take,
            preset_cmds::choose_take,
            preset_cmds::discard_takes,
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
                let st = app.state::<std::sync::Arc<AppState>>().inner().clone();
                // the plugins' latest settings (an open editor may have changed them)
                tauri::async_runtime::block_on(async { tokio::time::timeout(std::time::Duration::from_secs(2), plugin_cmds::save_all(&st)).await.ok() });
                st.reader.flush();
                // the router's port closes with the app
                let u = st.upnp.clone();
                tauri::async_runtime::block_on(async { tokio::time::timeout(std::time::Duration::from_secs(3), u.close()).await.ok() });
            }
        });
}
