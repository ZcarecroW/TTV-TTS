//! Takes over VCP's API key from VCP's own settings file, so the user doesn't
//! have to copy it. VCP (2.4.x) keeps its settings in
//! `%LOCALAPPDATA%\VoiceCloner\config.json`.
//!
//! Safety: the file may hold keys of other services too (LLM providers for
//! VCP's audiobook tools, cloud sync …). Fields that look like those are
//! skipped, candidates are only ever sent to a VCP on this same PC, detection
//! only runs when VCP really enforces a key, and a candidate is only kept if
//! VCP accepts it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use serde_json::Value;

use crate::state::AppState;

/// Unix seconds of the last automatic attempt (at most one per 5 minutes).
static LAST_AUTO: AtomicI64 = AtomicI64::new(0);

/// Path parts of settings that belong to other services.
const FOREIGN: &[&str] = &[
    "llm", "openai", "anthropic", "claude", "gemini", "google", "openrouter", "deepseek", "mistral", "groq", "xai", "elevenlabs",
    "fish", "huggingface", "hf_", "youtube", "cloud", "account", "sync", "license", "stripe",
];

fn config_files(exe: Option<&str>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        v.push(PathBuf::from(l).join("VoiceCloner").join("config.json"));
    }
    if let Some(h) = std::env::var_os("HOME") {
        let h = PathBuf::from(h);
        v.push(h.join("Library/Application Support/VoiceCloner/config.json"));
        v.push(h.join(".local/share/VoiceCloner/config.json"));
    }
    if let Some(dir) = exe.and_then(|e| Path::new(e).parent()) {
        v.push(dir.join("config.json"));
    }
    v.retain(|p| p.is_file());
    v
}

/// (dotted path, value) of every plausible API-key field.
fn collect(v: &Value, path: &str, out: &mut Vec<(String, String)>) {
    let Value::Object(map) = v else { return };
    for (k, val) in map {
        let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
        let lp = p.to_ascii_lowercase();
        if FOREIGN.iter().any(|f| lp.contains(f)) {
            continue;
        }
        match val {
            Value::String(s) => {
                let lk = k.to_ascii_lowercase();
                if (lk.contains("api_key") || lk.contains("apikey") || lk.contains("api-key")) && s.trim().len() >= 8 {
                    out.push((p, s.trim().to_string()));
                }
            }
            Value::Object(_) => collect(val, &p, out),
            _ => {}
        }
    }
}

fn candidates(exe: Option<&str>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for f in config_files(exe) {
        if let Ok(bytes) = std::fs::read(&f)
            && let Ok(v) = serde_json::from_slice::<Value>(&bytes)
        {
            collect(&v, "", &mut out);
        }
    }
    // Settings of the HTTP API first, then the shortest (most general) path.
    out.sort_by_key(|(p, _)| (!p.to_ascii_lowercase().contains("http"), p.len()));
    out.dedup_by(|a, b| a.1 == b.1);
    out
}

fn is_local(url: &str) -> bool {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string)).is_some_and(|h| h == "127.0.0.1" || h == "localhost" || h == "[::1]")
}

/// Finds VCP's key in its settings and returns it if VCP accepts it.
pub async fn detect(state: &Arc<AppState>) -> Result<String, String> {
    let url = state.cfg.read().vcp_url.clone();
    if !is_local(&url) {
        return Err("Voice Creator Pro runs on another computer - paste its key by hand.".into());
    }
    let http = state.vcp().http().clone();
    // Only meaningful if VCP enforces a key: a made-up one must be refused.
    let probe = ttv_vcp::Client::with_http(http.clone(), &url, Some("ttv-tts-proxy-probe-not-a-key".into()));
    match probe.check_key().await {
        Err(ttv_vcp::Error::Unauthorized) => {}
        Ok(()) => return Err("Voice Creator Pro has no API key set.".into()),
        Err(e) => return Err(e.to_string()),
    }
    let exe = state.cfg.read().vcp_exe.clone();
    let list = candidates(exe.as_deref());
    for (path, key) in list {
        let c = ttv_vcp::Client::with_http(http.clone(), &url, Some(key.clone()));
        if c.check_key().await.is_ok() {
            log::info!("took over VCP's API key from its settings ({path})");
            return Ok(key);
        }
    }
    Err("No working API key found in Voice Creator Pro's settings - paste it by hand.".into())
}

/// Called by the monitor when VCP refuses us: tries the settings file, at most
/// every 5 minutes, and stores a key VCP accepts.
pub async fn auto(state: &Arc<AppState>) -> bool {
    let now = chrono::Utc::now().timestamp();
    if now - LAST_AUTO.load(Ordering::Relaxed) < 300 {
        return false;
    }
    LAST_AUTO.store(now, Ordering::Relaxed);
    match detect(state).await {
        Ok(key) => {
            if state.secrets.set(crate::secrets::VCP_KEY, Some(&key)).is_ok() {
                state.refresh_vcp_client();
                return true;
            }
            false
        }
        Err(e) => {
            log::debug!("automatic key takeover: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_api_key_fields_but_not_foreign_ones() {
        let v: Value = serde_json::json!({
            "http_api": { "enabled": true, "port": 8100, "api_key": "local-key-123" },
            "api_key": "top-level-key",
            "audiobook_llm": { "api_key": "sk-provider-secret" },
            "openai_api_key": "sk-other",
            "short": { "api_key": "x" }
        });
        let mut out = Vec::new();
        collect(&v, "", &mut out);
        out.sort_by_key(|(p, _)| (!p.to_ascii_lowercase().contains("http"), p.len()));
        let paths: Vec<&str> = out.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, vec!["http_api.api_key", "api_key"]);
    }

    #[test]
    fn only_local_urls() {
        assert!(is_local("http://127.0.0.1:8100"));
        assert!(is_local("http://localhost:8100"));
        assert!(!is_local("http://192.168.178.25:8101"));
    }
}
