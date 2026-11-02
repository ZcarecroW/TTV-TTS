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
        .route("/library/archive", post(library_archive))
        .route("/library/archived", get(library_archived))
        .route("/library/adopt", post(library_adopt))
        .route("/library/keep", post(library_keep))
        .route("/library/verify", post(library_verify))
        .route("/library/sample/{key}", get(library_sample))
        .route("/takes", get(takes_list))
        .route("/takes/{key}/{n}", get(take_audio))
        .route("/takes/{key}/choose", post(take_choose))
        .route("/takes/{key}/discard", post(take_discard))
        .route("/internet", get(internet_get).post(internet_set))
        .route("/internet/invites", post(invite_create))
        .route("/internet/invites/{id}/withdraw", post(invite_withdraw))
        .route("/internet/guests/{id}/revoke", post(guest_revoke))
        .route("/internet/test", post(internet_test))
        .route("/packs/export", post(pack_export))
        .route("/packs/import", post(pack_import))
        .route("/jobs", post(job_submit))
        .route("/jobs/{id}/{action}", post(job_control))
        .route("/speak", post(crate::speak::speak))
        .route("/plugins", get(plugins_get))
        .route("/plugins/scan", post(plugins_scan))
        .route("/plugins/chains", axum::routing::put(plugins_chains))
        .route("/plugins/edit/{slot}", post(plugins_edit))
        .route("/hosted", get(hosted_list))
        .route("/hosted/{id}", get(hosted_one).put(hosted_put).delete(hosted_delete))
        .route("/hosted/{id}/history", get(hosted_history))
        .route("/hosted/{id}/test", post(hosted_test))
        .route("/hosted/{id}/viewers", get(hosted_viewers))
        .route("/hosted/{id}/viewers/{user}", post(hosted_viewer))
        .route("/hosted/{id}/do/{action}", post(hosted_control))
}

// ── OBS links read here (profiles handed over by the app) ───────────────

type IdPath = axum::extract::Path<String>;

async fn hosted_list(State(state): S) -> Json<Vec<crate::hosted::HostedView>> {
    Json(state.hosted.list(&state))
}

async fn hosted_one(State(state): S, axum::extract::Path(id): IdPath) -> Response {
    match state.hosted.one(&state, &id) {
        Some(v) => Json(v).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "not_found", "unknown OBS link"),
    }
}

async fn hosted_put(State(state): S, axum::extract::Path(id): IdPath, Json(req): Json<crate::hosted::HostedRequest>) -> Response {
    let r = state.hosted.put(&state, &id, req);
    state.hosted_changed.notify_waiters();
    state.certs.wake();
    state.upnp.wake();
    match r {
        Ok(v) => Json(v).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, "bad_request", e),
    }
}

// ── audio plugins run here ──────────────────────────────────────────────

async fn plugins_get(State(state): S) -> Json<ttv_plugins::wire::HostView> {
    // the first look starts the scan (it loads every plugin once)
    if state.plugins.view().installed.is_none() {
        state.plugins.scan();
    }
    Json(state.plugins.view())
}

async fn plugins_scan(State(state): S) -> Json<ttv_plugins::wire::HostView> {
    state.plugins.scan();
    Json(state.plugins.view())
}

async fn plugins_chains(State(state): S, Json(c): Json<ttv_plugins::wire::Chains>) -> Json<ttv_plugins::wire::HostView> {
    state.plugins.set_chains(c).await;
    if state.plugins.view().installed.is_none() {
        state.plugins.scan();
    }
    Json(state.plugins.view())
}

async fn plugins_edit(State(state): S, axum::extract::Path(slot): IdPath) -> Response {
    match state.plugins.edit(&slot).await {
        Ok(()) => Json(Ack { ok: true, message: None }).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, "bad_request", e),
    }
}

async fn hosted_delete(State(state): S, axum::extract::Path(id): IdPath) -> Json<Ack> {
    let ok = state.hosted.remove(&id);
    state.hosted_changed.notify_waiters();
    state.certs.wake();
    state.upnp.wake();
    Json(Ack { ok, message: None })
}

