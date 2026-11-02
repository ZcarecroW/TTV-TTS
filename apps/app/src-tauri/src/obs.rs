//! OBS links served by this app: profiles that play "in OBS on this PC" get
//! a link to paste as a browser source (see `ttv_reader::obs`). The server
//! runs only while such a profile exists – on this PC only, or for other PCs
//! too (a streaming PC; the internet with port forwarding). One port serves
//! plain HTTP (this PC, the network) and HTTPS (the internet, with a free
//! Let's Encrypt certificate for the DuckDNS name; see `ttv_net`).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use serde::Serialize;
use ttv_reader::obs::{self, Links, ObsSink, Refusal};
use ttv_reader::profile::OutputMode;

use crate::state::AppState;

/// The running server: stop signal and address.
static RUNNING: Mutex<Option<(tokio::sync::oneshot::Sender<()>, String)>> = Mutex::new(None);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);
/// The last DuckDNS update.
static DUCKDNS: Mutex<Option<DuckDnsView>> = Mutex::new(None);
static DUCKDNS_NOW: tokio::sync::Notify = tokio::sync::Notify::const_new();

#[derive(Debug, Clone, Serialize)]
pub struct DuckDnsView {
    /// Unix seconds.
    pub at: i64,
    pub error: Option<String>,
    /// The public address DuckDNS saw.
    pub ip: Option<String>,
}

/// The audio of each OBS profile (profile id -> sink), made on first use.
#[derive(Default)]
pub struct Sinks(Mutex<HashMap<String, Arc<ObsSink>>>);

impl Sinks {
    pub fn get(&self, profile: &str) -> Arc<ObsSink> {
        self.0.lock().entry(profile.to_string()).or_insert_with(ObsSink::new).clone()
    }

    fn listeners(&self, profile: &str) -> usize {
        self.0.lock().get(profile).map_or(0, |s| s.listeners())
    }
}

/// The link of one profile, as the window shows it.
#[derive(Debug, Clone, Serialize)]
pub struct LinkView {
    pub profile: String,
    /// For OBS on this PC.
    pub local: String,
    /// For OBS on another PC in this network.
    pub lan: Vec<String>,
    /// From the internet (needs port forwarding and a public address).
    pub online: Option<String>,
    /// That link uses HTTPS.
    pub online_secure: bool,
    /// Shared on the internet, but the HTTPS certificate is still being fetched.
    pub securing: bool,
    /// Browser sources connected right now.
    pub listeners: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ObsStatus {
    /// "127.0.0.1:8131" while the server runs.
    pub listening: Option<String>,
    pub error: Option<String>,
    pub links: Vec<LinkView>,
    /// The address from the internet is a DuckDNS name.
    pub duckdns: bool,
    /// Its token is stored.
    pub duckdns_token: bool,
    pub duckdns_update: Option<DuckDnsView>,
    /// The HTTPS certificate.
    pub https: ttv_net::CertStatus,
}

struct AppLinks(Weak<AppState>);

/// Constant-time comparison of a token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Links for AppLinks {
    fn sink(&self, token: &str, from: IpAddr, secure: bool) -> Result<Arc<ObsSink>, Refusal> {
        let st = self.0.upgrade().ok_or(Refusal::Unknown)?;
        let (id, online) = {
            let s = st.reader.store.read();
            let p = s.profiles.iter().find(|p| p.output.mode == OutputMode::Obs && same(&p.output.token, token)).ok_or(Refusal::Unknown)?;
            (p.id.clone(), p.output.online)
        };
        if !online && !obs::is_local(from) {
            return Err(Refusal::NotOnline);
        }
        // from the internet the link (its token) must not travel unencrypted
        if !obs::is_local(from) && !secure && needs_https(&st) {
            return Err(Refusal::NeedsHttps);
        }
        Ok(st.obs.get(&id))
    }
}

/// The DuckDNS name ("mystream") when the address from the internet is one.
fn duckdns_domain(c: &crate::config::ObsConfig) -> Option<String> {
    ttv_net::duckdns::is_duckdns(&c.public_host).then(|| ttv_net::duckdns::clean_domain(&c.public_host))
}

fn any_online(state: &AppState) -> bool {
    state.reader.store.read().profiles.iter().any(|p| p.output.mode == OutputMode::Obs && p.output.online)
}

/// The DuckDNS name and token, when an OBS link is shared on the internet.
fn duckdns_login(state: &AppState) -> Option<ttv_net::Want> {
    let domain = duckdns_domain(&state.cfg.read().obs)?;
    let token = state.secrets.get(crate::secrets::DUCKDNS_TOKEN)?;
    any_online(state).then_some(ttv_net::Want { domain, token })
}

/// What the certificate is needed for (HTTPS switched on).
pub fn https_want(state: &AppState) -> Option<ttv_net::Want> {
    state.cfg.read().obs.https.then(|| duckdns_login(state)).flatten()
}

/// From the internet over plain HTTP while HTTPS works: refused.
fn needs_https(state: &AppState) -> bool {
    let c = state.cfg.read().obs.clone();
    c.https && duckdns_domain(&c).is_some_and(|d| state.certs.valid_for(&ttv_net::duckdns::fqdn(&d)))
}

