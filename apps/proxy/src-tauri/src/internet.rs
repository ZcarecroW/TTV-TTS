//! Internet access: friends use this proxy's voices from elsewhere.
//!
//! Security first:
//! - A separate port with its own, small API (guest API). The LAN API –
//!   pairing, settings, logs, VCP control, jobs, the raw VCP API – is never
//!   reachable from the internet port.
//! - Friends join with a one-time invite code that expires; it carries the
//!   certificate's fingerprint, so their app can't be fooled by another
//!   server. Their token is stored only as a SHA-256 hash and can be revoked.
//! - Guests may only speak with the computed voices of the library (no
//!   uploading audio to clone anyone), with a length limit per message, a
//!   daily character quota and a request rate limit. The host's own chat
//!   always goes first; guests take turns.
//! - Wrong secrets and tokens lock the address out (shared with the LAN
//!   rate limit); connections per address and in total are capped.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use ttv_link::{Event, GuestInfo, InviteCode, JoinRequest, JoinResponse};

use crate::server::{api_error, presented_key};
use crate::state::AppState;

/// Longest text one request may speak.
pub const MAX_TEXT: usize = 600;
/// Requests per guest and minute.
const PER_MINUTE: u32 = 40;
/// Open connections from the internet in total and per address.
const MAX_CONNECTIONS: usize = 64;
const MAX_PER_ADDRESS: usize = 8;
/// A guest waits at most this long for the host's chat to pause.
const HOST_FIRST: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredInvite {
    id: String,
    name: String,
    secret_hash: String,
    created: i64,
    expires: i64,
    chars_per_day: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredGuest {
    id: String,
    name: String,
    device: String,
    token_hash: String,
    joined: i64,
    chars_per_day: u64,
    /// UTC day ("2026-11-01") the counters belong to.
    day: String,
    chars_today: u64,
    total_chars: u64,
    last_seen: Option<i64>,
    last_address: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    invites: Vec<StoredInvite>,
    guests: Vec<StoredGuest>,
}

/// An open invite, for the window.
#[derive(Debug, Clone, Serialize)]
pub struct InviteView {
    pub id: String,
    pub name: String,
    pub created: i64,
    pub expires: i64,
    pub chars_per_day: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum JoinError {
    Unknown,
    Expired,
}

#[derive(Debug, PartialEq, Eq)]
pub enum QuotaError {
    TooLong,
    Daily,
    Rate,
}

pub struct Guests {
    path: PathBuf,
    store: Mutex<Store>,
    saved: Mutex<Instant>,
    /// guest id -> (window start, requests)
    minute: Mutex<HashMap<String, (Instant, u32)>>,
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn view(g: &StoredGuest) -> GuestInfo {
    let today = g.day == self::today();
    GuestInfo {
        id: g.id.clone(),
        name: g.name.clone(),
        chars_today: if today { g.chars_today } else { 0 },
        chars_per_day: g.chars_per_day,
        joined: g.joined,
        last_seen: g.last_seen,
        last_address: g.last_address.clone(),
        total_chars: g.total_chars,
    }
}

impl Guests {
    pub fn load(data_dir: &std::path::Path) -> Self {
        let path = data_dir.join("guests.json");
        let store: Store = crate::storage::read_json(&path).unwrap_or_default();
        Self { path, store: Mutex::new(store), saved: Mutex::new(Instant::now()), minute: Mutex::new(HashMap::new()) }
    }

    fn save(&self, s: &Store) {
        if let Err(e) = crate::storage::write_json(&self.path, s) {
            log::warn!("saving the guest list failed: {e}");
        }
        *self.saved.lock() = Instant::now();
    }

    /// A new invite; returns it and its secret (shown once, stored hashed).
    pub fn invite(&self, name: &str, days: u32, chars_per_day: u64) -> (InviteView, String) {
        let secret = crate::secrets::random_key();
        let now = chrono::Utc::now().timestamp();
        let inv = StoredInvite {
            id: crate::secrets::random_key()[..10].to_ascii_lowercase(),
            name: name.trim().chars().take(40).collect::<String>().trim().to_string(),
            secret_hash: crate::access::hash(&secret),
            created: now,
            expires: now + i64::from(days.clamp(1, 30)) * 86_400,
            chars_per_day: chars_per_day.clamp(1_000, 1_000_000),
        };
        let v = InviteView { id: inv.id.clone(), name: inv.name.clone(), created: inv.created, expires: inv.expires, chars_per_day: inv.chars_per_day };
        let mut s = self.store.lock();
        s.invites.retain(|i| i.expires > now);
        s.invites.push(inv);
        self.save(&s);
        let d = days.clamp(1, 30);
        log::info!("invite for \"{}\" created (valid {d} day{})", v.name, if d == 1 { "" } else { "s" });
        (v, secret)
    }

    pub fn invites(&self) -> Vec<InviteView> {
        let now = chrono::Utc::now().timestamp();
        self.store.lock().invites.iter().filter(|i| i.expires > now).map(|i| InviteView { id: i.id.clone(), name: i.name.clone(), created: i.created, expires: i.expires, chars_per_day: i.chars_per_day }).collect()
    }

    pub fn withdraw(&self, id: &str) -> bool {
        let mut s = self.store.lock();
        let n = s.invites.len();
        s.invites.retain(|i| i.id != id);
        let changed = s.invites.len() != n;
        if changed {
            self.save(&s);
        }
        changed
    }

    /// Redeems an invite (once); returns the guest and their token.
    pub fn join(&self, secret: &str, device: &str, addr: IpAddr) -> Result<(GuestInfo, String), JoinError> {
        let h = crate::access::hash(secret.trim());
        let now = chrono::Utc::now().timestamp();
        let mut s = self.store.lock();
        let i = s.invites.iter().position(|i| bool::from(i.secret_hash.as_bytes().ct_eq(h.as_bytes()))).ok_or(JoinError::Unknown)?;
        let inv = s.invites.remove(i);
        if inv.expires <= now {
            self.save(&s);
            return Err(JoinError::Expired);
        }
        let token = crate::secrets::random_key();
        let g = StoredGuest {
            id: crate::secrets::random_key()[..12].to_ascii_lowercase(),
            name: inv.name.clone(),
            device: device.trim().chars().take(60).collect(),
            token_hash: crate::access::hash(&token),
            joined: now,
            chars_per_day: inv.chars_per_day,
            day: today(),
            chars_today: 0,
            total_chars: 0,
            last_seen: Some(now),
            last_address: Some(addr.to_string()),
        };
        let info = view(&g);
        s.guests.push(g);
        self.save(&s);
        log::info!("\"{}\" joined over the internet from {addr}", inv.name);
        Ok((info, token))
    }

    /// The guest a token belongs to.
    pub fn check(&self, token: &str, addr: IpAddr) -> Option<GuestInfo> {
        if token.is_empty() {
            return None;
        }
        let h = crate::access::hash(token);
        let mut s = self.store.lock();
        let g = s.guests.iter_mut().find(|g| bool::from(g.token_hash.as_bytes().ct_eq(h.as_bytes())))?;
        let a = addr.to_string();
        let moved = g.last_address.as_deref() != Some(a.as_str());
        g.last_seen = Some(chrono::Utc::now().timestamp());
        g.last_address = Some(a);
        let info = view(g);
        if moved || self.saved.lock().elapsed() > Duration::from_secs(60) {
            let snapshot = s.clone();
            drop(s);
            self.save(&snapshot);
        }
        Some(info)
    }

    /// Counts a request of `chars` characters against the guest's limits.
    pub fn charge(&self, id: &str, chars: usize) -> Result<(), QuotaError> {
        if chars > MAX_TEXT {
            return Err(QuotaError::TooLong);
        }
        {
            let mut m = self.minute.lock();
            let e = m.entry(id.to_string()).or_insert((Instant::now(), 0));
            if e.0.elapsed() >= Duration::from_secs(60) {
                *e = (Instant::now(), 0);
            }
            if e.1 >= PER_MINUTE {
                return Err(QuotaError::Rate);
            }
            e.1 += 1;
        }
        let mut s = self.store.lock();
        let Some(g) = s.guests.iter_mut().find(|g| g.id == id) else { return Err(QuotaError::Daily) };
        let day = today();
        if g.day != day {
            g.day = day;
            g.chars_today = 0;
        }
        if g.chars_today + chars as u64 > g.chars_per_day {
            return Err(QuotaError::Daily);
        }
        g.chars_today += chars as u64;
        g.total_chars += chars as u64;
        Ok(())
    }

    pub fn guests(&self) -> Vec<GuestInfo> {
        self.store.lock().guests.iter().map(view).collect()
    }

    pub fn revoke(&self, id: &str) -> bool {
        let mut s = self.store.lock();
        let n = s.guests.len();
        s.guests.retain(|g| g.id != id);
        let changed = s.guests.len() != n;
        if changed {
            self.save(&s);
            log::info!("guest {id} removed");
        }
        changed
    }

    pub fn set_limit(&self, id: &str, chars_per_day: u64) -> bool {
        let mut s = self.store.lock();
        let Some(g) = s.guests.iter_mut().find(|g| g.id == id) else { return false };
        g.chars_per_day = chars_per_day.clamp(1_000, 1_000_000);
        let snapshot = s.clone();
        drop(s);
        self.save(&snapshot);
        true
    }
}

// ── status for the window ───────────────────────────────────────────────

/// Internet access as the window shows it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct InternetStatus {
    pub enabled: bool,
    pub port: u16,
    /// The internet port is open on this PC.
    pub listening: bool,
    pub error: Option<String>,
    pub domain: String,
    pub duckdns_token: bool,
    /// Last DuckDNS update: unix seconds, result ("OK"/"KO"/error), address.
    pub duckdns_at: Option<i64>,
    pub duckdns_result: Option<String>,
    pub public_ip: Option<String>,
    /// Last reachability test from the internet.
    pub test_at: Option<i64>,
    pub reachable: Option<bool>,
    /// Address friends use ("name.duckdns.org:8111" or "ip:port").
    pub address: String,
    pub invites: Vec<InviteView>,
    pub guests: Vec<GuestInfo>,
    /// Open connections from the internet.
    pub connections: usize,
    /// OBS links from the internet use HTTPS.
    pub obs_https: bool,
    /// Their certificate (Let's Encrypt, for the DuckDNS name).
    pub https: ttv_net::CertStatus,
    /// Some OBS link is shared on the internet (the certificate is needed).
    pub obs_online: bool,
    /// Opens the ports in the router itself.
    pub upnp: bool,
    pub upnp_status: ttv_net::upnp::UpnpStatus,
    pub obs_port: u16,
}

#[derive(Debug, Clone, Default)]
pub struct Live {
    pub listening: bool,
    pub error: Option<String>,
    pub duckdns_at: Option<i64>,
    pub duckdns_result: Option<String>,
    pub public_ip: Option<String>,
    pub test_at: Option<i64>,
    pub reachable: Option<bool>,
}

pub fn status(state: &AppState) -> InternetStatus {
    let c = state.cfg.read().clone();
    let live = state.internet.read().clone();
    let address = address(&c, live.public_ip.as_deref());
    InternetStatus {
        enabled: c.internet,
        port: c.internet_port,
        listening: live.listening,
        error: live.error,
        domain: c.duckdns_domain.clone(),
        duckdns_token: state.secrets.get(DUCKDNS_TOKEN).is_some(),
        duckdns_at: live.duckdns_at,
        duckdns_result: live.duckdns_result,
        public_ip: live.public_ip,
        test_at: live.test_at,
        reachable: live.reachable,
        address,
        invites: state.guests.invites(),
        guests: state.guests.guests(),
        connections: INTERNET_CONNECTIONS.load(Ordering::Relaxed),
        obs_https: c.obs_https,
        https: state.certs.status(),
        obs_online: state.hosted.any_online(),
        upnp: c.upnp,
        upnp_status: state.upnp.status(),
        obs_port: c.obs_port,
    }
}

/// The ports to open in the router: friends' port while friends may use
/// this proxy, the OBS port while an OBS link is shared on the internet.
pub fn upnp_want(state: &AppState) -> Vec<(u16, &'static str)> {
    let c = state.cfg.read().clone();
    if !c.upnp {
        return Vec::new();
    }
    let mut v = Vec::new();
    if c.internet {
        v.push((c.internet_port, "friends"));
    }
    if state.hosted.any_online() {
        v.push((c.obs_port, "OBS"));
    }
    v
}

/// The DuckDNS name and token, when both are set.
pub fn duckdns_login(state: &AppState) -> Option<ttv_net::Want> {
    let domain = state.cfg.read().duckdns_domain.clone();
    let token = state.secrets.get(DUCKDNS_TOKEN)?;
    (!domain.is_empty()).then_some(ttv_net::Want { domain, token })
}

/// What the certificate is needed for: OBS links shared on the internet, with HTTPS on.
pub fn https_want(state: &AppState) -> Option<ttv_net::Want> {
    (state.cfg.read().obs_https && state.hosted.any_online()).then(|| duckdns_login(state)).flatten()
}

/// How OBS links from the internet start: (`https://name.duckdns.org:port`,
/// true) with a certificate, the plain address when HTTPS is off or can't be
/// had, `None` while the certificate is being fetched (a link sent now
/// would stop working in a minute).
pub fn obs_internet_base(state: &AppState) -> Option<(String, bool)> {
    let c = state.cfg.read().clone();
    let public = address(&c, state.internet.read().public_ip.as_deref());
    // the address carries the guests' port: OBS links use their own
    let host = public.rsplit_once(':').map(|(h, _)| h.to_string()).unwrap_or(public);
    if host.is_empty() {
        return None;
    }
    let fqdn = (!c.duckdns_domain.is_empty()).then(|| ttv_net::duckdns::fqdn(&c.duckdns_domain));
    match fqdn {
        Some(f) if c.obs_https && state.secrets.get(DUCKDNS_TOKEN).is_some() => {
            if state.certs.valid_for(&f) {
                Some((format!("https://{f}:{}", c.obs_port), true))
            } else if state.certs.status().error.is_some() {
                Some((format!("http://{f}:{}", c.obs_port), false))
            } else {
                None
            }
        }
        _ => Some((format!("http://{host}:{}", c.obs_port), false)),
    }
}

/// From the internet over plain HTTP while HTTPS works: refused.
pub fn needs_https(state: &AppState) -> bool {
    let c = state.cfg.read();
    c.obs_https && !c.duckdns_domain.is_empty() && state.certs.valid_for(&ttv_net::duckdns::fqdn(&c.duckdns_domain))
}

pub const DUCKDNS_TOKEN: &str = "duckdns-token";

/// Where friends reach this proxy: the DuckDNS name if set, else the public IP.
pub fn address(c: &crate::config::Config, public_ip: Option<&str>) -> String {
    let host = if !c.duckdns_domain.trim().is_empty() { format!("{}.duckdns.org", c.duckdns_domain.trim()) } else { public_ip.unwrap_or_default().to_string() };
    if host.is_empty() { String::new() } else { format!("{host}:{}", c.internet_port) }
}


/// An invite code friends paste into TTV-TTS.
/// `over` replaces the address (e.g. a Tailscale name: "vcp-pc.tail1234.ts.net:8111").
pub fn invite_code(state: &AppState, secret: &str, over: Option<&str>) -> Result<String, String> {
    let c = state.cfg.read().clone();
    let given = over.map(str::trim).filter(|a| !a.is_empty()).map(|a| if a.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) { a.to_string() } else { format!("{a}:{}", c.internet_port) });
    let address = given.unwrap_or_else(|| address(&c, state.internet.read().public_ip.as_deref()));
    if address.is_empty() {
        return Err("Set up a DuckDNS name (or test the internet access once, so the public address is known).".into());
    }
    let fingerprint = state.tls.read().fingerprint.clone();
    Ok(InviteCode { address, fingerprint, secret: secret.into(), host: c.display_name() }.encode())
}

// ── the guest API ───────────────────────────────────────────────────────

type S = State<Arc<AppState>>;

/// Who a request is (set by the middleware).
#[derive(Clone)]
struct Guest(GuestInfo);

fn is_public(path: &str) -> bool {
    matches!(path, "/ttv/v1/info" | "/ttv/v1/join" | "/api/v1/health" | "/api/v1/system/version")
}

async fn guest_auth(State(state): S, ConnectInfo(addr): ConnectInfo<SocketAddr>, mut req: Request, next: Next) -> Response {
    let ip = addr.ip();
    if state.access.locked_out(ip) {
        return api_error(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "Too many wrong attempts from this address - try again in a minute.");
    }
    if is_public(req.uri().path()) {
        return next.run(req).await;
    }
    let token = presented_key(req.headers(), req.uri().query());
    match token.as_deref().and_then(|t| state.guests.check(t, ip)) {
        Some(g) => {
            req.extensions_mut().insert(Guest(g));
            next.run(req).await
        }
        None => {
            if token.is_some() {
                state.access.failed(ip);
                log::warn!("{ip} used a wrong guest token for {}", req.uri().path());
            }
            api_error(StatusCode::UNAUTHORIZED, "unauthorized", "Missing or invalid guest token.")
        }
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ttv/v1/info", get(info))
        .route("/ttv/v1/join", post(join))
        .route("/ttv/v1/status", get(guest_status))
        .route("/ttv/v1/events", get(events))
        .route("/ttv/v1/library", get(library))
        .route("/api/v1/generation/clone", post(clone))
        .route("/api/v1/tts/languages", get(read_through))
        .route("/api/v1/models/active", get(read_through))
        .route("/api/v1/health", get(read_through))
        .route("/api/v1/system/version", get(read_through))
        .fallback(|| async { api_error(StatusCode::NOT_FOUND, "not_found", "Not available over the internet.") })
        .layer(middleware::from_fn_with_state(state.clone(), guest_auth))
        .with_state(state)
}

async fn info(State(state): S) -> Json<serde_json::Value> {
    let c = state.cfg.read();
    Json(serde_json::json!({ "name": c.display_name(), "version": env!("CARGO_PKG_VERSION"), "api": ttv_link::API_VERSION, "guest": true }))
}

async fn join(State(state): S, ConnectInfo(addr): ConnectInfo<SocketAddr>, Json(req): Json<JoinRequest>) -> Response {
    match state.guests.join(&req.secret, &req.device, addr.ip()) {
        Ok((guest, token)) => {
            state.publish_internet();
            Json(JoinResponse { token, guest, host: state.cfg.read().display_name() }).into_response()
        }
        Err(e) => {
            state.access.failed(addr.ip());
            let msg = match e {
                JoinError::Unknown => "This invite code isn't valid (any more) – each code works once.",
                JoinError::Expired => "This invite code has expired – ask for a new one.",
            };
            api_error(StatusCode::FORBIDDEN, "invite_invalid", msg)
        }
    }
}

/// The proxy's status without anything a guest shouldn't see.
fn sanitized(state: &AppState, guest: &GuestInfo) -> ttv_link::ProxyStatus {
    let mut info = state.info();
    info.pairing_open = false;
    let v = state.vcp_state.read().clone();
    let vcp = ttv_link::VcpState {
        reachable: v.reachable,
        running: v.running,
        version: v.version,
        model_loaded: v.model_loaded,
        model_kind: v.model_kind,
        model_name: v.model_name,
        active_model: v.active_model,
        queue_size: v.queue_size,
        generating: v.generating,
        starting: v.starting,
        key_ok: Some(true),
        ..Default::default()
    };
    // a fresh view of the guest (usage changes)
    let me = state.guests.guests().into_iter().find(|g| g.id == guest.id).unwrap_or_else(|| guest.clone());
    ttv_link::ProxyStatus { info, vcp, guest: Some(GuestInfo { last_address: None, ..me }), ..Default::default() }
}

async fn guest_status(State(state): S, axum::Extension(Guest(g)): axum::Extension<Guest>) -> Json<ttv_link::ProxyStatus> {
    Json(sanitized(&state, &g))
}

async fn library(State(state): S) -> Json<ttv_link::LibrarySnapshot> {
    Json(state.engine.library())
}

async fn events(State(state): S, axum::Extension(Guest(g)): axum::Extension<Guest>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| guest_events(socket, state, g))
}

/// Status (sanitized, at most every 5 s) and library changes.
async fn guest_events(mut socket: WebSocket, state: Arc<AppState>, g: GuestInfo) {
    let mut rx = state.events.subscribe();
    let send = |ev: &Event| Message::Text(serde_json::to_string(ev).unwrap_or_default().into());
    if socket.send(send(&Event::Status(Box::new(sanitized(&state, &g))))).await.is_err() {
        return;
    }
    let mut last_status = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(Event::Library(c)) => if socket.send(send(&Event::Library(c))).await.is_err() { return },
                Ok(Event::Status(_)) if last_status.elapsed() > Duration::from_secs(5) => {
                    last_status = Instant::now();
                    if socket.send(send(&Event::Status(Box::new(sanitized(&state, &g))))).await.is_err() { return }
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => return,
            },
            _ = tick.tick() => {
                // the guest was removed meanwhile: hang up
                if !state.guests.guests().iter().any(|x| x.id == g.id) { return }
                if socket.send(send(&Event::Status(Box::new(sanitized(&state, &g))))).await.is_err() { return }
            }
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            },
        }
    }
}

