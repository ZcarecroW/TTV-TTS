//! Twitch login (optional, settings) and the EventSub connection it enables.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::Emitter;
use tauri_plugin_opener::OpenerExt;

use crate::state::AppState;
use crate::twitch::account::{self, AppTokens, PendingLogin, Poll, TokenSet, TwitchStatus};
use crate::twitch::eventsub::{self, Live};

type St<'a> = tauri::State<'a, Arc<AppState>>;

/// The streamer's own client ID, else the one built into this release.
pub fn client_id(state: &AppState) -> Option<String> {
    let own = state.cfg.read().twitch_client_id.trim().to_string();
    if !own.is_empty() {
        return Some(own);
    }
    crate::twitch::account::built_in_client_id()
}

fn status(state: &AppState) -> TwitchStatus {
    let mut s = state.twitch.status.read().clone();
    s.available = client_id(state).is_some();
    s.own_client_id = !state.cfg.read().twitch_client_id.trim().is_empty();
    s.login = state.twitch.tokens(&state.secrets).map(|t| t.login);
    s
}

fn publish(state: &AppState) {
    if let Some(app) = state.app() {
        let _ = app.emit("app://twitch", status(state));
    }
}

/// (Re)starts EventSub when logged in, and checks the token hourly.
pub fn start(state: &Arc<AppState>) {
    stop(state);
    let (Some(id), Some(t), Some(tx)) = (client_id(state), state.twitch.tokens(&state.secrets), state.reader.inbox()) else { return };
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    *state.twitch.stop.lock() = Some(stop_tx);
    let cfg = eventsub::Config { http: state.twitch.http.clone(), urls: state.twitch.urls.clone(), client_id: id.clone(), user_id: t.user_id.clone(), tokens: Arc::new(AppTokens(state.clone())) };
    let st = state.clone();
    let on_live: eventsub::OnLive = Arc::new(move |l: Live| {
        {
            let mut s = st.twitch.status.write();
            s.connected = l.connected;
            s.subscribed = l.subscribed;
            s.error = l.error;
        }
        publish(&st);
    });
    log::info!("Twitch: events for #{} (logged in)", t.login);
    tauri::async_runtime::spawn(eventsub::run(cfg, tx, stop_rx.clone(), on_live));
    let st = state.clone();
    tauri::async_runtime::spawn(async move {
        let mut stop = stop_rx;
        loop {
            check_token(&st, &id).await;
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(3600)) => {}
                _ = stop.changed() => return,
            }
        }
    });
}

/// Twitch's rule: validate tokens at start and hourly. An invalid one is
/// refreshed; if that fails too, the login is gone.
async fn check_token(state: &Arc<AppState>, client_id: &str) {
    let Some(t) = state.twitch.tokens(&state.secrets) else { return };
    match account::validate(&state.twitch.http, &state.twitch.urls, &t.access_token).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            if let Err(e) = state.twitch.access_token(&state.secrets, client_id, Some(&t.access_token)).await {
                log::warn!("Twitch: {e}");
                stop(state);
                state.twitch.status.write().error = Some(e);
                publish(state);
            }
        }
        // offline for a moment: EventSub reconnects by itself
        Err(e) => log::info!("Twitch: token check skipped ({e})"),
    }
}

pub fn stop(state: &AppState) {
    if let Some(tx) = state.twitch.stop.lock().take() {
        let _ = tx.send(true);
    }
    let mut s = state.twitch.status.write();
    s.connected = false;
    s.subscribed.clear();
}

#[tauri::command]
pub fn twitch_status(state: St) -> TwitchStatus {
    status(&state)
}