async fn hosted_history(State(state): S, axum::extract::Path(id): IdPath) -> Response {
    match state.hosted.history(&id) {
        Some(h) => Json(h).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "not_found", "unknown OBS link"),
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TestMessage {
    user: String,
    text: String,
}

async fn hosted_test(State(state): S, axum::extract::Path(id): IdPath, Json(m): Json<TestMessage>) -> Response {
    let user = if m.user.trim().is_empty() { "TestUser" } else { m.user.trim() };
    match state.hosted.test(&id, user, &m.text) {
        Ok(()) => Json(Ack { ok: true, message: None }).into_response(),
        Err(e) => api_error(StatusCode::CONFLICT, "bad_request", e),
    }
}

async fn hosted_control(State(state): S, axum::extract::Path((id, action)): axum::extract::Path<(String, String)>) -> Response {
    match state.hosted.control(&id, &action) {
        Ok(()) => Json(Ack { ok: true, message: None }).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, "bad_request", e),
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ViewersQuery {
    q: String,
    limit: Option<usize>,
}

#[derive(serde::Serialize)]
struct ViewersPage {
    viewers: Vec<ttv_reader::reader::ViewerView>,
    total: usize,
}

async fn hosted_viewers(State(state): S, axum::extract::Path(id): IdPath, Query(q): Query<ViewersQuery>) -> Response {
    match state.hosted.viewers(&id, &q.q, q.limit.unwrap_or(300).min(1000)) {
        Some((viewers, total)) => Json(ViewersPage { viewers, total }).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "not_found", "unknown OBS link"),
    }
}

/// One change of a viewer: `{"role": 3}` (null = a new random voice),
/// `{"fx": "radio"}`, `{"forget": true}`, `{"mute": "login", "on": true}`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ViewerPatch {
    role: Option<Option<u32>>,
    fx: Option<Option<String>>,
    plugin: Option<Option<String>>,
    forget: bool,
    mute: Option<String>,
    on: bool,
}

async fn hosted_viewer(State(state): S, axum::extract::Path((id, user)): axum::extract::Path<(String, String)>, Json(p): Json<ViewerPatch>) -> Response {
    use crate::hosted::ViewerChange;
    let change = if p.forget {
        ViewerChange::Forget
    } else if let Some(login) = p.mute {
        ViewerChange::Mute(login, p.on)
    } else if let Some(role) = p.role {
        ViewerChange::Role(role)
    } else if let Some(fx) = p.fx {
        ViewerChange::Fx(fx)
    } else if let Some(plugin) = p.plugin {
        ViewerChange::Plugin(plugin)
    } else {
        return api_error(StatusCode::BAD_REQUEST, "bad_request", "nothing to change");
    };
    match state.hosted.set_viewer(&id, &user, change) {
        Ok(()) => Json(Ack { ok: true, message: None }).into_response(),
        Err(e) => api_error(StatusCode::NOT_FOUND, "not_found", e),
    }
}

// ── internet access (LAN only: these routes don't exist on the internet port)

async fn internet_get(State(state): S) -> Json<crate::internet::InternetStatus> {
    Json(crate::internet::status(&state))
}

async fn internet_set(State(state): S, Json(p): Json<crate::commands::InternetPatch>) -> Response {
    match crate::commands::apply_internet(&state, p).await {
        Ok(s) => Json(s).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, "bad_request", e),
    }
}

#[derive(Deserialize)]
struct InviteReq {
    name: String,
    #[serde(default = "seven")]
    days: u32,
    #[serde(default = "quota")]
    chars_per_day: u64,
    /// Instead of the DuckDNS name / public address (e.g. Tailscale).
    #[serde(default)]
    address: Option<String>,
}

fn seven() -> u32 {
    7
}

fn quota() -> u64 {
    30_000
}

async fn invite_create(State(state): S, Json(r): Json<InviteReq>) -> Response {
    match crate::commands::new_invite(&state, &r.name, r.days, r.chars_per_day, r.address.as_deref()) {
        Ok(i) => Json(i).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, "bad_request", e),
    }
}

async fn invite_withdraw(State(state): S, axum::extract::Path(id): axum::extract::Path<String>) -> Json<Ack> {
    state.guests.withdraw(&id);
    state.publish_internet();
    Json(Ack { ok: true, message: None })
}

async fn guest_revoke(State(state): S, axum::extract::Path(id): axum::extract::Path<String>) -> Json<Ack> {
    state.guests.revoke(&id);
    state.publish_internet();
    Json(Ack { ok: true, message: None })
}

async fn internet_test(State(state): S) -> Response {
    match crate::internet::test(&state).await {
        Ok(reachable) => Json(serde_json::json!({ "reachable": reachable })).into_response(),
        Err(e) => api_error(StatusCode::BAD_GATEWAY, "test_failed", e),
    }
}

/// Largest pack accepted (all 20 presets × 240 voices × 10 languages ≈ 10 GB
/// would be absurd to upload in one go; a full preset in 10 languages ≈ 1 GB).
const MAX_PACK: u64 = 4 << 30;

/// Streams a file in 256 KB chunks.
fn file_stream(file: tokio::fs::File) -> impl futures_util::Stream<Item = std::io::Result<Vec<u8>>> {
    futures_util::stream::unfold(file, |mut f| async move {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 256 * 1024];
        match f.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok(buf), f))
            }
            Err(e) => Some((Err(e), f)),
        }
    })
}