/// Guests take turns (one generation at a time across all of them).
static GUEST_TURN: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Deserialize)]
struct CloneBody {
    voice_id: String,
    target_text: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    emotion: Option<String>,
    #[serde(default)]
    intensity: Option<f32>,
}

/// Speaks with a computed voice of the library – nothing else.
async fn clone(State(state): S, axum::Extension(Guest(g)): axum::Extension<Guest>, Json(b): Json<CloneBody>) -> Response {
    let text = b.target_text.trim();
    if text.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "bad_request", "Nothing to say.");
    }
    if !state.engine.has_voice(&b.voice_id) {
        return api_error(StatusCode::FORBIDDEN, "voice_not_shared", "Only the computed preset voices of this host can be used.");
    }
    if let Err(e) = state.guests.charge(&g.id, text.chars().count()) {
        let (code, msg) = match e {
            QuotaError::TooLong => ("too_long", format!("At most {MAX_TEXT} characters per message.")),
            QuotaError::Daily => ("quota", "Today's character limit is used up – it resets at midnight (UTC).".to_string()),
            QuotaError::Rate => ("rate_limited", "Too many messages per minute – slow down a little.".to_string()),
        };
        return api_error(StatusCode::TOO_MANY_REQUESTS, code, msg);
    }
    // the host's own chat first, then guests one by one
    let _turn = GUEST_TURN.acquire().await;
    let deadline = Instant::now() + HOST_FIRST;
    while crate::forward::lan_busy(Duration::from_millis(1500)) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    // Busy while VCP works on it: computing jobs pause (they wait for 6 s of
    // quiet before every VCP call), so a guest never queues behind them.
    crate::updater::touch();
    crate::forward::touch_work();
    let busy = tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            crate::updater::touch();
            crate::forward::touch_work();
        }
    });
    let body = serde_json::json!({ "voice_id": b.voice_id, "target_text": text, "language": b.language, "emotion": b.emotion, "intensity": b.intensity });
    let vcp = state.vcp();
    let _vcp_turn = ttv_vcp::turn("/api/v1/generation/clone").await;
    let r = vcp.request(ttv_vcp::Method::POST, "/api/v1/generation/clone?sync=true").json(&body).timeout(Duration::from_secs(300)).send().await;
    let answer = match r {
        Ok(r) => {
            let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let bytes = r.bytes().await.unwrap_or_default();
            (status, [(axum::http::header::CONTENT_TYPE, "application/json")], bytes).into_response()
        }
        Err(e) => api_error(StatusCode::BAD_GATEWAY, "vcp_unreachable", format!("Voice Creator Pro didn't answer: {e}")),
    };
    busy.abort();
    answer
}

