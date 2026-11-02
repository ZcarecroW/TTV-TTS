//! Commands for the proxy's own window.

use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use ttv_link::{LogLine, ProxyStatus};

use crate::config::{Config, KeyMode};
use crate::error::{AppError, AppResult};
use crate::firewall::{self, FirewallStatus};
use crate::secrets::{self, ACCESS_KEY, VCP_KEY};
use crate::state::{AppState, SystemWatch};

type St<'a> = State<'a, Arc<AppState>>;

/// Holds the window's subscription to live system stats.
#[derive(Default)]
pub struct WindowWatch(pub Mutex<Option<SystemWatch>>);

#[derive(Serialize)]
pub struct Overview {
    pub status: ProxyStatus,
    pub config: Config,
    pub has_vcp_key: bool,
    pub fingerprint_short: String,
    pub data_dir: String,
    pub log_path: Option<String>,
    pub listen_error: Option<String>,
}

fn overview(state: &AppState) -> Overview {
    Overview {
        status: state.status(),
        config: state.cfg.read().clone(),
        has_vcp_key: state.secrets.get(VCP_KEY).is_some(),
        fingerprint_short: ttv_link::pin::short(&state.tls.read().fingerprint),
        data_dir: state.data_dir.to_string_lossy().into_owned(),
        log_path: crate::logbuf::get().path().map(|p| p.to_string_lossy().into_owned()),
        listen_error: state.listen_error.read().clone(),
    }
}

#[tauri::command]
pub fn get_overview(state: St) -> Overview {
    overview(&state)
}

/// Settings the window may change; missing fields stay as they are.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct SettingsPatch {
    name: Option<String>,
    port: Option<u16>,
    discovery: Option<bool>,
    vcp_url: Option<String>,
    key_mode: Option<KeyMode>,
    vcp_exe: Option<String>,
    start_vcp: Option<bool>,
    keep_vcp_running: Option<bool>,
    restart_on_hang: Option<bool>,
    hang_restart_secs: Option<u32>,
    restart_when_slow: Option<bool>,
    http_port: Option<u16>,
    autostart: Option<bool>,
    update_channel: Option<String>,
    auto_update: Option<bool>,
    language: Option<String>,
    theme: Option<String>,
    setup_done: Option<bool>,
    obs_port: Option<u16>,
}

#[tauri::command]
pub fn update_settings(app: AppHandle, state: St, patch: SettingsPatch) -> AppResult<Overview> {
    let (rebind, vcp_changed, autostart, lang_changed) = {
        let mut c = state.cfg.write();
        let old = c.clone();
        if let Some(v) = patch.name {
            c.name = v.trim().chars().take(60).collect();
        }
        if let Some(v) = patch.port {
            if v < 1024 {
                return Err(AppError::msg("Please use a port from 1024 to 65535."));
            }
            c.port = v;
        }
        if let Some(v) = patch.discovery {
            c.discovery = v;
        }
        if let Some(v) = patch.vcp_url {
            let v = v.trim().trim_end_matches('/').to_string();
            if url::Url::parse(&v).map(|u| !matches!(u.scheme(), "http" | "https")).unwrap_or(true) {
                return Err(AppError::msg("The VCP address must look like http://127.0.0.1:8100"));
            }
            c.vcp_url = v;
        }
        if let Some(v) = patch.key_mode {
            if v == KeyMode::Vcp && state.secrets.get(VCP_KEY).is_none() {
                return Err(AppError::msg("VCP has no API key stored in the proxy yet. Enter VCP's key first, or use an own access key."));
            }
            if v == KeyMode::Own && state.secrets.get(ACCESS_KEY).is_none() {
                state.secrets.set(ACCESS_KEY, Some(&secrets::random_key()))?;
            }
            c.key_mode = v;
        }
        if let Some(v) = patch.vcp_exe {
            let v = v.trim().trim_matches('"').to_string();
            c.vcp_exe = (!v.is_empty()).then_some(v);
        }
        if let Some(v) = patch.start_vcp {
            c.start_vcp = v;
        }
        if let Some(v) = patch.keep_vcp_running {
            c.keep_vcp_running = v;
        }
        if let Some(v) = patch.restart_on_hang {
            c.restart_on_hang = v;
        }
        if let Some(v) = patch.hang_restart_secs {
            c.hang_restart_secs = v;
        }
        if let Some(v) = patch.restart_when_slow {
            c.restart_when_slow = v;
        }
        if let Some(v) = patch.http_port {
            if v != 0 && (v < 1024 || v == c.port) {
                return Err(AppError::msg("The plain-HTTP port must be 1024–65535 and differ from the main port."));
            }
            c.http_port = v;
        }
        if let Some(v) = patch.autostart {
            c.autostart = v;
        }
        if let Some(v) = patch.update_channel {
            c.update_channel = v;
        }
        if let Some(v) = patch.auto_update {
            c.auto_update = v;
        }
        if let Some(v) = patch.language {
            c.language = v;
        }
        if let Some(v) = patch.theme {
            c.theme = v;
        }
        if let Some(v) = patch.setup_done {
            c.setup_done = v;
        }
        if let Some(v) = patch.obs_port {
            if v < 1024 || v == c.port || v == c.internet_port {
                return Err(AppError::msg("Please use a free port from 1024 to 65535 (not one the proxy uses already)."));
            }
            c.obs_port = v;
            state.hosted_changed.notify_waiters();
        }
        c.normalize();
        (
            c.port != old.port || c.http_port != old.http_port || c.discovery != old.discovery,
            c.vcp_url != old.vcp_url,
            (c.autostart != old.autostart).then_some(c.autostart),
            c.language != old.language,
        )
    };
    state.save_config()?;
    if rebind {
        state.rebind.notify_waiters();
    }
    if vcp_changed {
        state.refresh_vcp_client();
    }
    if let Some(on) = autostart {
        crate::set_autostart(&app, on);
    }
    if lang_changed {
        let _ = app.remove_tray_by_id("main");
        let _ = crate::shell::setup_tray(&app, &state);
    }
    state.update.write().channel = state.cfg.read().update_channel.clone();
    state.publish();
    Ok(overview(&state))
}

