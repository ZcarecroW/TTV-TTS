//! Keeps the voice host's status up to date. With a proxy: one WebSocket to
//! its event stream (status changes are pushed; live system stats only while
//! the window asks for them). With VCP directly: a cheap poll every 5 s.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tauri::Emitter;
use tokio_tungstenite::tungstenite::Message;
use ttv_link::{Event, VcpState};

use crate::config::HostConfig;
use crate::state::{AppState, HostStatus};

pub async fn run(state: Arc<AppState>) {
    loop {
        let host = state.cfg.read().host.clone();
        match host {
            HostConfig::None => {
                state.set_host_status(HostStatus { kind: "none".into(), ..Default::default() });
                state.host_changed.notified().await;
            }
            HostConfig::Proxy { id, address, fingerprint, .. } => {
                let token = state.secrets.get(&crate::secrets::proxy_token(&id)).unwrap_or_default();
                let watch = state.watching_system();
                let reason = proxy_session(&state, &address, &fingerprint, &token, watch).await;
                let base = HostStatus { kind: "proxy".into(), connected: false, error: reason.clone(), ..state.host.read().clone() };
                if let Some(r) = reason {
                    log::debug!("proxy events: {r}");
                    state.set_host_status(HostStatus { connected: false, error: Some(r), ..base });
                    tokio::select! {
                        _ = state.host_changed.notified() => {}
                        _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                    }
                }
            }
            HostConfig::Direct { .. } => {
                direct_poll(&state).await;
                tokio::select! {
                    _ = state.host_changed.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
            }
        }
    }
}

/// Runs until the connection drops, the host or the watch flag changes.
/// Returns an error text for a dropped connection, None for a planned restart.
async fn proxy_session(state: &Arc<AppState>, address: &str, fingerprint: &str, token: &str, watch: bool) -> Option<String> {
    let url = format!("wss://{address}/ttv/v1/events?system={}&api_key={}", u8::from(watch), url_encode(token));
    let connector = tokio_tungstenite::Connector::Rustls(crate::host::pinned_tls(fingerprint));
    let conn = tokio::time::timeout(Duration::from_secs(6), tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector))).await;
    let (mut ws, _) = match conn {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => return Some(describe(&e)),
        Err(_) => return Some(format!("No answer from the proxy at {address}.")),
    };
    // Changes may have been missed while disconnected.
    let mut fresh = true;
    loop {
        tokio::select! {
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(t))) => match serde_json::from_str::<Event>(&t) {
                    Ok(Event::Status(s)) => {
                        if std::mem::take(&mut fresh) {
                            let st = state.clone();
                            tokio::spawn(async move {
                                if let Err(e) = st.library.refresh(&st).await {
                                    log::warn!("library: {e}");
                                }
                            });
                        }
                        let vcp = s.vcp.clone();
                        state.set_host_status(HostStatus { kind: "proxy".into(), connected: true, error: None, vcp: Some(vcp), proxy: Some(*s) });
                    }
                    Ok(Event::System(sys)) => {
                        if let Some(app) = state.app() {
                            let _ = app.emit("app://host-system", &sys);
                        }
                    }
                    Ok(Event::Job(j)) => crate::library::VoiceLibrary::on_job(state, &j),
                    Ok(Event::Library(c)) => state.library.on_change(state, c),
                    _ => {}
                },
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_) | Message::Frame(_))) => {}
                Some(Ok(Message::Close(_))) | None => return Some("The proxy closed the connection.".into()),
                Some(Err(e)) => return Some(describe(&e)),
            },
            _ = state.host_changed.notified() => return None,
            _ = tokio::time::sleep(Duration::from_millis(400)) => {
                if state.watching_system() != watch {
                    return None;
                }
            }
        }
    }
}

fn describe(e: &tokio_tungstenite::tungstenite::Error) -> String {
    use tokio_tungstenite::tungstenite::Error as E;
    match e {
        E::Http(r) if r.status().as_u16() == 401 => "The proxy doesn't accept this device any more - pair again.".into(),
        E::Tls(t) => format!("Secure connection failed: {t}. If the proxy got a new certificate, pair again."),
        E::Io(io) => format!("No connection to the proxy: {io}"),
        other => other.to_string(),
    }
}

fn url_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

async fn direct_poll(state: &Arc<AppState>) {
    let Some(vcp) = state.vcp() else { return };
    let mut v = VcpState { url: vcp.base().to_string(), ..Default::default() };
    let connected = match vcp.health().await {
        Ok(()) => {
            v.reachable = true;
            v.running = true;
            v.version = vcp.version().await.ok();
            match vcp.generation_queue().await {
                Ok(q) => {
                    v.key_ok = Some(true);
                    v.queue_size = q.queue_size;
                    v.generating = q.is_generating;
                    if let Ok(m) = vcp.model_status().await {
                        v.model_loaded = m.loaded;
                        v.model_kind = m.kind;
                        v.model_name = m.name;
                    }
                }
                Err(ttv_vcp::Error::Unauthorized) => {
                    v.key_ok = Some(false);
                    v.error = Some("VCP rejected the API key.".into());
                }
                Err(e) => v.error = Some(e.to_string()),
            }
            true
        }
        Err(e) => {
            v.error = Some(e.to_string());
            false
        }
    };
    let error = v.error.clone();
    state.set_host_status(HostStatus { kind: "direct".into(), connected, error, proxy: None, vcp: Some(v) });
    if !state.library.loaded() {
        let _ = state.library.refresh(state).await;
    }
}
