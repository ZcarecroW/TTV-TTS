//! Secret storage (proxy device tokens, VCP's API key for direct use). Uses the OS
//! credential store (Windows Credential Manager, macOS Keychain, Secret
//! Service). Without one (e.g. a Linux box without a Secret Service) it falls
//! back to a file in the app data folder that only this user can read.

use std::collections::BTreeMap;
use std::path::PathBuf;

use parking_lot::Mutex;

use crate::error::{AppError, AppResult};

const SERVICE: &str = "TTV-TTS";

/// VCP's API key when TTV-TTS talks to VCP directly (same PC).
pub const VCP_KEY: &str = "vcp-api-key";
/// The streamer's own DeepL API key ("translate for me").
pub const DEEPL_KEY: &str = "deepl-api-key";

/// Device token for a paired proxy.
pub fn proxy_token(proxy_id: &str) -> String {
    format!("proxy-token:{proxy_id}")
}

pub struct Secrets {
    fallback_path: PathBuf,
    use_fallback: bool,
    lock: Mutex<()>,
    /// Secrets are read on every request: keep them in memory once loaded.
    cache: Mutex<BTreeMap<String, Option<String>>>,
}

impl Secrets {
    pub fn new(data_dir: PathBuf) -> Self {
        let use_fallback = keyring::Entry::store_status().is_err();
        if use_fallback {
            log::warn!("OS credential store unavailable, using a file in the app folder");
        }
        Self {
            fallback_path: data_dir.join("secrets.json"),
            use_fallback,
            lock: Mutex::new(()),
            cache: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        if let Some(v) = self.cache.lock().get(key) {
            return v.clone();
        }
        let v = self.load(key);
        self.cache.lock().insert(key.to_string(), v.clone());
        v
    }

    fn load(&self, key: &str) -> Option<String> {
        if self.use_fallback {
            let _g = self.lock.lock();
            return self.read_file().remove(key);
        }
        let entry = keyring::Entry::new(SERVICE, key).ok()?;
        match entry.get_password() {
            Ok(v) => Some(v),
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                log::warn!("reading secret {key} failed: {e}");
                None
            }
        }
    }

    pub fn set(&self, key: &str, value: Option<&str>) -> AppResult<()> {
        let value = value.map(str::trim).filter(|v| !v.is_empty());
        if self.use_fallback {
            let _g = self.lock.lock();
            let mut map = self.read_file();
            match value {
                Some(v) => map.insert(key.to_string(), v.to_string()),
                None => map.remove(key),
            };
            self.write_file(&map)?;
        } else {
            let entry = keyring::Entry::new(SERVICE, key).map_err(|e| AppError::Keyring(e.to_string()))?;
            match value {
                Some(v) => entry.set_password(v).map_err(|e| AppError::Keyring(e.to_string()))?,
                None => match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => return Err(AppError::Keyring(e.to_string())),
                },
            }
        }
        self.cache.lock().insert(key.to_string(), value.map(str::to_string));
        Ok(())
    }

    fn read_file(&self) -> BTreeMap<String, String> {
        crate::storage::read_json(&self.fallback_path).unwrap_or_default()
    }

    fn write_file(&self, map: &BTreeMap<String, String>) -> AppResult<()> {
        crate::storage::write_json(&self.fallback_path, map)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.fallback_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

/// A random key/token: 32 URL-safe characters (~190 bits).
pub fn random_key() -> String {
    use rand::RngExt;
    const ABC: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789-_";
    let mut rng = rand::rng();
    (0..32).map(|_| ABC[rng.random_range(0..ABC.len())] as char).collect()
}