#[derive(Serialize)]
pub struct KeyCheck {
    /// The key was stored.
    pub saved: bool,
    /// VCP confirmed the key (false = VCP was not reachable to check it).
    pub verified: bool,
    pub message: String,
}

/// Stores VCP's API key after checking it with VCP.
#[tauri::command]
pub async fn set_vcp_key(state: St<'_>, key: String) -> AppResult<KeyCheck> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(AppError::msg("Please paste VCP's API key."));
    }
    let url = state.cfg.read().vcp_url.clone();
    let test = ttv_vcp::Client::with_http(state.vcp().http().clone(), &url, Some(key.clone()));
    let check = match test.check_key().await {
        Ok(()) => KeyCheck { saved: true, verified: true, message: "VCP accepted the key.".into() },
        Err(ttv_vcp::Error::Unauthorized) => return Err(AppError::msg("VCP rejected this key. Copy it again from VCP's settings (HTTP API).")),
        Err(e) => KeyCheck { saved: true, verified: false, message: format!("Saved, but not checked: {e}") },
    };
    state.secrets.set(VCP_KEY, Some(&key))?;
    state.refresh_vcp_client();
    state.publish();
    Ok(check)
}

#[tauri::command]
pub fn clear_vcp_key(state: St) -> AppResult<Overview> {
    state.secrets.set(VCP_KEY, None)?;
    {
        let mut c = state.cfg.write();
        if c.key_mode == KeyMode::Vcp {
            // Apps would otherwise need no key at all.
            if state.secrets.get(ACCESS_KEY).is_none() {
                state.secrets.set(ACCESS_KEY, Some(&secrets::random_key()))?;
            }
            c.key_mode = KeyMode::Own;
        }
    }
    state.save_config()?;
    state.refresh_vcp_client();
    state.publish();
    Ok(overview(&state))
}

/// Gives VCP a new random API key (VCP must be running) and stores it.
#[tauri::command]
pub async fn create_vcp_key(state: St<'_>) -> AppResult<String> {
    let key = secrets::random_key();
    state.vcp().set_api_key(Some(&key)).await?;
    state.secrets.set(VCP_KEY, Some(&key))?;
    state.refresh_vcp_client();
    log::info!("set a new API key in Voice Creator Pro");
    state.publish();
    Ok(key)
}

/// Takes VCP's key over from VCP's own settings file.
#[tauri::command]
pub async fn detect_vcp_key(state: St<'_>) -> Result<KeyCheck, String> {
    let st = state.inner().clone();
    let key = crate::vcp_key::detect(&st).await?;
    st.secrets.set(VCP_KEY, Some(&key)).map_err(|e| e.to_string())?;
    st.refresh_vcp_client();
    st.publish();
    Ok(KeyCheck { saved: true, verified: true, message: "Found VCP's key in its settings and stored it.".into() })
}