/// Read-only VCP lookups the app needs (languages, the active model).
async fn read_through(State(state): S, req: Request) -> Response {
    let path = req.uri().path().to_string();
    let vcp = state.vcp();
    match vcp.request(ttv_vcp::Method::GET, &path).timeout(Duration::from_secs(10)).send().await {
        Ok(r) => {
            let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let bytes = r.bytes().await.unwrap_or_default();
            (status, [(axum::http::header::CONTENT_TYPE, "application/json")], bytes).into_response()
        }
        Err(e) => api_error(StatusCode::BAD_GATEWAY, "vcp_unreachable", e.to_string()),
    }
}

// ── the listener ────────────────────────────────────────────────────────

static INTERNET_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);
static PER_ADDRESS: Mutex<Option<HashMap<IpAddr, usize>>> = Mutex::new(None);

/// Counts a connection; refuses beyond the caps.
struct ConnSlot(IpAddr);
impl ConnSlot {
    fn take(ip: IpAddr) -> Option<Self> {
        let mut g = PER_ADDRESS.lock();
        let m = g.get_or_insert_with(HashMap::new);
        let n = m.entry(ip).or_insert(0);
        if *n >= MAX_PER_ADDRESS || INTERNET_CONNECTIONS.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
            return None;
        }
        *n += 1;
        INTERNET_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
        Some(Self(ip))
    }
}
impl Drop for ConnSlot {
    fn drop(&mut self) {
        INTERNET_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
        let mut g = PER_ADDRESS.lock();
        if let Some(m) = g.as_mut()
            && let Some(n) = m.get_mut(&self.0)
        {
            *n -= 1;
            if *n == 0 {
                m.remove(&self.0);
            }
        }
    }
}

