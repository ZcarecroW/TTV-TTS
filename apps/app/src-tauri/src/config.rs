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
            update_channel: "preview".into(),
            setup_done: false,
            translate_email: String::new(),
        }
    }
}

impl Config {
    pub fn normalize(&mut self) {
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
