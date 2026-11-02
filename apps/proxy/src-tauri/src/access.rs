//! Who may use the proxy: the access key (VCP's key or the proxy's own one)
//! and per-device tokens handed out by pairing. Tokens are stored only as
//! SHA-256 hashes. Failed attempts are rate limited per address.

use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use ttv_link::DeviceInfo;

use crate::storage;

/// A pairing code is valid this long after it was shown.
const CODE_TTL: Duration = Duration::from_secs(5 * 60);
/// Wrong codes allowed before the code is withdrawn.
const CODE_TRIES: u32 = 5;
/// Failed authentications per address and minute before it is locked out.
const MAX_FAILURES: u32 = 20;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDevice {
    id: String,
    name: String,
    token_hash: String,
    paired_at: i64,
    last_seen: Option<i64>,
    last_address: Option<String>,
}

struct Pairing {
    code: String,
    opened: Instant,
    wrong: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PairError {
    Closed,
    BadCode,
    RateLimited,
}

pub enum Who {
    /// The access key.
    Key,
    /// A paired device (its id).
    Device(#[allow(dead_code)] String),
}

pub struct Access {
    path: PathBuf,
    devices: Mutex<Vec<StoredDevice>>,
    pairing: Mutex<Option<Pairing>>,
    failures: ttv_net::limits::Strikes,
    /// Last time `last_seen` was written to disk.
    saved: Mutex<Instant>,
}

pub fn hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

impl Access {
    pub fn load(data_dir: &std::path::Path) -> Self {
        let path = data_dir.join("devices.json");
        let devices: Vec<StoredDevice> = storage::read_json(&path).unwrap_or_default();
        Self {
            path,
            devices: Mutex::new(devices),
            pairing: Mutex::new(None),
            failures: ttv_net::limits::Strikes::new(MAX_FAILURES, FAILURE_WINDOW),
            saved: Mutex::new(Instant::now()),
        }
    }

    fn save(&self, devices: &[StoredDevice]) {
        if let Err(e) = storage::write_json(&self.path, &devices) {
            log::warn!("saving the device list failed: {e}");
        }
        *self.saved.lock() = Instant::now();
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.devices
            .lock()
            .iter()
            .map(|d| DeviceInfo {
                id: d.id.clone(),
                name: d.name.clone(),
                paired_at: d.paired_at,
                last_seen: d.last_seen,
                last_address: d.last_address.clone(),
            })
            .collect()
    }

    pub fn revoke(&self, id: &str) -> bool {
        let mut g = self.devices.lock();
        let before = g.len();
        g.retain(|d| d.id != id);
        let changed = g.len() != before;
        if changed {
            self.save(&g);
            log::info!("device {id} removed");
        }
        changed
    }

    // ── pairing ───────────────────────────────────────────────────────────

    /// Shows a new 6-digit code (replaces an open one).
    pub fn open_pairing(&self) -> String {
        use rand::RngExt;
        let code = format!("{:06}", rand::rng().random_range(0..1_000_000u32));
        *self.pairing.lock() = Some(Pairing { code: code.clone(), opened: Instant::now(), wrong: 0 });
        log::info!("pairing opened for 5 minutes");
        code
    }

    pub fn close_pairing(&self) {
        *self.pairing.lock() = None;
    }

    /// The open code and its remaining seconds.
    pub fn pairing(&self) -> Option<(String, u64)> {
        let mut g = self.pairing.lock();
        let p = g.as_ref()?;
        let left = CODE_TTL.checked_sub(p.opened.elapsed());
        match left {
            Some(l) => Some((p.code.clone(), l.as_secs())),
            None => {
                *g = None;
                None
            }
        }
    }

    pub fn pairing_open(&self) -> bool {
        self.pairing().is_some()
    }

