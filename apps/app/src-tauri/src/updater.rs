//! Signed updates from GitHub releases. The app checks at start and every 6
//! hours and shows a notice; installing is the user's choice (never in the
//! middle of a stream). Channel "stable" = latest release, "preview" = the
//! rolling pre-release tagged `preview`.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

const STABLE: &str = "https://github.com/ZcarecroW/TTV-TTS/releases/latest/download/latest.json";
const PREVIEW: &str = "https://github.com/ZcarecroW/TTV-TTS/releases/download/preview/latest.json";

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateInfo {
    pub available: Option<String>,
    /// "idle", "checking", "downloading", "error"
    pub phase: String,
    pub error: Option<String>,
}

static INFO: RwLock<Option<UpdateInfo>> = RwLock::new(None);

pub fn info() -> UpdateInfo {
    INFO.read().clone().unwrap_or_else(|| UpdateInfo { phase: "idle".into(), ..Default::default() })
}

fn set(app: &AppHandle, i: UpdateInfo) {
    *INFO.write() = Some(i.clone());
    let _ = app.emit("app://update", &i);
}

async fn find(app: &AppHandle, channel: &str) -> Result<Option<tauri_plugin_updater::Update>, String> {
    let url = url::Url::parse(if channel == "preview" { PREVIEW } else { STABLE }).expect("updater URL");
    app.updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())
}

pub async fn check(app: &AppHandle, state: &Arc<AppState>) -> UpdateInfo {
    let channel = state.cfg.read().update_channel.clone();
    set(app, UpdateInfo { phase: "checking".into(), ..info() });
    let i = match find(app, &channel).await {
        Ok(u) => UpdateInfo { available: u.map(|u| u.version), phase: "idle".into(), error: None },
        Err(e) => {
            log::warn!("update check failed: {e}");
            UpdateInfo { phase: "error".into(), error: Some(e), ..info() }
        }
    };
    set(app, i.clone());
    i
}

pub async fn install(app: &AppHandle, state: &Arc<AppState>) -> Result<(), String> {
    let channel = state.cfg.read().update_channel.clone();
    let Some(update) = find(app, &channel).await? else { return Ok(()) };
    set(app, UpdateInfo { phase: "downloading".into(), ..info() });
    crate::reader::flush(state);
    update.download_and_install(|_, _| {}, || log::info!("update downloaded, installing")).await.map_err(|e| {
        set(app, UpdateInfo { phase: "error".into(), error: Some(e.to_string()), ..info() });
        e.to_string()
    })?;
    app.restart();
}

pub async fn run(app: AppHandle, state: Arc<AppState>) {
    tokio::time::sleep(Duration::from_secs(20)).await;
    loop {
        check(&app, &state).await;
        tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
    }
}
