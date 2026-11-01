//! The proxy's own API under `/ttv/v1`. `info` and `pair` are public, the
//! rest needs the access key or a device token (checked in `server::auth`).

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use ttv_link::{Ack, Event, PairRequest, PairResponse};

use crate::access::PairError;
use crate::server::api_error;
use crate::state::{AppState, SystemWatch};

type S = State<Arc<AppState>>;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/info", get(info))
        .route("/pair", post(pair))
        .route("/status", get(status))
        .route("/system", get(system))
        .route("/logs", get(logs))
        .route("/events", get(events))
        .route("/vcp/start", post(vcp_start))
        .route("/vcp/stop", post(vcp_stop))
        .route("/vcp/restart", post(vcp_restart))
        .route("/update/check", post(update_check))
        .route("/update/install", post(update_install))
        .route("/diagnostics", get(diagnostics))
        .route("/dev/pairing", post(dev_pairing))
        .merge(engine_routes())
        .fallback(|| async { api_error(StatusCode::NOT_FOUND, "not_found", "Unknown TTV-TTS Proxy endpoint.") })
}

async fn info(State(state): S) -> Json<ttv_link::ProxyInfo> {
    Json(state.info())
}

async fn pair(State(state): S, ConnectInfo(addr): ConnectInfo<SocketAddr>, Json(req): Json<PairRequest>) -> Response {
    match state.access.pair(&req.code, &req.device_id, &req.device_name, addr.ip()) {
        Ok(token) => {
            state.publish();
            if let Some(app) = state.app() {
                crate::shell::paired(app, &req.device_name);
            }
            Json(PairResponse { token, proxy: state.info() }).into_response()
        }
        Err(PairError::Closed) => api_error(
            StatusCode::FORBIDDEN,
            "pairing_closed",
            "No pairing code is open. Click \"Pair a device\" in TTV-TTS Proxy first.",
        ),
        Err(PairError::BadCode) => api_error(StatusCode::FORBIDDEN, "bad_code", "Wrong pairing code."),
        Err(PairError::RateLimited) => api_error(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "Too many attempts - try again in a minute."),
    }
}

async fn status(State(state): S) -> Json<ttv_link::ProxyStatus> {
    Json(state.status())
}

async fn system(State(state): S) -> Response {
    // A fresh sample needs two CPU readings; the monitor provides them while
    // someone watches. Answer with the latest one, or take one now.
    let latest = state.system.read().clone();
    match latest {
        Some(s) => Json(s).into_response(),
        None => Json(crate::sysmon::sample_once().await).into_response(),
    }
}

async fn logs() -> Json<Vec<ttv_link::LogLine>> {
    Json(crate::logbuf::get().lines())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EventsQuery {
    /// Also send live system stats every 2 s.
    system: u8,
    /// Also send log lines.
    logs: u8,
}

async fn events(State(state): S, Query(q): Query<EventsQuery>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| event_stream(socket, state, q))
}

async fn event_stream(mut socket: WebSocket, state: Arc<AppState>, q: EventsQuery) {
    let _watch = (q.system == 1).then(|| SystemWatch::new(state.clone()));
    let mut rx = state.events.subscribe();
    let mut logs = (q.logs == 1).then(|| crate::logbuf::get().subscribe());
    if send(&mut socket, &Event::Status(Box::new(state.status()))).await.is_err() {
        return;
    }
    loop {
        let log_next = async {
            match logs.as_mut() {
                Some(r) => r.recv().await.ok(),
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(Event::System(s)) if q.system != 1 => drop(s),
                Ok(ev) => if send(&mut socket, &ev).await.is_err() { return },
                // A slow client missed events: send the current status instead.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if send(&mut socket, &Event::Status(Box::new(state.status()))).await.is_err() { return }
                }
                Err(_) => return,
            },
            line = log_next => if let Some(l) = line && send(&mut socket, &Event::Log(l)).await.is_err() { return },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            },
        }
    }
}

async fn send(socket: &mut WebSocket, ev: &Event) -> Result<(), axum::Error> {
    let text = serde_json::to_string(ev).unwrap_or_default();
    socket.send(Message::Text(text.into())).await
}

fn ack(r: Result<String, String>) -> Response {
    match r {
        Ok(m) => Json(Ack { ok: true, message: Some(m) }).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(Ack { ok: false, message: Some(e) })).into_response(),
    }
}