async fn bind(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    let domain = if addr.is_ipv6() { socket2::Domain::IPV6 } else { socket2::Domain::IPV4 };
    let socket = socket2::Socket::new(domain, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
    if addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(128)?;
    tokio::net::TcpListener::from_std(socket.into())
}

async fn accept(listener: tokio::net::TcpListener, state: Arc<AppState>) {
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use hyper_util::service::TowerToHyperService;
    use tower::ServiceExt;
    let router = router(state.clone());
    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(x) => x,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let Some(slot) = ConnSlot::take(addr.ip()) else { continue };
        let _ = stream.set_nodelay(true);
        let acceptor = tokio_rustls::TlsAcceptor::from(state.tls.read().config.clone());
        let router = router.clone();
        let st = state.clone();
        tokio::spawn(async move {
            let _slot = slot;
            let svc = router.map_request(move |mut req: hyper::Request<hyper::body::Incoming>| {
                req.extensions_mut().insert(ConnectInfo(addr));
                req
            });
            let svc = TowerToHyperService::new(svc);
            let Ok(Ok(tls)) = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(stream)).await else { return };
            let builder = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new());
            // Switched off or changed: open connections (keep-alive, events)
            // end too, not only the listener.
            tokio::select! {
                _ = builder.serve_connection_with_upgrades(TokioIo::new(tls), svc) => {}
                _ = st.internet_rebind.notified() => {}
            }
        });
    }
}

