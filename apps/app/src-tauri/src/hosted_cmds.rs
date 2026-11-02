//! Profiles that play "in OBS through the voice host PC": the proxy reads
//! their chat itself (same reader as here) and serves the OBS link, so this
//! PC does nothing for them – it can even be off. The app hands them over
//! with their preset's voices and the viewers' voices, keeps them up to date
//! and shows what the proxy reports.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use ttv_reader::profile::{OutputMode, VoiceMode};

use crate::config::HostConfig;
use crate::state::AppState;

type St<'a> = tauri::State<'a, Arc<AppState>>;

const T: Duration = Duration::from_secs(15);

/// What went wrong, in words: an older proxy doesn't know OBS links.
fn why(e: ttv_vcp::Error) -> String {
    match e {
        ttv_vcp::Error::Http { status: 404, .. } => "The TTV-TTS Proxy on the voice host PC is too old for OBS links – update it (Voice host → Update proxy).".into(),
        e => e.to_string(),
    }
}

/// The proxy (not as a guest): only it can read profiles for us.
fn proxy(state: &AppState) -> Result<ttv_vcp::Client, String> {
    match &state.cfg.read().host {
        HostConfig::Proxy { guest: false, .. } => {}
        HostConfig::Proxy { guest: true, .. } => return Err("OBS links on the voice host PC need your own TTV-TTS Proxy (not a friend's).".into()),
        _ => return Err("OBS links on the voice host PC need TTV-TTS Proxy on a second PC.".into()),
    }
    state.vcp().ok_or_else(|| "The voice host isn't connected.".into())
}

/// Hands a profile over to the proxy (or updates it there).
pub async fn push(state: &Arc<AppState>, id: &str) -> Result<serde_json::Value, String> {
    let c = proxy(state)?;
    let profile = state.reader.store.read().profiles.iter().find(|p| p.id == id).cloned().ok_or("unknown profile")?;
    let preset = (profile.voices.mode == VoiceMode::Preset)
        .then(|| {
            state.presets.with(&profile.voices.preset, |p| {
                let n = match profile.voices.preset_voices {
                    0 => p.defs.len(),
                    k => (k as usize).min(p.defs.len()),
                };
                serde_json::json!({ "id": p.id, "defs": p.defs[..n] })
            })
        })
        .flatten();
    let settings = {
        let streaming = crate::bench::mode(state) == crate::config::GenerationMode::Streaming;
        let cfg = state.cfg.read();
        serde_json::json!({ "gap_ms": cfg.audio.gap_ms, "streaming": streaming, "translate_email": cfg.translate_email })
    };
    let (roles, fx) = state.reader.viewer_roles(id);
    let body = serde_json::json!({ "profile": profile, "preset": preset, "settings": settings, "roles": roles, "fx": fx });
    c.put(&format!("/ttv/v1/hosted/{id}"), &body, T).await.map_err(why)
}

/// Takes a profile back from the proxy (and the viewers' voices it gave out).
pub async fn take_back(state: &Arc<AppState>, id: &str) -> Result<(), String> {
    let c = proxy(state)?;
    // a deleted profile doesn't need its viewers back
    let exists = state.reader.store.read().profiles.iter().any(|p| p.id == id);
    if exists && let Ok(v) = c.get::<serde_json::Value>(&format!("/ttv/v1/hosted/{id}/viewers?limit=100000"), T).await {
        let mut roles = std::collections::HashMap::new();
        let mut fx = std::collections::HashMap::new();
        for row in v["viewers"].as_array().into_iter().flatten() {
            let Some(user) = row["user_id"].as_str() else { continue };
            if let Some(r) = row["role"].as_u64() {
                roles.insert(user.to_string(), r as u32);
            }
            if let Some(f) = row["fx"].as_str() {
                fx.insert(user.to_string(), f.to_string());
            }
        }
        state.reader.adopt_viewers(id, &roles, &fx);
    }
    c.request(reqwest::Method::DELETE, &format!("/ttv/v1/hosted/{id}")).timeout(T).send().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// After the voice host (re)connected or a preset changed: hands over every
/// profile that plays there again (the proxy may have been reinstalled, a
/// preset may have new voices).
pub async fn sync_all(state: &Arc<AppState>) {
    if proxy(state).is_err() {
        return;
    }
    let ids: Vec<String> = state.reader.store.read().profiles.iter().filter(|p| p.output.mode == OutputMode::HostObs).map(|p| p.id.clone()).collect();
    for id in ids {
        if let Err(e) = push(state, &id).await {
            log::warn!("OBS link on the voice host: {e}");
        }
    }
}

/// A profile was saved: hand it over, update it, or take it back.
pub fn profile_saved(state: &Arc<AppState>, id: &str, was: Option<OutputMode>) {
    let now = state.reader.store.read().profiles.iter().find(|p| p.id == id).map(|p| p.output.mode);
    let (st, id) = (state.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        let r = match (was, now) {
            (_, Some(OutputMode::HostObs)) => push(&st, &id).await.map(|_| ()),
            (Some(OutputMode::HostObs), _) => take_back(&st, &id).await,
            _ => Ok(()),
        };
        if let Err(e) = r {
            log::warn!("OBS link on the voice host: {e}");
        }
    });
}

// ── commands ────────────────────────────────────────────────────────────

/// The profiles the voice host PC reads for OBS, with links and status.
#[tauri::command]
pub async fn hosted_list(state: St<'_>) -> Result<serde_json::Value, String> {
    let c = proxy(&state)?;
    c.get("/ttv/v1/hosted", T).await.map_err(why)
}

#[tauri::command]
pub async fn hosted_history(state: St<'_>, id: String) -> Result<serde_json::Value, String> {
    let c = proxy(&state)?;
    c.get(&format!("/ttv/v1/hosted/{id}/history"), T).await.map_err(why)
}

/// skip, clear, pause, resume, replay
#[tauri::command]
pub async fn hosted_control(state: St<'_>, id: String, action: String) -> Result<(), String> {
    let c = proxy(&state)?;
    c.post::<_, serde_json::Value>(&format!("/ttv/v1/hosted/{id}/do/{action}"), &(), T).await.map(|_| ()).map_err(why)
}

#[tauri::command]
pub async fn hosted_test(state: St<'_>, id: String, user: String, text: String) -> Result<(), String> {
    let c = proxy(&state)?;
    c.post::<_, serde_json::Value>(&format!("/ttv/v1/hosted/{id}/test"), &serde_json::json!({ "user": user, "text": text }), T).await.map(|_| ()).map_err(why)
}

#[tauri::command]
pub async fn hosted_viewers(state: St<'_>, id: String, query: String) -> Result<serde_json::Value, String> {
    let c = proxy(&state)?;
    let q: String = query.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
    c.get(&format!("/ttv/v1/hosted/{id}/viewers?q={q}&limit=300"), T).await.map_err(why)
}

#[derive(Debug, Deserialize)]
pub struct ViewerPatch(serde_json::Value);

/// One change of a viewer there: `{"role": 3}`, `{"role": null}`, `{"fx": "radio"}`, `{"forget": true}`.
#[tauri::command]
pub async fn hosted_viewer(state: St<'_>, id: String, user_id: String, patch: ViewerPatch) -> Result<(), String> {
    let c = proxy(&state)?;
    c.post::<_, serde_json::Value>(&format!("/ttv/v1/hosted/{id}/viewers/{user_id}"), &patch.0, T).await.map(|_| ()).map_err(why)
}
