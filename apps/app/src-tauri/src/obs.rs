//! OBS links served by this app: profiles that play "in OBS on this PC" get
//! a link to paste as a browser source (see `ttv_reader::obs`). The server
//! runs only while such a profile exists – on this PC only, or for other PCs
//! too (a streaming PC; the internet with port forwarding).

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use serde::Serialize;
use ttv_reader::obs::{self, Links, ObsSink, Refusal};
use ttv_reader::profile::OutputMode;

use crate::state::AppState;

/// The running server: stop signal and address.
static RUNNING: Mutex<Option<(tokio::sync::oneshot::Sender<()>, String)>> = Mutex::new(None);
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

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
    /// Browser sources connected right now.
    pub listeners: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ObsStatus {
    /// "127.0.0.1:8131" while the server runs.
    pub listening: Option<String>,
    pub error: Option<String>,
    pub links: Vec<LinkView>,
}

struct AppLinks(Weak<AppState>);

/// Constant-time comparison of a token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Links for AppLinks {
    fn sink(&self, token: &str, from: IpAddr) -> Result<Arc<ObsSink>, Refusal> {
        let st = self.0.upgrade().ok_or(Refusal::Unknown)?;
        let (id, online) = {
            let s = st.reader.store.read();
            let p = s.profiles.iter().find(|p| p.output.mode == OutputMode::Obs && same(&p.output.token, token)).ok_or(Refusal::Unknown)?;
            (p.id.clone(), p.output.online)
        };
        if !online && !obs::is_local(from) {
            return Err(Refusal::NotOnline);
        }
        Ok(st.obs.get(&id))
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
    let host = if c.lan || online { "0.0.0.0" } else { "127.0.0.1" };
    let addr = format!("{host}:{}", c.port);
    if needed && RUNNING.lock().as_ref().is_some_and(|(_, a)| *a == addr) {
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
    *RUNNING.lock() = Some((tx, addr.clone()));
    log::info!("OBS links on http://{addr}/");
    let app = obs::router(Arc::new(AppLinks(Arc::downgrade(state))));
    tokio::spawn(async move {
        let r = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(async {
            let _ = rx.await;
        });
        if let Err(e) = r.await {
            log::warn!("OBS links stopped: {e}");
        }
    });
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
    let listening = RUNNING.lock().as_ref().map(|(_, a)| a.clone());
    let c = state.cfg.read().obs.clone();
    let lan = listening.as_deref().is_some_and(|a| a.starts_with("0.0.0.0"));
    let lan_ips = if lan { lan_addresses() } else { Vec::new() };
    let links = state
        .reader
        .store
        .read()
        .profiles
        .iter()
        .filter(|p| p.output.mode == OutputMode::Obs)
        .map(|p| {
            let path = format!("/o/{}", p.output.token);
            LinkView {
                profile: p.id.clone(),
                local: format!("http://127.0.0.1:{}{path}", c.port),
                lan: lan_ips.iter().map(|ip| format!("http://{ip}:{}{path}", c.port)).collect(),
                online: (p.output.online && !c.public_host.is_empty()).then(|| format!("http://{}:{}{path}", c.public_host, c.port)),
                listeners: state.obs.listeners(&p.id),
            }
        })
        .collect();
    ObsStatus { listening, error: LAST_ERROR.lock().clone(), links }
}