/// The key apps use with the current key mode.
#[tauri::command]
pub fn get_access_key(state: St) -> Option<String> {
    state.access_key()
}

#[tauri::command]
pub fn new_access_key(state: St) -> AppResult<String> {
    let key = secrets::random_key();
    state.secrets.set(ACCESS_KEY, Some(&key))?;
    state.cfg.write().key_mode = KeyMode::Own;
    state.save_config()?;
    state.publish();
    log::info!("created a new access key - apps that use the old one must be updated");
    Ok(key)
}

#[tauri::command]
pub fn set_access_key(state: St, key: String) -> AppResult<()> {
    let key = key.trim();
    if key.len() < 12 {
        return Err(AppError::msg("Please use at least 12 characters."));
    }
    state.secrets.set(ACCESS_KEY, Some(key))?;
    state.cfg.write().key_mode = KeyMode::Own;
    state.save_config()?;
    state.publish();
    Ok(())
}

#[derive(Serialize)]
pub struct PairingView {
    pub code: String,
    pub secs_left: u64,
}

#[tauri::command]
pub fn open_pairing(state: St) -> PairingView {
    let code = state.access.open_pairing();
    state.publish();
    PairingView { code, secs_left: 300 }
}

#[tauri::command]
pub fn get_pairing(state: St) -> Option<PairingView> {
    state.access.pairing().map(|(code, secs_left)| PairingView { code, secs_left })
}

#[tauri::command]
pub fn close_pairing(state: St) {
    state.access.close_pairing();
    state.publish();
}

#[tauri::command]
pub fn revoke_device(state: St, id: String) {
    if state.access.revoke(&id) {
        state.publish();
    }
}

#[tauri::command]
pub async fn vcp_action(state: St<'_>, action: String) -> Result<String, String> {
    let st = state.inner().clone();
    match action.as_str() {
        "start" => crate::vcp_watch::start(&st).await,
        "stop" => crate::vcp_watch::stop(&st).await,
        "restart" => crate::vcp_watch::restart(&st).await,
        _ => Err("unknown action".into()),
    }
}

#[tauri::command]
pub async fn firewall_check() -> FirewallStatus {
    firewall::check().await
}

#[tauri::command]
pub async fn firewall_fix(all_networks: bool) -> Result<FirewallStatus, String> {
    firewall::fix(all_networks).await?;
    log::info!("firewall rule added ({})", if all_networks { "all networks" } else { "private networks" });
    Ok(firewall::check().await)
}

#[tauri::command]
pub fn get_logs() -> Vec<LogLine> {
    crate::logbuf::get().lines()
}

#[tauri::command]
pub fn watch_system(state: St, watch: State<WindowWatch>, on: bool) {
    let mut g = watch.0.lock();
    if on && g.is_none() {
        *g = Some(SystemWatch::new(state.inner().clone()));
    } else if !on {
        *g = None;
    }
}

#[tauri::command]
pub async fn check_update(app: AppHandle, state: St<'_>) -> AppResult<ttv_link::UpdateState> {
    crate::updater::check(&app, &state).await;
    Ok(state.update.read().clone())
}

#[tauri::command]
pub async fn install_update(app: AppHandle, state: St<'_>) -> AppResult<()> {
    crate::updater::install(&app, &state).await;
    Ok(())
}

/// New TLS certificate: every paired app has to pair again.
#[tauri::command]
pub fn reset_certificate(state: St) -> AppResult<Overview> {
    let name = state.cfg.read().display_name();
    let tls = crate::tls::recreate(&state.data_dir, &name)?;
    *state.tls.write() = Arc::new(tls);
    state.rebind.notify_waiters();
    state.publish();
    Ok(overview(&state))
}

#[tauri::command]
pub async fn diagnostics(state: St<'_>) -> AppResult<crate::vcp_watch::Diagnostics> {
    Ok(crate::vcp_watch::diagnostics(&state).await)
}

// ── internet access for friends ─────────────────────────────────────────