/// Runs the internet port while internet access is on.
pub async fn run(state: Arc<AppState>) {
    loop {
        let (on, port) = {
            let c = state.cfg.read();
            (c.internet, c.internet_port)
        };
        if !on {
            set_live(&state, |l| {
                l.listening = false;
                l.error = None;
            });
            state.internet_rebind.notified().await;
            continue;
        }
        let ip4 = crate::server::bind_ip();
        let v4 = bind(SocketAddr::from((ip4, port))).await;
        // a debug build bound to one address (tests) skips IPv6
        let v6 = if ip4 == [0, 0, 0, 0] { bind(SocketAddr::from(([0u16; 8], port))).await.ok() } else { None };
        match v4 {
            Ok(l4) => {
                log::info!("internet access on port {port}{}", if v6.is_some() { " (IPv4 + IPv6)" } else { "" });
                set_live(&state, |l| {
                    l.listening = true;
                    l.error = None;
                });
                let six = async {
                    match v6 {
                        Some(l) => accept(l, state.clone()).await,
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    _ = accept(l4, state.clone()) => {}
                    _ = six => {}
                    _ = state.internet_rebind.notified() => {
                        let on = state.cfg.read().internet;
                        log::info!("internet access {}", if on { "restarts (settings changed)" } else { "off" });
                    }
                }
            }
            Err(e) => {
                let msg = format!("port {port} cannot be used: {e}");
                log::warn!("internet access: {msg}");
                set_live(&state, |l| {
                    l.listening = false;
                    l.error = Some(msg);
                });
                tokio::select! {
                    _ = state.internet_rebind.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(30)) => {}
                }
            }
        }
    }
}

