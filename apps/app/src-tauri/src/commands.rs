//! Commands for the window.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audio::{self, OutputDevice};
use crate::config::{Config, GenerationMode, HostConfig};
use crate::error::{AppError, AppResult};
use crate::host::{self, Found, Probe};
use crate::speech::{self, SayReport, Utterance};
use crate::state::{AppState, HostStatus};

type St<'a> = State<'a, Arc<AppState>>;

#[derive(Serialize)]
pub struct AppView {
    pub config: Config,
    pub host: HostStatus,
    pub devices: Vec<OutputDevice>,
    pub version: String,
    pub data_dir: String,
}

pub fn view(state: &AppState) -> AppView {
    AppView {
        config: state.cfg.read().clone(),
        host: state.host.read().clone(),
        devices: audio::devices(),
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: state.data_dir.to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub fn get_state(state: St) -> AppView {
    view(&state)
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct SettingsPatch {
    language: Option<String>,
    theme: Option<String>,
    update_channel: Option<String>,
    generation: Option<GenerationMode>,
    setup_done: Option<bool>,
}

#[tauri::command]
pub fn update_settings(state: St, patch: SettingsPatch) -> AppResult<AppView> {
    {
        let mut c = state.cfg.write();
        if let Some(v) = patch.language {
            c.language = v;
        }
        if let Some(v) = patch.theme {
            c.theme = v;
        }
        if let Some(v) = patch.update_channel {
            c.update_channel = v;
        }
        if let Some(v) = patch.generation {
            c.generation = v;
        }
        if let Some(v) = patch.setup_done {
            c.setup_done = v;
        }
        c.normalize();
    }
    state.save_config()?;
    Ok(view(&state))
}

// ── voice host ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn discover_proxies(state: St<'_>) -> AppResult<Vec<Found>> {
    // Also ask the remembered proxy directly (broadcasts may be filtered).
    let extra: Vec<std::net::SocketAddr> = match &state.cfg.read().host {
        HostConfig::Proxy { address, .. } => address.parse().ok().into_iter().collect(),
        _ => Vec::new(),
    };
    Ok(host::discover(Duration::from_millis(1200), &extra).await)
}

#[tauri::command]
pub async fn probe_proxy(address: String) -> Result<Probe, String> {
    host::probe(&address).await
}

/// Pairs with a proxy and makes it the voice host.
#[tauri::command]
pub async fn pair_proxy(state: St<'_>, address: String, fingerprint: String, code: String) -> Result<AppView, String> {
    // Re-check the certificate the user just compared.
    let probe = host::probe(&address).await?;
    if probe.fingerprint != fingerprint {
        return Err("The proxy's certificate changed since you looked at it. Search again.".into());
    }
    let (device_id, device_name) = (state.cfg.read().device_id.clone(), crate::config::device_name());
    let res = host::pair(&probe, &code, &device_id, &device_name).await?;
    state.secrets.set(&crate::secrets::proxy_token(&probe.info.id), Some(&res.token)).map_err(|e| e.to_string())?;
    {
        let mut c = state.cfg.write();
        c.host = HostConfig::Proxy { id: probe.info.id.clone(), name: probe.info.name.clone(), address: probe.address.clone(), fingerprint: probe.fingerprint.clone(), mac: None, guest: false };
    }
    state.save_config().map_err(|e| e.to_string())?;
    state.host_updated();
    log::info!("paired with proxy \"{}\" at {}", probe.info.name, probe.address);
    Ok(view(&state))
}

/// Uses Voice Creator Pro on this PC directly.
#[tauri::command]
pub async fn use_direct(state: St<'_>, url: String, key: Option<String>) -> Result<AppView, String> {
    let url = url.trim().trim_end_matches('/').to_string();
    let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    let client = ttv_vcp::Client::new(&url, key.clone());
    client.health().await.map_err(|e| format!("Voice Creator Pro doesn't answer at {url}: {e}"))?;
    match client.check_key().await {
        Ok(()) => {}
        Err(ttv_vcp::Error::Unauthorized) => {
            return Err(if key.is_some() { "VCP rejected this API key." } else { "VCP has an API key set - enter it." }.into());
        }
        Err(e) => return Err(e.to_string()),
    }
    state.secrets.set(crate::secrets::VCP_KEY, key.as_deref()).map_err(|e| e.to_string())?;
    state.cfg.write().host = HostConfig::Direct { url };
    state.save_config().map_err(|e| e.to_string())?;
    state.host_updated();
    Ok(view(&state))
}

#[tauri::command]
pub fn forget_host(state: St) -> AppResult<AppView> {
    let old = std::mem::take(&mut state.cfg.write().host);
    if let HostConfig::Proxy { id, .. } = old {
        let _ = state.secrets.set(&crate::secrets::proxy_token(&id), None);
    }
    state.save_config()?;
    state.host_updated();
    Ok(view(&state))
}

#[tauri::command]
pub fn watch_host_system(state: St, on: bool) {
    state.watch_system.store(on, Ordering::Relaxed);
}

/// Start / stop / restart VCP through the proxy.
#[tauri::command]
pub async fn proxy_vcp_action(state: St<'_>, action: String) -> Result<String, String> {
    let vcp = state.vcp().ok_or("no voice host")?;
    let ack: ttv_link::Ack = vcp
        .post(&format!("/ttv/v1/vcp/{action}"), &serde_json::json!({}), Duration::from_secs(300))
        .await
        .map_err(|e| e.to_string())?;
    if ack.ok { Ok(ack.message.unwrap_or_default()) } else { Err(ack.message.unwrap_or_default()) }
}

/// Updates TTV-TTS Proxy from here: "check" looks for a newer version,
/// "install" installs it now (the proxy restarts, ~10 s).
#[tauri::command]
pub async fn proxy_update(state: St<'_>, action: String) -> Result<Option<ttv_link::UpdateState>, String> {
    if !matches!(state.cfg.read().host, HostConfig::Proxy { .. }) {
        return Err("Only a TTV-TTS Proxy can be updated from here.".into());
    }
    let vcp = state.vcp().ok_or("no voice host")?;
    match action.as_str() {
        "check" => vcp.post("/ttv/v1/update/check", &serde_json::json!({}), Duration::from_secs(60)).await.map(Some).map_err(|e| e.to_string()),
        "install" => {
            let _: ttv_link::Ack = vcp.post("/ttv/v1/update/install", &serde_json::json!({}), Duration::from_secs(30)).await.map_err(|e| e.to_string())?;
            Ok(None)
        }
        _ => Err(format!("unknown action {action}")),
    }
}

/// Wake-on-LAN for the proxy PC (MAC learned from the proxy's status).
#[tauri::command]
pub async fn wake_host(state: St<'_>) -> Result<(), String> {
    let mac = {
        let h = state.host.read();
        h.proxy.as_ref().and_then(|p| p.addresses.first().map(|a| a.mac.clone()))
    }
    .or_else(|| match &state.cfg.read().host {
        HostConfig::Proxy { mac, .. } => mac.clone(),
        _ => None,
    })
    .ok_or("The proxy PC's network address isn't known yet - connect once while it is on.")?;
    host::wake(&mac).await
}

// ── audio & speech ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AudioPatch {
    device: Option<Option<String>>,
    volume: Option<f32>,
    gap_ms: Option<u32>,
}

#[tauri::command]
pub fn set_audio(state: St, patch: AudioPatch) -> AppResult<AppView> {
    {
        let mut c = state.cfg.write();
        if let Some(d) = patch.device {
            c.audio.device = d.clone();
            state.audio.open(d);
        }
        if let Some(v) = patch.volume {
            c.audio.volume = v.clamp(0.0, 1.0);
            state.audio.set_volume(c.audio.volume);
        }
        if let Some(g) = patch.gap_ms {
            c.audio.gap_ms = g.min(3000);
        }
    }
    state.save_config()?;
    Ok(view(&state))
}

#[tauri::command]
pub async fn list_voices(state: St<'_>) -> AppResult<Vec<ttv_vcp::Voice>> {
    let vcp = state.vcp().ok_or_else(|| AppError::msg("No voice host set up yet."))?;
    let mut v = vcp.voices().await?;
    v.sort_by_cached_key(|a| a.name.to_lowercase());
    Ok(v)
}

#[tauri::command]
pub async fn speak_test(state: St<'_>, utterance: Utterance) -> Result<SayReport, String> {
    let vcp = state.vcp().ok_or("No voice host set up yet.")?;
    speech::say(vcp, state.audio.clone(), utterance).await
}

#[tauri::command]
pub fn skip(state: St) {
    state.audio.skip_all();
}

// ── chat reader ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ReaderView {
    pub store: crate::reader::ProfileStore,
    pub status: crate::reader::ReaderStatus,
    pub history: Vec<crate::reader::ItemView>,
}

