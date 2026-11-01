//! Friends over the internet: the host manages internet access of their
//! proxy (LAN API), a friend joins with an invite code and then uses the
//! host's computed voices as a guest.

use std::sync::Arc;
use std::time::Duration;

use tauri::State;
use ttv_link::{InviteCode, JoinRequest, JoinResponse};

use crate::commands::{AppView, view};
use crate::config::HostConfig;
use crate::state::AppState;

type St<'a> = State<'a, Arc<AppState>>;
type Json = serde_json::Value;

/// The paired proxy (not a friend's one) – managing needs the LAN API.
fn own_proxy(state: &AppState) -> Result<ttv_vcp::Client, String> {
    match &state.cfg.read().host {
        HostConfig::Proxy { guest: false, .. } => {}
        HostConfig::Proxy { guest: true, .. } => return Err("This is a friend's voice host – only its owner can share it.".into()),
        _ => return Err("Internet access needs TTV-TTS Proxy on the voice host PC.".into()),
    }
    state.vcp().ok_or_else(|| "No voice host set up yet.".into())
}

async fn call(c: &ttv_vcp::Client, method: ttv_vcp::Method, path: &str, body: Option<Json>) -> Result<Json, String> {
    let mut rb = c.request(method, path).timeout(Duration::from_secs(30));
    if let Some(b) = body {
        rb = rb.json(&b);
    }
    let r = rb.send().await.map_err(|e| e.to_string())?;
    let status = r.status();
    let v: Json = r.json().await.unwrap_or(Json::Null);
    if status.is_success() {
        return Ok(v);
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err("The proxy is too old for internet access – update it on this page first.".into());
    }
    Err(v["error"].as_str().map(str::to_string).unwrap_or_else(|| format!("The proxy answered {status}.")))
}

#[tauri::command]
pub async fn internet_status(state: St<'_>) -> Result<Json, String> {
    call(&own_proxy(&state)?, ttv_vcp::Method::GET, "/ttv/v1/internet", None).await
}

/// `patch`: {enabled?, port?, domain?, token?}
#[tauri::command]
pub async fn internet_set(state: St<'_>, patch: Json) -> Result<Json, String> {
    call(&own_proxy(&state)?, ttv_vcp::Method::POST, "/ttv/v1/internet", Some(patch)).await
}

#[tauri::command]
pub async fn internet_invite(state: St<'_>, name: String, days: u32, chars_per_day: u64, address: Option<String>) -> Result<Json, String> {
    call(&own_proxy(&state)?, ttv_vcp::Method::POST, "/ttv/v1/internet/invites", Some(serde_json::json!({ "name": name, "days": days, "chars_per_day": chars_per_day, "address": address }))).await
}

#[tauri::command]
pub async fn internet_withdraw(state: St<'_>, id: String) -> Result<Json, String> {
    call(&own_proxy(&state)?, ttv_vcp::Method::POST, &format!("/ttv/v1/internet/invites/{id}/withdraw"), None).await
}

#[tauri::command]
pub async fn internet_revoke(state: St<'_>, id: String) -> Result<Json, String> {
    call(&own_proxy(&state)?, ttv_vcp::Method::POST, &format!("/ttv/v1/internet/guests/{id}/revoke"), None).await
}

#[tauri::command]
pub async fn internet_test(state: St<'_>) -> Result<Json, String> {
    call(&own_proxy(&state)?, ttv_vcp::Method::POST, "/ttv/v1/internet/test", None).await
}

/// A friend's invite code: shows where it leads (nothing is changed yet).
#[tauri::command]
pub fn read_invite(code: String) -> Result<Json, String> {
    let c = InviteCode::decode(&code).ok_or("This isn't a TTV-TTS invite code (it starts with ttv1:).")?;
    Ok(serde_json::json!({ "host": c.host, "address": c.address }))
}

/// Joins a friend's voice host with their invite code: the app then speaks
/// with the friend's computed voices (as a guest; the code works once).
#[tauri::command]
pub async fn join_invite(state: St<'_>, code: String) -> Result<AppView, String> {
    let c = InviteCode::decode(&code).ok_or("This isn't a TTV-TTS invite code (it starts with ttv1:).")?;
    let http = crate::host::pinned_http(&c.fingerprint);
    let device = format!("TTV-TTS on {}", crate::config::device_name());
    let r = http
        .post(format!("https://{}/ttv/v1/join", c.address))
        .json(&JoinRequest { secret: c.secret.clone(), device })
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| {
            if e.is_connect() || e.is_timeout() {
                format!("No answer from {}. Your friend's internet access may be off, or their router doesn't forward the port yet.", c.address)
            } else if format!("{e:?}").contains("certificate") || format!("{e:?}").contains("Certificate") {
                "The server at this address isn't the one in the invite (its certificate differs). Ask your friend for a new code.".to_string()
            } else {
                format!("Connecting to {} failed: {e}", c.address)
            }
        })?;
    if !r.status().is_success() {
        let err: ttv_link::ApiError = r.json().await.unwrap_or_default();
        return Err(if err.error.is_empty() { "The invite was refused.".into() } else { err.error });
    }
    let j: JoinResponse = r.json().await.map_err(|e| e.to_string())?;
    let id = format!("guest-{}", &c.fingerprint[..12]);
    state.secrets.set(&crate::secrets::proxy_token(&id), Some(&j.token)).map_err(|e| e.to_string())?;
    {
        let mut cfg = state.cfg.write();
        cfg.host = HostConfig::Proxy { id, name: if j.host.is_empty() { c.host.clone() } else { j.host.clone() }, address: c.address.clone(), fingerprint: c.fingerprint.clone(), mac: None, guest: true };
    }
    state.save_config().map_err(|e| e.to_string())?;
    state.host_updated();
    log::info!("joined {}'s voice host at {} as a guest", j.host, c.address);
    Ok(view(&state))
}
