//! Remote control for Stream Deck, Streamer.bot, Touch Portal and friends:
//! a small HTTP API on this PC, off by default. Every call needs the token
//! (`?token=…` or `Authorization: Bearer …`). Actions also answer GET,
//! because Stream Deck's "Website" action can only open a URL.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// The running server: stop signal and the address it listens on.
static RUNNING: Mutex<Option<(tokio::sync::oneshot::Sender<()>, String)>> = Mutex::new(None);
/// Why the last start failed (port taken …).
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

#[derive(Debug, Clone, Default, Serialize)]
pub struct RemoteStatus {
    /// "127.0.0.1:8130" while it runs.
    pub listening: Option<String>,
    pub error: Option<String>,
}

pub fn status() -> RemoteStatus {
    RemoteStatus { listening: RUNNING.lock().as_ref().map(|(_, a)| a.clone()), error: LAST_ERROR.lock().clone() }
}

/// Starts, restarts or stops the server to match the settings.
pub async fn apply(state: &Arc<AppState>) {
    if let Some((stop, _)) = RUNNING.lock().take() {
        let _ = stop.send(());
    }
    *LAST_ERROR.lock() = None;
    let c = state.cfg.read().remote.clone();
    if !c.enabled {
        return;
    }
    let host = if c.lan { "0.0.0.0" } else { "127.0.0.1" };
    let addr = format!("{host}:{}", c.port);
    // the old server may still hold the port for a moment
    let mut listener = None;
    for _ in 0..10 {
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => {
                listener = Some(l);
                break;
            }
            Err(e) => {
                *LAST_ERROR.lock() = Some(format!("port {} can't be used: {e}", c.port));
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
        }
    }
    let Some(listener) = listener else {
        log::warn!("remote control: {}", LAST_ERROR.lock().clone().unwrap_or_default());
        return;
    };
    *LAST_ERROR.lock() = None;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    *RUNNING.lock() = Some((tx, addr.clone()));
    log::info!("remote control on http://{addr}/");
    let app = router(state.clone());
    tokio::spawn(async move {
        let r = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = rx.await;
        });
        if let Err(e) = r.await {
            log::warn!("remote control stopped: {e}");
        }
    });
}

fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/status", get(status_route))
        .route("/api/skip", any(skip))
        .route("/api/pause", any(pause))
        .route("/api/resume", any(resume))
        .route("/api/toggle-pause", any(toggle_pause))
        .route("/api/clear", any(clear))
        .route("/api/replay", any(replay))
        .route("/api/approve", any(approve))
        .route("/api/reject", any(reject))
        .route("/api/say", any(say))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .route("/", get(help))
        .with_state(state)
}

type S = State<Arc<AppState>>;

/// Constant-time comparison of the token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn auth(State(state): S, req: Request, next: Next) -> Response {
    let want = state.cfg.read().remote.token.clone();
    let from_query = req.uri().query().and_then(|q| q.split('&').find_map(|p| p.strip_prefix("token="))).map(percent_decode);
    let from_header = req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(str::to_string);
    let ok = !want.is_empty() && from_query.or(from_header).is_some_and(|t| same(t.trim(), &want));
    if !ok {
        return (StatusCode::UNAUTHORIZED, Json(Answer { ok: false, message: Some("wrong or missing token".into()) })).into_response();
    }
    next.run(req).await
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

use axum::Json;

#[derive(Serialize)]
struct Answer {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn done(ok: bool, message: Option<String>) -> Json<Answer> {
    Json(Answer { ok, message })
}

async fn help() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "TTV-TTS remote control. Add ?token=<your token> (Settings → Remote control) to every call.\n\
         GET  /api/status\n\
         GET|POST /api/skip  /api/pause  /api/resume  /api/toggle-pause  /api/clear  /api/replay  /api/approve  /api/reject\n\
         GET|POST /api/say?text=Hello&user=StreamDeck   (or POST JSON {\"text\": \"…\", \"user\": \"…\"})\n",
    )
}

#[derive(Serialize)]
struct StatusAnswer {
    connected: bool,
    paused: bool,
    queue: usize,
    pending: usize,
    speaking: Option<String>,
}

async fn status_route(State(state): S) -> Json<StatusAnswer> {
    let s = state.reader.status();
    Json(StatusAnswer { connected: s.connected, paused: s.paused, queue: s.queue.len(), pending: s.pending.len(), speaking: s.current.map(|c| format!("{}: {}", c.display, c.speech)) })
}

async fn skip(State(state): S) -> Json<Answer> {
    state.reader.skip();
    done(true, None)
}
async fn pause(State(state): S) -> Json<Answer> {
    state.reader.set_paused(true);
    done(true, None)
}
async fn resume(State(state): S) -> Json<Answer> {
    state.reader.set_paused(false);
    done(true, None)
}
async fn toggle_pause(State(state): S) -> Json<Answer> {
    let on = !state.reader.is_paused();
    state.reader.set_paused(on);
    done(true, Some(if on { "paused".into() } else { "reading".into() }))
}
async fn clear(State(state): S) -> Json<Answer> {
    state.reader.clear("remote control");
    done(true, None)
}
async fn replay(State(state): S) -> Json<Answer> {
    match state.reader.replay(None) {
        Ok(()) => done(true, None),
        Err(e) => done(false, Some(e)),
    }
}
async fn approve(State(state): S) -> Json<Answer> {
    let ok = state.reader.approve(None);
    done(ok, (!ok).then(|| "nothing waits for approval".into()))
}
async fn reject(State(state): S) -> Json<Answer> {
    let ok = state.reader.reject(None);
    done(ok, (!ok).then(|| "nothing waits for approval".into()))
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Say {
    text: String,
    user: String,
}

async fn say(State(state): S, Query(q): Query<Say>, body: axum::body::Bytes) -> Json<Answer> {
    let mut s = q;
    if s.text.trim().is_empty()
        && let Ok(b) = serde_json::from_slice::<Say>(&body)
    {
        s = b;
    }
    let text = s.text.trim().chars().take(500).collect::<String>();
    if text.is_empty() {
        return done(false, Some("nothing to say (text=…)".into()));
    }
    let user = if s.user.trim().is_empty() { "StreamDeck".to_string() } else { s.user.trim().chars().take(25).collect() };
    let channel = {
        let st = state.reader.store.read();
        st.profiles.iter().filter(|p| st.active.contains(&p.id)).find_map(|p| p.channels.first().cloned())
    };
    let Some(channel) = channel else { return done(false, Some("no profile reads a channel right now".into())) };
    state.reader.test_message(&channel, &user, &text);
    done(true, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_and_query_decoding() {
        assert!(same("abc123", "abc123"));
        assert!(!same("abc123", "abc124") && !same("abc", "abc123"));
        assert_eq!(percent_decode("Hallo%20Welt+%C3%BC"), "Hallo Welt ü");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