fn reader_view(state: &AppState) -> ReaderView {
    ReaderView { store: state.reader.store.read().clone(), status: state.reader.status(), history: state.reader.history() }
}

#[tauri::command]
pub fn get_reader(state: St) -> ReaderView {
    reader_view(&state)
}

/// Creates or updates a profile (an empty id creates a new one).
#[tauri::command]
pub fn save_profile(state: St, profile: crate::profile::Profile) -> ReaderView {
    let mut p = profile;
    p.normalize();
    {
        let mut s = state.reader.store.write();
        match s.profiles.iter_mut().find(|x| x.id == p.id) {
            Some(existing) => *existing = p,
            None => s.profiles.push(p),
        }
    }
    state.reader.save_store();
    crate::reader::apply_channels(&state);
    reader_view(&state)
}

#[tauri::command]
pub fn delete_profile(state: St, id: String) -> ReaderView {
    {
        let mut s = state.reader.store.write();
        if s.profiles.len() > 1 {
            s.profiles.retain(|p| p.id != id);
            s.active.retain(|a| *a != id);
        }
    }
    state.reader.save_store();
    crate::reader::apply_channels(&state);
    reader_view(&state)
}

/// Which profiles read their channels now (several at once are fine).
#[tauri::command]
pub fn set_active_profiles(state: St, ids: Vec<String>) -> ReaderView {
    {
        let mut s = state.reader.store.write();
        let known: Vec<String> = s.profiles.iter().map(|p| p.id.clone()).collect();
        s.active = ids.into_iter().filter(|i| known.contains(i)).collect();
    }
    state.reader.save_store();
    crate::reader::apply_channels(&state);
    reader_view(&state)
}

