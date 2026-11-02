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
    /// A DeepL API key is stored.
    pub deepl_key: bool,
}

pub fn view(state: &AppState) -> AppView {
    AppView {
        config: state.cfg.read().clone(),
        host: state.host.read().clone(),
        devices: audio::devices(),
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: state.data_dir.to_string_lossy().into_owned(),
        deepl_key: state.secrets.get(crate::secrets::DEEPL_KEY).is_some(),
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
    translate_email: Option<String>,
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
        if let Some(v) = patch.translate_email {
            c.translate_email = v.trim().to_string();
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

#[derive(Serialize)]
pub struct Translated {
    pub text: String,
    pub ms: u64,
}

/// Development builds only: reads a VCP setting or status (GET under
/// /api/v1/) through the voice host, for checks against the real setup.
#[tauri::command]
pub async fn debug_vcp_get(state: St<'_>, path: String) -> Result<serde_json::Value, String> {
    if !cfg!(debug_assertions) {
        return Err("only in development builds".into());
    }
    if !path.starts_with("/api/v1/") || path.contains("..") {
        return Err("only VCP API paths".into());
    }
    let vcp = state.vcp().ok_or("no voice host")?;
    vcp.get(&path, Duration::from_secs(20)).await.map_err(|e| e.to_string())
}

/// Development builds only: changes VCP's low-VRAM setting (PUT
/// /api/v1/settings/low-vram), for measurements against the real setup.
#[tauri::command]
pub async fn debug_vcp_low_vram(state: St<'_>, enabled: bool) -> Result<serde_json::Value, String> {
    if !cfg!(debug_assertions) {
        return Err("only in development builds".into());
    }
    let vcp = state.vcp().ok_or("no voice host")?;
    vcp.put(&format!("/api/v1/settings/low-vram?enabled={enabled}"), &serde_json::json!({}), Duration::from_secs(60)).await.map_err(|e| e.to_string())
}

/// Development builds only: every VCP generation fails while on.
#[tauri::command]
pub fn debug_fail_vcp(on: bool) -> Result<(), String> {
    if !cfg!(debug_assertions) {
        return Err("only in development builds".into());
    }
    speech::DEBUG_FAIL.store(on, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Translates a text the way "translate for me" would (to try it).
#[tauri::command]
pub async fn try_translate(state: St<'_>, text: String, from: Option<String>, target: String, provider: String) -> Result<Translated, String> {
    let from = match from.filter(|f| !f.is_empty()) {
        Some(f) => f,
        None => crate::text::lang::detect(&text).map(str::to_string).ok_or("The language of this text isn't clear – write a little more.")?,
    };
    let email = state.cfg.read().translate_email.clone();
    let deepl = state.secrets.get(crate::secrets::DEEPL_KEY);
    let vcp = state.vcp();
    let t0 = std::time::Instant::now();
    let svc = crate::translate::Service { provider: &provider, email: &email, deepl_key: deepl.as_deref(), vcp: vcp.as_ref() };
    let out = ttv_reader::reader::Host::translator(&state.reader.host).translate(&svc, &text, &from, &target).await?;
    Ok(Translated { text: out, ms: t0.elapsed().as_millis() as u64 })
}

/// Stores (or with None/"" removes) the DeepL API key in the system keyring.
#[tauri::command]
pub fn set_deepl_key(state: St, key: Option<String>) -> AppResult<AppView> {
    let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    state.secrets.set(crate::secrets::DEEPL_KEY, key.as_deref())?;
    Ok(view(&state))
}

#[tauri::command]
pub async fn speak_test(state: St<'_>, utterance: Utterance) -> Result<SayReport, String> {
    let vcp = state.vcp().ok_or("No voice host set up yet.")?;
    speech::say(vcp, &state, utterance).await
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
    state.reader.apply_channels();
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
    state.reader.apply_channels();
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
    state.reader.apply_channels();
    reader_view(&state)
}

#[tauri::command]
pub fn reader_skip(state: St) {
    state.reader.skip();
}

#[tauri::command]
pub fn reader_clear(state: St) {
    state.reader.clear("app");
}

#[tauri::command]
pub fn reader_pause(state: St, on: bool) {
    state.reader.set_paused(on);
}

/// Approval mode: a waiting message is read (None = the oldest).
#[tauri::command]
pub fn reader_approve(state: St, id: Option<u64>) -> bool {
    state.reader.approve(id)
}

/// Approval mode: a waiting message is dropped (None = the oldest).
#[tauri::command]
pub fn reader_reject(state: St, id: Option<u64>) -> bool {
    state.reader.reject(id)
}

/// Plays a spoken message again (None = the last one), without VCP.
#[tauri::command]
pub fn reader_replay(state: St, id: Option<u64>) -> Result<(), String> {
    state.reader.replay(id)
}

/// The last speed measurement of the two generation modes (this host).
#[tauri::command]
pub fn generation_info(state: St) -> Option<crate::bench::Measured> {
    crate::bench::current(&state)
}

/// Measures whole sentences against streaming now (~30 s on a slow setup).
#[tauri::command]
pub async fn measure_generation(state: St<'_>) -> Result<crate::bench::Measured, String> {
    crate::bench::run(&state).await
}

/// Remote control (Stream Deck, Streamer.bot): switch, port, LAN, new token.
#[tauri::command]
pub async fn set_remote(state: St<'_>, enabled: bool, port: u16, lan: bool, new_token: bool) -> Result<AppView, String> {
    {
        let mut c = state.cfg.write();
        c.remote.enabled = enabled;
        c.remote.port = port.max(1024);
        c.remote.lan = lan;
        if new_token {
            c.remote.token = crate::secrets::random_key()[..24].to_string();
        }
    }
    state.save_config().map_err(|e| e.to_string())?;
    crate::remote::apply(&state).await;
    Ok(view(&state))
}

#[tauri::command]
pub fn remote_status() -> crate::remote::RemoteStatus {
    crate::remote::status()
}

/// Sets (or with None/"" removes) the global hotkey for an action.
#[tauri::command]
pub fn set_hotkey(app: tauri::AppHandle, state: St, action: String, accel: Option<String>) -> Result<AppView, String> {
    #[cfg(desktop)]
    {
        if !crate::hotkeys::ACTIONS.contains(&action.as_str()) {
            return Err(format!("unknown action {action}"));
        }
        let accel = accel.map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
        let before = state.cfg.read().hotkeys.clone();
        if let Some(a) = &accel {
            let wanted = crate::hotkeys::parse(a)?;
            if let Some((other, _)) = before.iter().find(|(k, v)| **k != action && crate::hotkeys::parse(v).is_ok_and(|s| s == wanted)) {
                return Err(format!("This combination is already used for \"{other}\"."));
            }
        }
        {
            let mut c = state.cfg.write();
            match &accel {
                Some(a) => {
                    c.hotkeys.insert(action.clone(), a.clone());
                }
                None => {
                    c.hotkeys.remove(&action);
                }
            }
        }
        let refused = crate::hotkeys::apply(&app, &state);
        if refused.contains(&action) {
            state.cfg.write().hotkeys = before;
            crate::hotkeys::apply(&app, &state);
            return Err("Another program already uses this combination.".into());
        }
        state.save_config().map_err(|e| e.to_string())?;
    }
    #[cfg(not(desktop))]
    let _ = (app, action, accel);
    Ok(view(&state))
}

/// An example event announcement (sub, resub, subgift, submysterygift, raid, bits).
#[tauri::command]
pub fn test_event(state: St, profile: String, kind: String) -> Result<(), String> {
    state.reader.test_event(&profile, &kind)
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
    state.reader.test_message(&channel, if user.trim().is_empty() { "TestUser" } else { user.trim() }, &text);
    Ok(())
}

#[tauri::command]
pub fn mute_viewer(state: St, profile: String, login: String, on: bool) -> ReaderView {
    state.reader.set_muted(&profile, &login.to_ascii_lowercase(), on);
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
    let (viewers, total) = state.reader.viewers(&profile, &query, 300);
    ViewerList { viewers, total }
}

/// Another voice for a viewer; None = a new random one on the next message.
#[tauri::command]
pub fn set_viewer_role(state: St, profile: String, user_id: String, role: Option<u32>) {
    state.reader.set_role(&profile, &user_id, role);
}

/// A voice effect for a viewer: a name, "none" or None (the voice's own).
#[tauri::command]
pub fn set_viewer_fx(state: St, profile: String, user_id: String, fx: Option<String>) {
    state.reader.set_fx(&profile, &user_id, fx);
}

#[tauri::command]
pub fn forget_viewer(state: St, profile: String, user_id: String) {
    state.reader.forget_viewer(&profile, &user_id);
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
