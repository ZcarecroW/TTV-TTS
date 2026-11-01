//! API types of TTV-TTS Proxy (`/ttv/v1/…`). Everything the proxy does not
//! handle itself is forwarded to Voice Creator Pro unchanged, so the proxy can
//! be used like VCP (same paths, same `X-API-Key` / Bearer / `?api_key=` auth).

use serde::{Deserialize, Serialize};

/// Default TCP port (HTTPS) and UDP port (discovery) of the proxy.
pub const DEFAULT_PORT: u16 = 8110;
/// Version of the `/ttv/v1` API. Bumped on incompatible changes.
pub const API_VERSION: u32 = 1;
/// Path prefix of the proxy's own API.
pub const API_PREFIX: &str = "/ttv/v1";

/// Public description of a proxy: answered to discovery and `GET /ttv/v1/info`
/// (no authentication needed, contains nothing secret).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyInfo {
    /// Stable random id of this proxy installation.
    pub id: String,
    /// Display name (default: the computer name).
    pub name: String,
    /// Proxy app version, e.g. "2.0.0".
    pub version: String,
    pub api: u32,
    /// HTTPS port.
    pub port: u16,
    /// SHA-256 fingerprint of the TLS certificate (lower-case hex, 64 chars).
    pub fingerprint: String,
    /// A pairing code is currently shown on the proxy.
    pub pairing_open: bool,
    /// "windows", "macos", "linux".
    pub os: String,
    /// VCP's HTTP API answers on the proxy PC.
    pub vcp_reachable: bool,
    pub vcp_version: Option<String>,
}

/// `POST /ttv/v1/pair` (no authentication; rate limited).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PairRequest {
    /// The 6-digit code shown in the proxy window.
    pub code: String,
    /// Name shown in the proxy's device list, e.g. "Gaming PC".
    pub device_name: String,
    /// Stable id of the app installation (re-pairing replaces the old token).
    pub device_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PairResponse {
    /// Per-device token; send it like an API key. Revocable in the proxy.
    pub token: String,
    pub proxy: ProxyInfo,
}

/// One paired device, as listed by the proxy.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    /// Unix seconds.
    pub paired_at: i64,
    pub last_seen: Option<i64>,
    pub last_address: Option<String>,
}

/// State of Voice Creator Pro on the proxy PC.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VcpState {
    /// URL the proxy uses to reach VCP (normally http://127.0.0.1:8100).
    pub url: String,
    /// VCP's process was found running on this PC.
    pub running: bool,
    pub pid: Option<u32>,
    /// Path of VCP's program file, if known (found or set by the user).
    pub exe: Option<String>,
    /// The HTTP API answers.
    pub reachable: bool,
    /// The API key the proxy uses is accepted (None = not checked yet).
    pub key_ok: Option<bool>,
    /// VCP has an API key set at all (false = open API).
    pub key_required: Option<bool>,
    pub version: Option<String>,
    /// Last error talking to VCP, in English.
    pub error: Option<String>,
    pub model_loaded: bool,
    /// Loaded checkpoint type: base, design, custom_voice …
    pub model_kind: Option<String>,
    pub model_name: Option<String>,
    pub active_model: Option<String>,
    pub queue_size: u32,
    pub generating: bool,
    /// VCP's own hardware summary ("GPU: RTX 3070 (8GB)\nAccel: CUDA").
    pub hardware: Option<String>,
    /// The proxy is starting or restarting VCP right now.
    pub starting: bool,
}

/// Live load of the proxy PC.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemStats {
    pub cpu_percent: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub gpus: Vec<GpuStats>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpuStats {
    pub name: String,
    pub vendor: Option<String>,
    pub usage_percent: Option<f32>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    /// NVIDIA only (NVML).
    pub temperature_c: Option<u32>,
    /// NVIDIA only (NVML).
    pub power_w: Option<f32>,
    /// Programs using this GPU, largest video memory first (NVIDIA only).
    pub processes: Vec<GpuProcess>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpuProcess {
    pub pid: u32,
    pub name: String,
    /// Video memory used by the process, if the driver reports it.
    pub vram: Option<u64>,
    /// GPU load caused by the process in percent (busiest engine), if known.
    pub usage: Option<f32>,
}

/// Network addresses of the proxy PC (for manual setup and Wake-on-LAN).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetAddress {
    pub interface: String,
    pub ip: String,
    pub prefix: u8,
    /// MAC address of the interface ("aa:bb:cc:dd:ee:ff").
    pub mac: String,
}

/// `GET /ttv/v1/status`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxyStatus {
    pub info: ProxyInfo,
    pub vcp: VcpState,
    pub addresses: Vec<NetAddress>,
    pub devices: Vec<DeviceInfo>,
    /// Connections currently open to the proxy (app instances, streams).
    pub connections: u32,
    pub uptime_secs: u64,
    pub update: Option<UpdateState>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateState {
    /// Newer version available, if any.
    pub available: Option<String>,
    /// "idle", "checking", "downloading", "installing", "error".
    pub phase: String,
    pub progress: Option<f32>,
    pub error: Option<String>,
    /// "stable" or "preview".
    pub channel: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogLine {
    /// Unix milliseconds.
    pub t: i64,
    /// "error", "warn", "info", "debug".
    pub level: String,
    pub message: String,
}

/// Messages on the `GET /ttv/v1/events` WebSocket (JSON text frames).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Sent on connect and whenever the status changes.
    Status(Box<ProxyStatus>),
    /// Sent every 2 s while the client subscribed with `?system=1`.
    System(SystemStats),
    Log(LogLine),
    /// A computation job changed (progress, state).
    Job(crate::voices::JobView),
    /// The library of computed voices changed.
    Library(crate::voices::LibraryChange),
}

/// `POST /ttv/v1/vcp/{start|stop|restart}` answer and other simple results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Ack {
    pub ok: bool,
    pub message: Option<String>,
}

/// Error body of every `/ttv/v1` endpoint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiError {
    pub error: String,
    /// Machine-readable code: unauthorized, pairing_closed, bad_code,
    /// rate_limited, vcp_unreachable, not_found, bad_request, internal.
    pub code: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_shape() {
        let e = Event::Log(LogLine { t: 1, level: "info".into(), message: "hi".into() });
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(s, r#"{"type":"log","t":1,"level":"info","message":"hi"}"#);
        let back: Event = serde_json::from_str(&s).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn info_is_lenient() {
        let i: ProxyInfo = serde_json::from_str(r#"{"id":"x","future_field":1}"#).unwrap();
        assert_eq!(i.id, "x");
        assert_eq!(i.port, 0);
    }
}
