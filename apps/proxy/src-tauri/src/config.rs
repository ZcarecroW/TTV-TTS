//! Proxy settings (`config.json` in the app's config folder). Secrets are not
//! in here; see `secrets.rs`.

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeyMode {
    /// Apps use VCP's own API key to talk to the proxy.
    #[default]
    Vcp,
    /// Apps use a separate access key of the proxy.
    Own,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    /// Stable random id of this installation (shown to apps, used for pairing).
    pub id: String,
    /// Display name in the apps; empty = computer name.
    pub name: String,
    /// HTTPS port for apps, also the UDP discovery port.
    pub port: u16,
    /// Answer LAN discovery requests.
    pub discovery: bool,
    /// Where VCP's HTTP API listens on this PC.
    pub vcp_url: String,
    /// Which key apps use to talk to the proxy.
    pub key_mode: KeyMode,
    /// VCP's program file (found automatically or set by the user).
    pub vcp_exe: Option<String>,
    /// Start VCP when the proxy starts (if it isn't running). Off by default.
    pub start_vcp: bool,
    /// Start VCP again whenever it is not running.
    pub keep_vcp_running: bool,
    /// Restart VCP when its API stops answering for `hang_restart_secs`.
    pub restart_on_hang: bool,
    pub hang_restart_secs: u32,
    /// Restart VCP at a quiet moment when it has become clearly slower than
    /// usual (it slows down the longer it runs).
    pub restart_when_slow: bool,
    /// Also accept plain-HTTP connections on this port (0 = off). For older
    /// tools that can't use the proxy's certificate; the key still applies.
    pub http_port: u16,
    /// Start when the user signs in. Off by default.
    pub autostart: bool,
    /// Keep this PC from sleeping while the proxy works (computing voices,
    /// a TTV-TTS connected, an OBS link playing, a friend connected).
    pub keep_awake: bool,
    /// "stable" or "preview".
    pub update_channel: String,
    /// Install updates automatically when the proxy is idle.
    pub auto_update: bool,
    /// UI language: "system", "en", "de".
    pub language: String,
    /// "system", "dark", "light".
    pub theme: String,
    /// The first-start assistant has been completed.
    pub setup_done: bool,
    /// Internet access for friends (guest API on its own port).
    pub internet: bool,
    pub internet_port: u16,
    /// DuckDNS name ("mystream" for mystream.duckdns.org); its token is a secret.
    pub duckdns_domain: String,
    /// Port of the OBS links: plain HTTP in the network, HTTPS from the
    /// internet (OBS can't use the proxy's own certificate, so links from the
    /// internet get a free one from Let's Encrypt for the DuckDNS name).
    pub obs_port: u16,
    /// Links from the internet use HTTPS (a Let's Encrypt certificate for the DuckDNS name).
    pub obs_https: bool,
    /// Opens the ports for the internet in the router itself (UPnP).
    pub upnp: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            id: String::new(),
            name: String::new(),
            port: ttv_link::DEFAULT_PORT,
            discovery: true,
            vcp_url: "http://127.0.0.1:8100".into(),
            key_mode: KeyMode::Vcp,
            vcp_exe: None,
            start_vcp: false,
            keep_vcp_running: false,
            restart_on_hang: false,
            hang_restart_secs: 180,
            restart_when_slow: true,
            http_port: 0,
            autostart: false,
            keep_awake: true,
            // preview builds follow the previews, releases the releases
            update_channel: if env!("CARGO_PKG_VERSION").contains('-') { "preview" } else { "stable" }.into(),
            auto_update: true,
            language: "system".into(),
            theme: "system".into(),
            setup_done: false,
            internet: false,
            internet_port: 8111,
            duckdns_domain: String::new(),
            obs_port: 8112,
            obs_https: true,
            upnp: true,
        }
    }
}

impl Config {
    /// Fills in what a fresh or older config lacks and keeps values sane.
    pub fn normalize(&mut self) {
        if self.id.is_empty() {
            self.id = crate::secrets::random_key()[..12].to_ascii_lowercase();
        }
        if self.port == 0 {
            self.port = ttv_link::DEFAULT_PORT;
        }
        if self.obs_port < 1024 || self.obs_port == self.port || self.obs_port == self.internet_port {
            self.obs_port = 8112;
        }
        if self.vcp_url.trim().is_empty() {
            self.vcp_url = Config::default().vcp_url;
        }
        self.vcp_url = self.vcp_url.trim().trim_end_matches('/').to_string();
        self.hang_restart_secs = self.hang_restart_secs.clamp(60, 1800);
        if self.internet_port == 0 || self.internet_port == self.port {
            self.internet_port = if self.port == 8111 { 8112 } else { 8111 };
        }
        self.duckdns_domain = ttv_net::duckdns::clean_domain(&self.duckdns_domain);
        if !matches!(self.update_channel.as_str(), "stable" | "preview") {
            self.update_channel = "stable".into();
        }
        self.version = CONFIG_VERSION;
    }

    pub fn display_name(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.trim().to_string();
        }
        sysinfo::System::host_name().unwrap_or_else(|_| "TTV-TTS Proxy".into())
    }
}