fn set_live(state: &AppState, f: impl FnOnce(&mut Live)) {
    f(&mut state.internet.write());
    state.publish_internet();
}

fn web() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(20)).user_agent(concat!("TTV-TTS-Proxy/", env!("CARGO_PKG_VERSION"))).build().expect("web client")
}

/// Keeps the DuckDNS name pointing at this network (every 5 minutes, so a
/// new address after the daily reconnect is out within minutes) while
/// friends use this proxy or an OBS link is shared on the internet.
pub async fn duckdns(state: Arc<AppState>) {
    let mut last: Option<(std::time::Instant, bool)> = None;
    loop {
        let needed = state.cfg.read().internet || state.hosted.any_online();
        // an OBS link change wakes this up too: at most one update a minute
        let recent = last.is_some_and(|(t, needed_then)| needed_then == needed && t.elapsed() < Duration::from_secs(60));
        if needed && !recent && let Some(ttv_net::Want { domain, token }) = duckdns_login(&state) {
            last = Some((std::time::Instant::now(), needed));
            let r = ttv_net::duckdns::update_ip(&domain, &token, state.upnp.public_ip().as_deref()).await;
            set_live(&state, |l| {
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
            });
        }
        tokio::select! {
            _ = state.internet_rebind.notified() => last = None,
            _ = state.hosted_changed.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(300)) => last = None,
        }
    }
}

