//! App settings (`config.json` in the config folder). Secrets (proxy tokens,
//! VCP's key) are not in here; see `secrets.rs`.

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u32 = 1;

/// Where the voices come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostConfig {
    /// Not set up yet.
    #[default]
    None,
    /// A paired TTV-TTS Proxy (usually on a second PC).
    Proxy {
        /// The proxy's installation id.
        id: String,
        name: String,
        /// "192.168.1.25:8110"
        address: String,
        /// Pinned certificate fingerprint (lower-case hex SHA-256).
        fingerprint: String,
        /// MAC address for Wake-on-LAN, learned from the proxy.
        #[serde(default)]
        mac: Option<String>,
        /// A friend's proxy over the internet: this app is their guest.
        #[serde(default)]
        guest: bool,
    },
    /// Voice Creator Pro on this PC, without a proxy.
    Direct { url: String },
}

/// How speech is generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GenerationMode {
    /// Measure both on connect and use the faster one.
    #[default]
    Auto,
    /// Whole sentences (sync generation), the next one generated while the
    /// current one plays.
    Sentences,
    /// VCP's streaming sessions.
    Streaming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// cpal device id; None = the system's default output.
    pub device: Option<String>,
    /// 0.0 – 1.0
    pub volume: f32,
    /// Pause between two messages in milliseconds.
    pub gap_ms: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { device: None, volume: 0.9, gap_ms: 250 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    /// Stable random id of this installation (used when pairing).
    pub device_id: String,
    pub host: HostConfig,
    pub generation: GenerationMode,
    pub audio: AudioConfig,
    /// "system", "en", "de"
    pub language: String,
    /// "system", "dark", "light"
    pub theme: String,
    /// "stable" or "preview"
    pub update_channel: String,
    pub setup_done: bool,
    /// Contact email for MyMemory (optional, 10× more translations a day).
    pub translate_email: String,
    /// Global hotkeys: action (skip, pause, …) -> accelerator ("Ctrl+Alt+KeyS").
    pub hotkeys: std::collections::BTreeMap<String, String>,
    /// Remote control API (Stream Deck, Streamer.bot).
    pub remote: RemoteConfig,
    /// Audio plugins (VST3, CLAP) on all speech, in this order.
    pub plugins: Vec<PluginSlot>,
    /// Client ID of the streamer's own Twitch application ("" = built in).
    pub twitch_client_id: String,
    /// OBS links served by this app (profiles that play in OBS on this PC).
    pub obs: ObsConfig,
    /// Voice effects are applied on the voice host PC (TTV-TTS Proxy), not
    /// here: less work for the gaming PC. Audio plugins always run here.
    pub effects_on_host: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ObsConfig {
    pub port: u16,
    /// Also for other PCs (a streaming PC, or the internet with port
    /// forwarding); else only OBS on this PC.
    pub lan: bool,
    /// How friends reach this PC from the internet (a DuckDNS name or the
    /// public IP). With a DuckDNS name and its token, TTV-TTS keeps the name
    /// pointing here and gets an HTTPS certificate for it.
    pub public_host: String,
    /// Links from the internet use HTTPS (needs a DuckDNS name and token).
    pub https: bool,
}

impl Default for ObsConfig {
    fn default() -> Self {
        Self { port: 8131, lan: false, public_host: String::new(), https: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginSlot {
    pub id: String,
    pub plugin: ttv_plugins::PluginInfo,
    pub enabled: bool,
    /// The plugin's settings as it saved them (base64).
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RemoteConfig {
    pub enabled: bool,
    pub port: u16,
    /// Also from other PCs in the LAN (else only this PC).
    pub lan: bool,
    /// Needed by every call; made when empty.
    pub token: String,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self { enabled: false, port: 8130, lan: false, token: String::new() }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            device_id: String::new(),
            host: HostConfig::None,
            generation: GenerationMode::Auto,
            audio: AudioConfig::default(),
            language: "system".into(),
            theme: "system".into(),
            // preview builds follow the previews, releases the releases
            update_channel: if env!("CARGO_PKG_VERSION").contains('-') { "preview" } else { "stable" }.into(),
            setup_done: false,
            translate_email: String::new(),
            hotkeys: Default::default(),
            remote: RemoteConfig::default(),
            plugins: Vec::new(),
            twitch_client_id: String::new(),
            obs: ObsConfig::default(),
            effects_on_host: true,
        }
    }
}

impl Config {
    pub fn normalize(&mut self) {
        if self.remote.token.is_empty() {
            self.remote.token = crate::secrets::random_key()[..24].to_string();
        }
        if self.remote.port < 1024 {
            self.remote.port = 8130;
        }
        if self.obs.port < 1024 || self.obs.port == self.remote.port {
            self.obs.port = 8131;
        }
        self.obs.public_host = self.obs.public_host.trim().trim_start_matches("http://").trim_start_matches("https://").trim_end_matches('/').to_string();
        if self.device_id.is_empty() {
            self.device_id = crate::secrets::random_key()[..16].to_ascii_lowercase();
        }
        self.audio.volume = self.audio.volume.clamp(0.0, 1.0);
        self.audio.gap_ms = self.audio.gap_ms.min(3000);
        if !matches!(self.update_channel.as_str(), "stable" | "preview") {
            self.update_channel = "stable".into();
        }
        self.version = CONFIG_VERSION;
    }
}

/// Name shown in the proxy's device list.
pub fn device_name() -> String {
    sysinfo::System::host_name().unwrap_or_else(|_| "TTV-TTS".into())
}
