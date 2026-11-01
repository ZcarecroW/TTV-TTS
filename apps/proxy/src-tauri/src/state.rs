//! Shared state of the proxy: settings, secrets, TLS material, the VCP client
//! and the live status that the window and connected apps see.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use parking_lot::RwLock;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Notify, broadcast};
use ttv_link::{Event, NetAddress, ProxyInfo, ProxyStatus, SystemStats, UpdateState, VcpState};

use crate::access::Access;
use crate::config::{Config, KeyMode};
use crate::error::AppResult;
use crate::secrets::{self, Secrets};
use crate::tls::TlsMaterial;

pub struct AppState {
    pub cfg: RwLock<Config>,
    cfg_path: PathBuf,
    pub data_dir: PathBuf,
    pub secrets: Secrets,
    pub access: Access,
    pub tls: RwLock<Arc<TlsMaterial>>,
    vcp: RwLock<ttv_vcp::Client>,
    pub vcp_state: RwLock<VcpState>,
    pub events: broadcast::Sender<Event>,
    pub started: Instant,
    /// Open connections (HTTP keep-alive, streams, WebSockets).
    pub connections: AtomicU32,
    /// Listeners that want live system stats (window visible, app subscribed).
    pub system_watchers: AtomicUsize,
    pub system: RwLock<Option<SystemStats>>,
    pub addresses: RwLock<Vec<NetAddress>>,
    pub update: RwLock<UpdateState>,
    /// Wakes the monitor loop (e.g. after a setting changed).
    pub poke: Notify,
    /// Restarts the listeners (port or certificate changed).
    pub rebind: Notify,
    /// Last error of the HTTPS listener (e.g. port in use).
    pub listen_error: RwLock<Option<String>>,
    app: OnceLock<AppHandle>,
}

impl AppState {
    pub fn new(cfg: Config, cfg_path: PathBuf, data_dir: PathBuf, tls: TlsMaterial) -> Arc<Self> {
        let secrets = Secrets::new(data_dir.clone());
        let access = Access::load(&data_dir);
        let vcp = ttv_vcp::Client::new(&cfg.vcp_url, dev_vcp_key().or_else(|| secrets.get(secrets::VCP_KEY)));
        let update = UpdateState { phase: "idle".into(), channel: cfg.update_channel.clone(), ..Default::default() };
        Arc::new(Self {
            vcp_state: RwLock::new(VcpState { url: cfg.vcp_url.clone(), exe: cfg.vcp_exe.clone(), ..Default::default() }),
            cfg: RwLock::new(cfg),
            cfg_path,
            data_dir,
            secrets,
            access,
            tls: RwLock::new(Arc::new(tls)),
            vcp: RwLock::new(vcp),
            events: broadcast::channel(64).0,
            started: Instant::now(),
            connections: AtomicU32::new(0),
            system_watchers: AtomicUsize::new(0),
            system: RwLock::new(None),
            addresses: RwLock::new(Vec::new()),
            update: RwLock::new(update),
            poke: Notify::new(),
            rebind: Notify::new(),
            listen_error: RwLock::new(None),
            app: OnceLock::new(),
        })
    }

    pub fn set_app(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }

    pub fn app(&self) -> Option<&AppHandle> {
        self.app.get()
    }

    pub fn vcp(&self) -> ttv_vcp::Client {
        self.vcp.read().clone()
    }

    /// Rebuilds the VCP client after the URL or VCP's key changed.
    pub fn refresh_vcp_client(&self) {
        let url = self.cfg.read().vcp_url.clone();
        let key = self.vcp_key();
        let http = self.vcp.read().http().clone();
        *self.vcp.write() = ttv_vcp::Client::with_http(http, &url, key);
        self.vcp_state.write().url = url;
        self.poke.notify_one();
    }

    pub fn save_config(&self) -> AppResult<()> {
        let cfg = self.cfg.read().clone();
        crate::storage::write_json(&self.cfg_path, &cfg)?;
        Ok(())
    }

    /// VCP's API key (a debug build may take it from the environment).
    pub fn vcp_key(&self) -> Option<String> {
        dev_vcp_key().or_else(|| self.secrets.get(secrets::VCP_KEY))
    }

    /// The key apps must present (besides device tokens).
    pub fn access_key(&self) -> Option<String> {
        match self.cfg.read().key_mode {
            KeyMode::Vcp => self.vcp_key(),
            KeyMode::Own => self.secrets.get(secrets::ACCESS_KEY),
        }
    }

    pub fn info(&self) -> ProxyInfo {
        let cfg = self.cfg.read();
        let vcp = self.vcp_state.read();
        ProxyInfo {
            id: cfg.id.clone(),
            name: cfg.display_name(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            api: ttv_link::API_VERSION,
            port: cfg.port,
            fingerprint: self.tls.read().fingerprint.clone(),
            pairing_open: self.access.pairing_open(),
            os: std::env::consts::OS.to_string(),
            vcp_reachable: vcp.reachable,
            vcp_version: vcp.version.clone(),
        }
    }

    pub fn status(&self) -> ProxyStatus {
        ProxyStatus {
            info: self.info(),
            vcp: self.vcp_state.read().clone(),
            addresses: self.addresses.read().clone(),
            devices: self.access.devices(),
            connections: self.connections.load(Ordering::Relaxed),
            uptime_secs: self.started.elapsed().as_secs(),
            update: Some(self.update.read().clone()),
        }
    }

    /// Tells the window and connected apps that something changed.
    pub fn publish(&self) {
        let status = self.status();
        if let Some(app) = self.app() {
            let _ = app.emit("proxy://status", &status);
        }
        let _ = self.events.send(Event::Status(Box::new(status)));
    }

    pub fn publish_system(&self, stats: SystemStats) {
        if let Some(app) = self.app() {
            let _ = app.emit("proxy://system", &stats);
        }
        let _ = self.events.send(Event::System(stats.clone()));
        *self.system.write() = Some(stats);
    }
}

/// Development only: `TTV_PROXY_VCP_KEY` gives a debug build VCP's key
/// without storing it. Release builds ignore the variable.
fn dev_vcp_key() -> Option<String> {
    if cfg!(debug_assertions) { std::env::var("TTV_PROXY_VCP_KEY").ok().filter(|k| !k.is_empty()) } else { None }
}

/// Counts a listener of live system stats while it lives.
pub struct SystemWatch(Arc<AppState>);

impl SystemWatch {
    pub fn new(state: Arc<AppState>) -> Self {
        if state.system_watchers.fetch_add(1, Ordering::SeqCst) == 0 {
            state.poke.notify_one();
        }
        Self(state)
    }
}

impl Drop for SystemWatch {
    fn drop(&mut self) {
        self.0.system_watchers.fetch_sub(1, Ordering::SeqCst);
    }
}