async fn vcp_start(State(state): S) -> Response {
    ack(crate::vcp_watch::start(&state).await)
}

async fn vcp_stop(State(state): S) -> Response {
    ack(crate::vcp_watch::stop(&state).await)
}

async fn vcp_restart(State(state): S) -> Response {
    ack(crate::vcp_watch::restart(&state).await)
}

async fn update_check(State(state): S) -> Response {
    let Some(app) = state.app().cloned() else { return api_error(StatusCode::SERVICE_UNAVAILABLE, "internal", "not ready") };
    crate::updater::check(&app, &state).await;
    Json(state.update.read().clone()).into_response()
}

async fn update_install(State(state): S) -> Response {
    let Some(app) = state.app().cloned() else { return api_error(StatusCode::SERVICE_UNAVAILABLE, "internal", "not ready") };
    // Installing ends this process; answer first, then start.
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        crate::updater::install(&app, &state).await;
    });
    Json(Ack { ok: true, message: Some("update started".into()) }).into_response()
}

/// Debug builds only (tests): opens pairing and returns the code.
async fn dev_pairing(State(state): S) -> Response {
    if !cfg!(debug_assertions) {
        return api_error(StatusCode::NOT_FOUND, "not_found", "Unknown TTV-TTS Proxy endpoint.");
    }
    let code = state.access.open_pairing();
    state.publish();
    Json(serde_json::json!({ "code": code })).into_response()
}

async fn diagnostics(State(state): S) -> Response {
    Json(crate::vcp_watch::diagnostics(&state).await).into_response()
}

// ── voice computation (engine) ──────────────────────────────────────────

pub fn engine_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/engine", get(engine_status))
        .route("/engine/benchmark", post(engine_benchmark))
        .route("/engine/estimate", get(engine_estimate))
        .route("/library", get(library))
        .route("/library/delete", post(library_delete))
        .route("/library/adopt", post(library_adopt))
        .route("/library/verify", post(library_verify))
        .route("/library/sample/{key}", get(library_sample))
        .route("/jobs", post(job_submit))
        .route("/jobs/{id}/{action}", post(job_control))
}

async fn engine_status(State(state): S) -> Json<ttv_link::EngineStatus> {
    Json(state.engine.status())
}

async fn engine_benchmark(State(state): S) -> Response {
    match state.engine.benchmark().await {
        Ok(d) => Json(serde_json::json!({ "design_secs": d })).into_response(),
        Err(e) => api_error(StatusCode::BAD_GATEWAY, "vcp_unreachable", e),
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EstimateQuery {
    voices: u32,
    takes: u8,
    check: bool,
}

async fn engine_estimate(State(state): S, Query(q): Query<EstimateQuery>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "secs": state.engine.estimate(q.voices, q.takes.max(1), q.check) }))
}

async fn library(State(state): S) -> Json<ttv_link::LibrarySnapshot> {
    Json(state.engine.library())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Keys {
    keys: Vec<String>,
}

async fn library_delete(State(state): S, Json(k): Json<Keys>) -> Response {
    match state.engine.delete(&k.keys).await {
        Ok(n) => Json(serde_json::json!({ "deleted": n })).into_response(),
        Err(e) => api_error(StatusCode::BAD_GATEWAY, "vcp_unreachable", e),
    }
}

async fn library_adopt(State(state): S, Json(entries): Json<Vec<ttv_link::LibraryEntry>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "adopted": state.engine.adopt(entries) }))
}

async fn library_verify(State(state): S) -> Response {
    match state.engine.verify().await {
        Ok(report) => Json(report).into_response(),
        Err(e) => api_error(StatusCode::BAD_GATEWAY, "vcp_unreachable", e),
    }
}

async fn library_sample(State(state): S, axum::extract::Path(key): axum::extract::Path<String>) -> Response {
    match state.engine.sample(&key) {
        Some(bytes) => ([(axum::http::header::CONTENT_TYPE, "audio/flac")], bytes).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "not_found", "No backup for this voice."),
    }
}

async fn job_submit(State(state): S, Json(req): Json<ttv_link::JobRequest>) -> Response {
    match state.engine.submit(req) {
        Ok(v) => Json(v).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, "bad_request", e),
    }
}

async fn job_control(State(state): S, axum::extract::Path((id, action)): axum::extract::Path<(String, String)>) -> Response {
    match state.engine.control(&id, &action) {
        Ok(()) => Json(Ack { ok: true, message: None }).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, "bad_request", e),
    }
}