#[derive(Deserialize)]
struct PortCheck {
    ip: String,
    reachable: bool,
}

/// Asks ifconfig.co to connect to our public address on the internet port.
pub async fn test(state: &Arc<AppState>) -> Result<bool, String> {
    let port = state.cfg.read().internet_port;
    let r: PortCheck = web().get(format!("https://ifconfig.co/port/{port}")).header("Accept", "application/json").send().await.map_err(|e| format!("The test service didn't answer: {}", e.without_url()))?.json().await.map_err(|e| e.to_string())?;
    set_live(state, |l| {
        l.test_at = Some(chrono::Utc::now().timestamp());
        l.reachable = Some(r.reachable);
        l.public_ip = Some(r.ip.clone());
    });
    Ok(r.reachable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ttv-guests-test-{}", crate::secrets::random_key()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn invites_work_once_and_tokens_are_checked() {
        let dir = tmp();
        let g = Guests::load(&dir);
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        let (inv, secret) = g.invite("Alex", 7, 20_000);
        assert_eq!(g.invites().len(), 1);
        assert_eq!(g.join("nope", "PC", ip), Err(JoinError::Unknown));
        let (info, token) = g.join(&secret, "Alex's PC", ip).unwrap();
        assert_eq!((info.name.as_str(), info.chars_per_day), ("Alex", 20_000));
        assert!(g.invites().is_empty(), "used up");
        assert_eq!(g.join(&secret, "again", ip), Err(JoinError::Unknown), "once");
        assert_eq!(g.check(&token, ip).unwrap().id, info.id);
        assert!(g.check("wrong", ip).is_none());
        // survives a restart, can be revoked
        let g2 = Guests::load(&dir);
        assert!(g2.check(&token, ip).is_some());
        assert!(g2.revoke(&info.id));
        assert!(g2.check(&token, ip).is_none());
        assert!(!inv.id.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn quotas_and_rate_limits() {
        let dir = tmp();
        let g = Guests::load(&dir);
        let (_, secret) = g.invite("Sam", 1, 1_000);
        let (info, _) = g.join(&secret, "", "198.51.100.2".parse().unwrap()).unwrap();
        assert_eq!(g.charge(&info.id, MAX_TEXT + 1), Err(QuotaError::TooLong));
        assert_eq!(g.charge(&info.id, 600), Ok(()));
        assert_eq!(g.charge(&info.id, 401), Err(QuotaError::Daily), "1000 a day");
        assert_eq!(g.charge(&info.id, 400), Ok(()));
        assert_eq!(g.guests()[0].chars_today, 1000);
        // requests per minute (also counted when the quota refused them)
        let (_, s2) = g.invite("Kim", 1, 1_000_000);
        let (k, _) = g.join(&s2, "", "198.51.100.3".parse().unwrap()).unwrap();
        for _ in 0..PER_MINUTE {
            assert_eq!(g.charge(&k.id, 10), Ok(()));
        }
        assert_eq!(g.charge(&k.id, 10), Err(QuotaError::Rate));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn expired_invites_are_refused() {
        let dir = tmp();
        let g = Guests::load(&dir);
        let (_, secret) = g.invite("Old", 1, 5_000);
        g.store.lock().invites[0].expires = chrono::Utc::now().timestamp() - 1;
        assert_eq!(g.join(&secret, "", "192.0.2.1".parse().unwrap()), Err(JoinError::Expired));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn domains_and_addresses() {
        assert_eq!(ttv_net::duckdns::clean_domain(" https://My-Stream.duckdns.org/ "), "my-stream");
        assert_eq!(ttv_net::duckdns::clean_domain("bad name!"), "badname");
        let mut c = crate::config::Config { internet_port: 8111, ..Default::default() };
        assert_eq!(address(&c, Some("203.0.113.9")), "203.0.113.9:8111");
        c.duckdns_domain = "mystream".into();
        assert_eq!(address(&c, Some("203.0.113.9")), "mystream.duckdns.org:8111");
    }

    #[test]
    fn only_the_guest_api_is_public() {
        assert!(is_public("/ttv/v1/join"));
        assert!(!is_public("/ttv/v1/library"));
        assert!(!is_public("/ttv/v1/pair"), "pairing is LAN only");
    }
}