/// Starts a login: Twitch's page opens with the code filled in; the app
/// waits until the streamer confirms it there.
#[tauri::command]
pub async fn twitch_login(app: tauri::AppHandle, state: St<'_>) -> Result<TwitchStatus, String> {
    let id = client_id(&state).ok_or("This build has no Twitch client ID: enter your own in the settings.")?;
    let dc = account::start_device_flow(&state.twitch.http, &state.twitch.urls, &id).await.map_err(|e| e.to_string())?;
    let gen_ = state.twitch.login_gen.fetch_add(1, Ordering::Relaxed) + 1;
    let expires_at = chrono::Utc::now().timestamp() + dc.expires_in as i64;
    {
        let mut s = state.twitch.status.write();
        s.pending = Some(PendingLogin { user_code: dc.user_code.clone(), verification_uri: dc.verification_uri.clone(), expires_at });
        s.error = None;
    }
    // only Twitch's own page (the address comes from Twitch's answer)
    if dc.verification_uri.starts_with("https://www.twitch.tv/") {
        let _ = app.opener().open_url(&dc.verification_uri, None::<&str>);
    }
    let st = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = dc.interval.max(1);
        let outcome: Result<TokenSet, String> = loop {
            tokio::time::sleep(Duration::from_secs(interval)).await;
            if st.twitch.login_gen.load(Ordering::Relaxed) != gen_ {
                return;
            }
            if chrono::Utc::now().timestamp() > expires_at {
                break Err("The code expired before it was confirmed. Try again.".into());
            }
            match account::poll_device_token(&st.twitch.http, &st.twitch.urls, &id, &dc.device_code).await {
                Ok(Poll::Pending) => {}
                Ok(Poll::SlowDown) => interval += 5,
                Ok(Poll::Done { access_token, refresh_token, expires_at, scopes }) => {
                    break match account::validate(&st.twitch.http, &st.twitch.urls, &access_token).await {
                        Ok(Some(v)) => Ok(TokenSet { access_token, refresh_token, expires_at, scopes: if scopes.is_empty() { v.scopes } else { scopes }, user_id: v.user_id, login: v.login }),
                        Ok(None) => Err("Twitch didn't accept the new login.".into()),
                        Err(e) => Err(e.to_string()),
                    };
                }
                Err(e) => break Err(e.to_string()),
            }
        };
        st.twitch.status.write().pending = None;
        match outcome.and_then(|t| st.twitch.store(&st.secrets, Some(&t)).map(|_| t)) {
            Ok(t) => {
                log::info!("Twitch: logged in as {}", t.login);
                start(&st);
            }
            Err(e) => st.twitch.status.write().error = Some(e),
        }
        publish(&st);
    });
    Ok(status(&state))
}

#[tauri::command]
pub fn twitch_cancel_login(state: St) -> TwitchStatus {
    state.twitch.login_gen.fetch_add(1, Ordering::Relaxed);
    state.twitch.status.write().pending = None;
    status(&state)
}

/// Logs out: the token is revoked at Twitch and removed here.
#[tauri::command]
pub async fn twitch_logout(state: St<'_>) -> Result<TwitchStatus, String> {
    stop(&state);
    if let (Some(t), Some(id)) = (state.twitch.tokens(&state.secrets), client_id(&state)) {
        account::revoke(&state.twitch.http, &state.twitch.urls, &id, &t.access_token).await;
    }
    state.twitch.store(&state.secrets, None)?;
    state.twitch.status.write().error = None;
    log::info!("Twitch: logged out");
    let s = status(&state);
    publish(&state);
    Ok(s)
}

/// The client ID of the streamer's own Twitch application ("" = the built-in one).
#[tauri::command]
pub async fn set_twitch_client_id(state: St<'_>, id: String) -> Result<TwitchStatus, String> {
    let id = id.trim().to_string();
    if !id.is_empty() && (id.len() < 20 || id.len() > 40 || !id.chars().all(|c| c.is_ascii_alphanumeric())) {
        return Err("That doesn't look like a Twitch client ID (about 30 letters and digits).".into());
    }
    let before = client_id(&state);
    state.cfg.write().twitch_client_id = id;
    state.save_config().map_err(|e| e.to_string())?;
    let changed = client_id(&state) != before;
    // a login belongs to the application it was made with
    if changed && state.twitch.tokens(&state.secrets).is_some() {
        return twitch_logout(state).await;
    }
    let s = status(&state);
    publish(&state);
    Ok(s)
}