    /// Checks the code; on success stores the device and returns its new token.
    pub fn pair(&self, code: &str, device_id: &str, device_name: &str, addr: IpAddr) -> Result<String, PairError> {
        if self.locked_out(addr) {
            return Err(PairError::RateLimited);
        }
        let mut g = self.pairing.lock();
        let Some(p) = g.as_mut() else { return Err(PairError::Closed) };
        if p.opened.elapsed() > CODE_TTL {
            *g = None;
            return Err(PairError::Closed);
        }
        let ok: bool = p.code.as_bytes().ct_eq(code.trim().as_bytes()).into();
        if !ok {
            p.wrong += 1;
            self.failed(addr);
            if p.wrong >= CODE_TRIES {
                *g = None;
                log::warn!("pairing closed after {CODE_TRIES} wrong codes (last from {addr})");
            }
            return Err(PairError::BadCode);
        }
        *g = None;
        drop(g);

        let token = crate::secrets::random_key();
        let id = if device_id.trim().is_empty() { crate::secrets::random_key()[..12].to_string() } else { device_id.trim().to_string() };
        let name = if device_name.trim().is_empty() { "TTV-TTS".to_string() } else { device_name.trim().chars().take(60).collect() };
        let mut devices = self.devices.lock();
        devices.retain(|d| d.id != id);
        devices.push(StoredDevice {
            id: id.clone(),
            name: name.clone(),
            token_hash: hash(&token),
            paired_at: chrono::Utc::now().timestamp(),
            last_seen: Some(chrono::Utc::now().timestamp()),
            last_address: Some(addr.to_string()),
        });
        self.save(&devices);
        log::info!("paired with \"{name}\" ({addr})");
        Ok(token)
    }

    // ── authentication ────────────────────────────────────────────────────

    /// Checks a presented key against the access key and the device tokens.
    pub fn check(&self, presented: &str, access_key: Option<&str>, addr: IpAddr) -> Option<Who> {
        if presented.is_empty() {
            return None;
        }
        if let Some(k) = access_key
            && !k.is_empty()
            && bool::from(k.as_bytes().ct_eq(presented.as_bytes()))
        {
            return Some(Who::Key);
        }
        let h = hash(presented);
        let mut devices = self.devices.lock();
        let d = devices.iter_mut().find(|d| bool::from(d.token_hash.as_bytes().ct_eq(h.as_bytes())))?;
        let now = chrono::Utc::now().timestamp();
        let id = d.id.clone();
        let addr_s = addr.to_string();
        let changed = d.last_address.as_deref() != Some(addr_s.as_str());
        d.last_seen = Some(now);
        d.last_address = Some(addr_s);
        // `last_seen` changes on every request: write it at most once a minute.
        if changed || self.saved.lock().elapsed() > Duration::from_secs(60) {
            let snapshot = devices.clone();
            drop(devices);
            self.save(&snapshot);
        }
        Some(Who::Device(id))
    }

    pub fn locked_out(&self, addr: IpAddr) -> bool {
        self.failures.blocked(addr)
    }

    pub fn failed(&self, addr: IpAddr) {
        if self.failures.strike(addr) == MAX_FAILURES {
            log::warn!("{addr} sent {MAX_FAILURES} wrong keys within a minute - blocked for a minute");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ttv-proxy-test-{}", crate::secrets::random_key()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn pairing_flow() {
        let dir = tmp();
        let a = Access::load(&dir);
        let ip: IpAddr = "192.168.1.5".parse().unwrap();
        assert_eq!(a.pair("123456", "dev1", "PC", ip), Err(PairError::Closed));
        let code = a.open_pairing();
        let wrong = if code == "000000" { "111111" } else { "000000" };
        assert_eq!(a.pair(wrong, "dev1", "PC", ip), Err(PairError::BadCode));
        let token = a.pair(&code, "dev1", "PC", ip).unwrap();
        assert!(!a.pairing_open(), "a used code is closed");
        assert!(matches!(a.check(&token, None, ip), Some(Who::Device(id)) if id == "dev1"));
        assert!(a.check("nope", None, ip).is_none());
        assert!(matches!(a.check("key", Some("key"), ip), Some(Who::Key)));
        // Survives a restart.
        let b = Access::load(&dir);
        assert!(b.check(&token, None, ip).is_some());
        assert!(b.revoke("dev1"));
        assert!(b.check(&token, None, ip).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn wrong_codes_close_pairing() {
        let dir = tmp();
        let a = Access::load(&dir);
        let ip: IpAddr = "10.0.0.2".parse().unwrap();
        let code = a.open_pairing();
        let wrong = if code == "000000" { "111111" } else { "000000" };
        for _ in 0..CODE_TRIES {
            assert_eq!(a.pair(wrong, "x", "x", ip), Err(PairError::BadCode));
        }
        assert_eq!(a.pair(&code, "x", "x", ip), Err(PairError::Closed));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lockout() {
        let dir = tmp();
        let a = Access::load(&dir);
        let ip: IpAddr = "10.0.0.3".parse().unwrap();
        for _ in 0..MAX_FAILURES {
            a.failed(ip);
        }
        assert!(a.locked_out(ip));
        assert!(!a.locked_out("10.0.0.4".parse().unwrap()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