/// Builds a voice pack and sends it (application/zip).
async fn pack_export(State(state): S, Json(req): Json<ttv_link::ExportRequest>) -> Response {
    let path = match state.engine.export(req, env!("CARGO_PKG_VERSION")).await {
        Ok(p) => p,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, "export_failed", e),
    };
    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "export_failed", e.to_string()),
    };
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let headers = [(axum::http::header::CONTENT_TYPE, "application/zip".to_string()), (axum::http::header::CONTENT_LENGTH, len.to_string())];
    (headers, axum::body::Body::from_stream(file_stream(file))).into_response()
}

/// Receives a voice pack (the raw file as body), checks it and queues the import.
async fn pack_import(State(state): S, body: axum::body::Body) -> Response {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let dir = state.data_dir.join("engine").join("packs");
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, "import_failed", e.to_string());
    }
    let path = dir.join(format!("incoming-{:08x}.{}", rand::random::<u32>(), ttv_engine::pack::EXTENSION));
    let result: Result<(), String> = async {
        let mut file = tokio::fs::File::create(&path).await.map_err(|e| e.to_string())?;
        let mut stream = body.into_data_stream();
        let mut size = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            size += chunk.len() as u64;
            if size > MAX_PACK {
                return Err("the pack is too large".into());
            }
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        }
        file.sync_all().await.map_err(|e| e.to_string())?;
        Ok(())
    }
    .await;
    let queued = result.and_then(|()| state.engine.submit(ttv_link::JobRequest { kind: ttv_link::JobKind::Import, pack: path.to_string_lossy().into(), ..Default::default() }));
    match queued {
        Ok(view) => {
            if view.total == 0 {
                let _ = tokio::fs::remove_file(&path).await;
            }
            Json(view).into_response()
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&path).await;
            api_error(StatusCode::BAD_REQUEST, "import_failed", e)
        }
    }
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

/// Moves computed voices to the engine's archive (ready-made packs): they
/// leave VCP, their lossless sample and data stay here for exports.
async fn library_archive(State(state): S, Json(k): Json<Keys>) -> Response {
    match state.engine.archive(&k.keys).await {
        Ok(n) => Json(serde_json::json!({ "archived": n })).into_response(),
        Err(e) => api_error(StatusCode::BAD_GATEWAY, "vcp_unreachable", e),
    }
}

async fn library_archived(State(state): S) -> Json<ttv_link::LibrarySnapshot> {
    Json(state.engine.archived())
}

/// A list of entries to adopt, or entries replacing others (re-keying adopted
/// voices; nothing is deleted in VCP).
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Adopt {
    List(Vec<ttv_link::LibraryEntry>),
    Replace {
        entries: Vec<ttv_link::LibraryEntry>,
        #[serde(default)]
        remove: Vec<String>,
    },
}

async fn library_adopt(State(state): S, Json(a): Json<Adopt>) -> Json<serde_json::Value> {
    let n = match a {
        Adopt::List(entries) => state.engine.adopt(entries),
        Adopt::Replace { entries, remove } => state.engine.rekey(entries, &remove),
    };
    Json(serde_json::json!({ "adopted": n }))
}

#[derive(serde::Deserialize)]
struct Keep {
    keys: Vec<String>,
    #[serde(default = "yes")]
    kept: bool,
}

fn yes() -> bool {
    true
}

/// Marks voices as listened to and fine (the quality check stops listing them).
async fn library_keep(State(state): S, Json(k): Json<Keep>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "changed": state.engine.keep(&k.keys, k.kept) }))
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

async fn takes_list(State(state): S) -> Json<Vec<ttv_link::TakeChoice>> {
    Json(state.engine.takes())
}

async fn take_audio(State(state): S, axum::extract::Path((key, n)): axum::extract::Path<(String, u32)>) -> Response {
    match state.engine.take_wav(&key, n) {
        Some(bytes) => ([(axum::http::header::CONTENT_TYPE, "audio/wav")], bytes).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "not_found", "This take doesn't exist."),
    }
}

#[derive(serde::Deserialize)]
struct Choose {
    n: u32,
}

async fn take_choose(State(state): S, axum::extract::Path(key): axum::extract::Path<String>, Json(c): Json<Choose>) -> Response {
    match state.engine.choose_take(&key, c.n).await {
        Ok(e) => Json(e).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, "bad_request", e),
    }
}

async fn take_discard(State(state): S, axum::extract::Path(key): axum::extract::Path<String>) -> Json<Ack> {
    Json(Ack { ok: state.engine.discard_takes(&key), message: None })
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