/// How links from the internet start: (`https://name.duckdns.org:port`, true)
/// with a certificate; the plain address when HTTPS is off or can't be had;
/// `None` while the certificate is being fetched.
fn online_base(state: &AppState) -> Option<(String, bool)> {
    let c = state.cfg.read().obs.clone();
    let host = c.public_host.split(['/', ':']).next().unwrap_or_default().to_string();
    if host.is_empty() {
        return None;
    }
    match duckdns_domain(&c) {
        Some(d) if c.https && state.secrets.get(crate::secrets::DUCKDNS_TOKEN).is_some() => {
            let f = ttv_net::duckdns::fqdn(&d);
            if state.certs.valid_for(&f) {
                Some((format!("https://{f}:{}", c.port), true))
            } else if state.certs.status().error.is_some() {
                Some((format!("http://{f}:{}", c.port), false))
            } else {
                None
            }
        }
        _ => Some((format!("http://{host}:{}", c.port), false)),
    }
}

/// Keeps the DuckDNS name pointing at this network while an OBS link is
/// shared on the internet (every 5 minutes: a new address after the daily
/// reconnect is out within minutes).
pub async fn duckdns(state: Arc<AppState>) {
    loop {
        if let Some(ttv_net::Want { domain, token }) = duckdns_login(&state) {
            let r = ttv_net::duckdns::update_ip(&domain, &token).await;
            let at = chrono::Utc::now().timestamp();
            *DUCKDNS.lock() = Some(match r {
                Ok(ip) => DuckDnsView { at, error: None, ip },
                Err(e) => DuckDnsView { at, error: Some(e), ip: None },
            });
        }
        tokio::select! {
            _ = DUCKDNS_NOW.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(300)) => {}
        }
    }
}

/// Starts, restarts or stops the server to match the profiles and settings.
pub async fn apply(state: &Arc<AppState>) {
    let (needed, online) = {
        let s = state.reader.store.read();
        let obs: Vec<_> = s.profiles.iter().filter(|p| p.output.mode == OutputMode::Obs).collect();
        (!obs.is_empty(), obs.iter().any(|p| p.output.online))
    };
    let c = state.cfg.read().obs.clone();
    // the certificate and the DuckDNS name follow the profiles and settings
    state.certs.wake();
    DUCKDNS_NOW.notify_one();
    let host = if c.lan || online { "0.0.0.0" } else { "127.0.0.1" };
    let https = c.https && online;
    let addr = format!("{host}:{}", c.port);
    let key = format!("{addr} {https}");
    if needed && RUNNING.lock().as_ref().is_some_and(|(_, a)| *a == key) {
        return;
    }
    if let Some((stop, _)) = RUNNING.lock().take() {
        let _ = stop.send(());
    }
    *LAST_ERROR.lock() = None;
    if !needed {
        return;
    }
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
        log::warn!("OBS links: {}", LAST_ERROR.lock().clone().unwrap_or_default());
        return;
    };
    *LAST_ERROR.lock() = None;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    *RUNNING.lock() = Some((tx, key));
    log::info!("OBS links on {addr}{}", if https { " (HTTP and HTTPS)" } else { "" });
    let app = obs::router(Arc::new(AppLinks(Arc::downgrade(state))));
    let tls = https.then(|| state.certs.server_config());
    tokio::spawn(ttv_net::serve::serve(listener, app, tls, async {
        let _ = rx.await;
    }));
}

/// This PC's addresses in the local network.
fn lan_addresses() -> Vec<String> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            IpAddr::V4(v) if v.is_private() => Some(v.to_string()),
            _ => None,
        })
        .collect()
}

pub fn status(state: &AppState) -> ObsStatus {
    let listening = RUNNING.lock().as_ref().map(|(_, a)| a.split(' ').next().unwrap_or_default().to_string());
    let c = state.cfg.read().obs.clone();
    let lan = listening.as_deref().is_some_and(|a| a.starts_with("0.0.0.0"));
    let lan_ips = if lan { lan_addresses() } else { Vec::new() };
    let online = online_base(state);
    let links = state
        .reader
        .store
        .read()
        .profiles
        .iter()
        .filter(|p| p.output.mode == OutputMode::Obs)
        .map(|p| {
            let path = format!("/o/{}", p.output.token);
            let base = p.output.online.then(|| online.clone()).flatten();
            LinkView {
                profile: p.id.clone(),
                local: format!("http://127.0.0.1:{}{path}", c.port),
                lan: lan_ips.iter().map(|ip| format!("http://{ip}:{}{path}", c.port)).collect(),
                online_secure: base.as_ref().is_some_and(|b| b.1),
                securing: p.output.online && base.is_none() && !c.public_host.is_empty(),
                online: base.map(|(b, _)| format!("{b}{path}")),
                listeners: state.obs.listeners(&p.id),
            }
        })
        .collect();
    ObsStatus {
        listening,
        error: LAST_ERROR.lock().clone(),
        links,
        duckdns: duckdns_domain(&c).is_some(),
        duckdns_token: state.secrets.get(crate::secrets::DUCKDNS_TOKEN).is_some(),
        duckdns_update: DUCKDNS.lock().clone(),
        https: state.certs.status(),
    }
}