#[tauri::command]
pub fn internet_status(state: St) -> crate::internet::InternetStatus {
    crate::internet::status(&state)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct InternetPatch {
    pub enabled: Option<bool>,
    pub port: Option<u16>,
    pub domain: Option<String>,
    /// Some("") removes the stored token.
    pub token: Option<String>,
}

/// Switches internet access, sets its port and the DuckDNS name and token.
pub async fn apply_internet(state: &AppState, patch: InternetPatch) -> Result<crate::internet::InternetStatus, String> {
    if let Some(t) = &patch.token {
        let t = t.trim();
        if !t.is_empty() && (t.len() < 30 || !t.chars().all(|c| c.is_ascii_hexdigit() || c == '-')) {
            return Err("That doesn't look like a DuckDNS token (it looks like 1a2b3c4d-…).".into());
        }
        state.secrets.set(crate::internet::DUCKDNS_TOKEN, (!t.is_empty()).then_some(t)).map_err(|e| e.to_string())?;
    }
    {
        let mut c = state.cfg.write();
        if let Some(on) = patch.enabled {
            c.internet = on;
        }
        if let Some(p) = patch.port {
            if p < 1024 || p == c.port {
                return Err(format!("Use a port from 1024 to 65535 other than {} (the LAN port).", c.port));
            }
            c.internet_port = p;
        }
        if let Some(d) = &patch.domain {
            c.duckdns_domain = crate::internet::clean_domain(d);
        }
        c.normalize();
    }
    state.save_config().map_err(|e| e.to_string())?;
    // check DuckDNS right away when name or token changed
    if patch.domain.is_some() || patch.token.is_some() {
        let (domain, token) = (state.cfg.read().duckdns_domain.clone(), state.secrets.get(crate::internet::DUCKDNS_TOKEN));
        if let (false, Some(token)) = (domain.is_empty(), token) {
            let r = crate::internet::update_duckdns(&domain, &token).await;
            let mut l = state.internet.write();
            l.duckdns_at = Some(chrono::Utc::now().timestamp());
            match r {
                Ok(ip) => {
                    l.duckdns_result = Some("OK".into());
                    if ip.is_some() {
                        l.public_ip = ip;
                    }
                }
                Err(e) => l.duckdns_result = Some(e),
            }
        }
    }
    state.internet_rebind.notify_waiters();
    state.publish_internet();
    Ok(crate::internet::status(state))
}

#[tauri::command]
pub async fn set_internet(state: St<'_>, patch: InternetPatch) -> Result<crate::internet::InternetStatus, String> {
    apply_internet(&state, patch).await
}

#[derive(Debug, Serialize)]
pub struct NewInvite {
    pub invite: crate::internet::InviteView,
    /// The code to send to the friend (shown once).
    pub code: String,
}

#[tauri::command]
pub fn create_invite(state: St, name: String, days: u32, chars_per_day: u64, address: Option<String>) -> Result<NewInvite, String> {
    new_invite(&state, &name, days, chars_per_day, address.as_deref())
}

pub fn new_invite(state: &AppState, name: &str, days: u32, chars_per_day: u64, address: Option<&str>) -> Result<NewInvite, String> {
    if name.trim().is_empty() {
        return Err("Give the invite a name (who it's for).".into());
    }
    let (invite, secret) = state.guests.invite(name, days, chars_per_day);
    let code = crate::internet::invite_code(state, &secret, address);
    if code.is_err() {
        state.guests.withdraw(&invite.id);
    }
    state.publish_internet();
    Ok(NewInvite { invite, code: code? })
}

#[tauri::command]
pub fn withdraw_invite(state: St, id: String) {
    state.guests.withdraw(&id);
    state.publish_internet();
}

#[tauri::command]
pub fn revoke_guest(state: St, id: String) {
    state.guests.revoke(&id);
    state.publish_internet();
}

#[tauri::command]
pub fn set_guest_limit(state: St, id: String, chars_per_day: u64) {
    state.guests.set_limit(&id, chars_per_day);
    state.publish_internet();
}

/// Can the internet reach the internet port? (asks ifconfig.co to connect)
#[tauri::command]
pub async fn test_internet(state: St<'_>) -> Result<bool, String> {
    crate::internet::test(state.inner()).await
}

// ── OBS links read here ─────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct ObsLinks {
    pub port: u16,
    pub error: Option<String>,
    pub links: Vec<crate::hosted::HostedView>,
}

/// The profiles this proxy reads for OBS (handed over by the apps).
#[tauri::command]
pub fn obs_links(state: St) -> ObsLinks {
    ObsLinks { port: state.cfg.read().obs_port, error: state.obs_error.read().clone(), links: state.hosted.list(&state) }
}

/// Stops reading a profile here; its link stops working.
#[tauri::command]
pub fn obs_remove(state: St, id: String) -> ObsLinks {
    state.hosted.remove(&id);
    state.hosted_changed.notify_waiters();
    obs_links(state)
}
