//! Shared state of the app.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;
use ttv_link::{ProxyStatus, VcpState};

use crate::audio::Audio;
use crate::config::Config;
use crate::error::AppResult;
use crate::reader::ReaderData;
use crate::secrets::Secrets;

/// What the window shows about the voice host.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct HostStatus {
    /// "none", "proxy", "direct"
    pub kind: String,
    /// The proxy (or VCP) answers.
    pub connected: bool,
    pub error: Option<String>,
    /// Full proxy status (proxy mode).
    pub proxy: Option<ProxyStatus>,
    /// VCP's state (from the proxy, or measured directly).
    pub vcp: Option<VcpState>,
}

pub struct AppState {
    pub cfg: RwLock<Config>,
    cfg_path: PathBuf,
    pub data_dir: PathBuf,
    pub secrets: Secrets,
    pub audio: Arc<Audio>,
    pub reader: ReaderData,
    pub host: RwLock<HostStatus>,
    vcp: RwLock<Option<ttv_vcp::Client>>,
    /// The host setting changed: reconnect.
    pub host_changed: Notify,
    /// The window wants live system stats of the proxy PC.
    pub watch_system: AtomicBool,
    app: OnceLock<AppHandle>,
}

impl AppState {
    pub fn new(cfg: Config, cfg_path: PathBuf, data_dir: PathBuf) -> Arc<Self> {
        let secrets = Secrets::new(data_dir.clone());
        let audio = Arc::new(Audio::start(cfg.audio.device.clone(), cfg.audio.volume));
        let vcp = crate::host::client(&cfg.host, |k| secrets.get(k));
        Arc::new(Self {
            reader: ReaderData::load(&data_dir),
            cfg: RwLock::new(cfg),
            cfg_path,
            data_dir,
            secrets,
            audio,
            host: RwLock::new(HostStatus::default()),
            vcp: RwLock::new(vcp),
            host_changed: Notify::new(),
            watch_system: AtomicBool::new(false),
            app: OnceLock::new(),
        })
    }

    pub fn set_app(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }

    pub fn app(&self) -> Option<&AppHandle> {
        self.app.get()
    }

    pub fn save_config(&self) -> AppResult<()> {
        let cfg = self.cfg.read().clone();
        crate::storage::write_json(&self.cfg_path, &cfg)?;
        Ok(())
    }

    /// Client for the current voice host, if one is set up.
    pub fn vcp(&self) -> Option<ttv_vcp::Client> {
        self.vcp.read().clone()
    }

    /// Call after `cfg.host` or a host secret changed.
    pub fn host_updated(&self) {
        let host = self.cfg.read().host.clone();
        *self.vcp.write() = crate::host::client(&host, |k| self.secrets.get(k));
        self.host_changed.notify_waiters();
    }

    pub fn set_host_status(&self, s: HostStatus) {
        let changed = *self.host.read() != s;
        if changed {
            *self.host.write() = s.clone();
            if let Some(app) = self.app() {
                let _ = app.emit("app://host", &s);
            }
        }
    }

    pub fn watching_system(&self) -> bool {
        self.watch_system.load(Ordering::Relaxed)
    }
}
