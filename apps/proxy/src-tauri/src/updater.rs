//! Self-update from GitHub releases (signed; the public key is in
//! tauri.conf.json). Channel "stable" follows the latest release, "preview"
//! the rolling pre-release tagged `preview`. Automatic installs wait until the
//! proxy has been idle for 10 minutes, so an update never cuts into a stream.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

const STABLE: &str = "https://github.com/ZcarecroW/TTV-TTS/releases/latest/download/latest-proxy.json";
const PREVIEW: &str = "https://github.com/ZcarecroW/TTV-TTS/releases/download/preview/latest-proxy.json";
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
const IDLE_BEFORE_INSTALL: i64 = 600;

/// Unix seconds of the last forwarded request (set by `forward`).
pub static LAST_ACTIVITY: AtomicI64 = AtomicI64::new(0);

pub fn touch() {
    LAST_ACTIVITY.store(chrono::Utc::now().timestamp(), Ordering::Relaxed);
}

fn endpoint(channel: &str) -> url::Url {
    url::Url::parse(if channel == "preview" { PREVIEW } else { STABLE }).expect("updater URL")
}

fn set(state: &AppState, f: impl FnOnce(&mut ttv_link::UpdateState)) {
    f(&mut state.update.write());
    state.publish();
}

async fn find(app: &AppHandle, state: &AppState) -> Result<Option<tauri_plugin_updater::Update>, String> {
    let channel = state.cfg.read().update_channel.clone();
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint(&channel)])
        .map_err(|e| e.to_string())?
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| e.to_string())
}

pub async fn check(app: &AppHandle, state: &Arc<AppState>) {
    let channel = state.cfg.read().update_channel.clone();
    set(state, |u| {
        u.phase = "checking".into();
        u.error = None;
        u.channel = channel;
    });
    match find(app, state).await {
        Ok(found) => {
            if let Some(u) = &found {
                log::info!("update available: {}", u.version);
            }
            set(state, |u| {
                u.available = found.map(|f| f.version);
                u.phase = "idle".into();
            });
        }
        Err(e) => {
            log::warn!("update check failed: {e}");
            set(state, |u| {
                u.phase = "error".into();
                u.error = Some(e);
            });
        }
    }
}

pub async fn install(app: &AppHandle, state: &Arc<AppState>) {
    let update = match find(app, state).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            set(state, |u| {
                u.available = None;
                u.phase = "idle".into();
            });
            return;
        }
        Err(e) => {
            set(state, |u| {
                u.phase = "error".into();
                u.error = Some(e);
            });
            return;
        }
    };
    log::info!("installing update {}", update.version);
    set(state, |u| {
        u.phase = "downloading".into();
        u.progress = Some(0.0);
    });
    let st = state.clone();
    let mut got: u64 = 0;
    let result = update
        .download_and_install(
            move |chunk, total| {
                got += chunk as u64;
                if let Some(t) = total.filter(|t| *t > 0) {
                    st.update.write().progress = Some(got as f32 / t as f32);
                }
            },
            || log::info!("update downloaded, installing"),
        )
        .await;
    match result {
        // Windows: the installer has taken over and ends this process.
        // Elsewhere: restart into the new version.
        Ok(()) => app.restart(),
        Err(e) => {
            log::warn!("update failed: {e}");
            set(state, |u| {
                u.phase = "error".into();
                u.error = Some(e.to_string());
                u.progress = None;
            });
        }
    }
}

pub async fn run(app: AppHandle, state: Arc<AppState>) {
    tokio::time::sleep(Duration::from_secs(30)).await;
    loop {
        check(&app, &state).await;
        let auto = state.cfg.read().auto_update;
        if auto && state.update.read().available.is_some() {
            // Wait for a quiet moment: no traffic for 10 minutes, VCP idle.
            loop {
                let idle = chrono::Utc::now().timestamp() - LAST_ACTIVITY.load(Ordering::Relaxed);
                let busy = state.vcp_state.read().generating;
                if idle >= IDLE_BEFORE_INSTALL && !busy {
                    install(&app, &state).await;
                    break;
                }
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}