#[tauri::command]
pub fn reader_skip(state: St) {
    crate::reader::skip(&state);
}

#[tauri::command]
pub fn reader_clear(state: St) {
    crate::reader::clear(&state, "app");
}

#[tauri::command]
pub fn reader_pause(state: St, on: bool) {
    crate::reader::set_paused(&state, on);
}

/// Sends a test message through the pipeline of a profile's first channel.
#[tauri::command]
pub fn test_message(state: St, profile: String, user: String, text: String) -> Result<(), String> {
    let channel = {
        let s = state.reader.store.read();
        let p = s.profiles.iter().find(|p| p.id == profile).ok_or("unknown profile")?;
        if !s.active.contains(&p.id) {
            return Err("Switch this profile on first.".into());
        }
        p.channels.first().cloned().ok_or("Add a channel to this profile first.")?
    };
    crate::reader::test_message(&state, &channel, if user.trim().is_empty() { "TestUser" } else { user.trim() }, &text);
    Ok(())
}

#[tauri::command]
pub fn mute_viewer(state: St, profile: String, login: String, on: bool) -> ReaderView {
    crate::reader::set_muted(&state, &profile, &login.to_ascii_lowercase(), on);
    reader_view(&state)
}

#[derive(Serialize)]
pub struct ViewerList {
    pub viewers: Vec<crate::reader::ViewerView>,
    /// All matching viewers (the list shows the most recent ones).
    pub total: usize,
}

#[tauri::command]
pub fn list_viewers(state: St, profile: String, query: String) -> ViewerList {
    let (viewers, total) = crate::reader::viewers(&state, &profile, &query, 300);
    ViewerList { viewers, total }
}

/// Another voice for a viewer; None = a new random one on the next message.
#[tauri::command]
pub fn set_viewer_role(state: St, profile: String, user_id: String, role: Option<u32>) {
    crate::reader::set_role(&state, &profile, &user_id, role);
}

/// A voice effect for a viewer: a name, "none" or None (the voice's own).
#[tauri::command]
pub fn set_viewer_fx(state: St, profile: String, user_id: String, fx: Option<String>) {
    crate::reader::set_fx(&state, &profile, &user_id, fx);
}

#[tauri::command]
pub fn forget_viewer(state: St, profile: String, user_id: String) {
    crate::reader::forget_viewer(&state, &profile, &user_id);
}

// ── updates ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn update_info() -> crate::updater::UpdateInfo {
    crate::updater::info()
}

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle, state: St<'_>) -> Result<crate::updater::UpdateInfo, String> {
    Ok(crate::updater::check(&app, &state).await)
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle, state: St<'_>) -> Result<(), String> {
    crate::updater::install(&app, &state).await
}
