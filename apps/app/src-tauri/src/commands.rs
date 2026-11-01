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

fn view(state: &AppState) -> AppView {
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
        c.host = HostConfig::Proxy { id: probe.info.id.clone(), name: probe.info.name.clone(), address: probe.address.clone(), fingerprint: probe.fingerprint.clone(), mac: None };
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
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
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
