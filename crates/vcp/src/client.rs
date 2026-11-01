use std::time::Duration;

use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::types::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// VCP could not be reached at all (not running, wrong port, firewall).
    #[error("VCP is not reachable: {0}")]
    Unreachable(String),
    /// VCP answered 401/403.
    #[error("VCP rejected the API key")]
    Unauthorized,
    /// Any other HTTP error, with VCP's own message.
    #[error("VCP answered HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("unexpected answer from VCP: {0}")]
    Decode(String),
    #[error("request to VCP timed out")]
    Timeout,
}

impl Error {
    fn from_reqwest(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Error::Timeout
        } else if e.is_connect() || e.is_request() {
            Error::Unreachable(root_cause(&e))
        } else if e.is_decode() {
            Error::Decode(e.to_string())
        } else {
            Error::Unreachable(root_cause(&e))
        }
    }

    /// HTTP status of an error answer, if VCP answered at all.
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::Unauthorized => Some(401),
            Error::Http { status, .. } => Some(*status),
            _ => None,
        }
    }
}

fn root_cause(e: &(dyn std::error::Error + 'static)) -> String {
    let mut cur: &dyn std::error::Error = e;
    while let Some(next) = cur.source() {
        cur = next;
    }
    cur.to_string()
}

/// Client for one VCP instance. Cheap to clone (shares the connection pool).
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    key: Option<String>,
}

impl Client {
    /// `base` like `http://127.0.0.1:8100`; `key` = VCP's API key, if one is set.
    pub fn new(base: &str, key: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .pool_idle_timeout(Duration::from_secs(60))
            // VCP is a local uvicorn server: HTTP/1.1, no compression worth negotiating.
            .http1_only()
            .no_proxy()
            .build()
            .expect("reqwest client");
        Self::with_http(http, base, key)
    }

    pub fn with_http(http: reqwest::Client, base: &str, key: Option<String>) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            key: key.filter(|k| !k.is_empty()),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    /// A request builder with the API key attached.
    pub fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let rb = self.http.request(method, self.url(path));
        match &self.key {
            Some(k) => rb.header("X-API-Key", k),
            None => rb,
        }
    }

    async fn send<T: DeserializeOwned>(&self, rb: reqwest::RequestBuilder) -> Result<T, Error> {
        let res = rb.send().await.map_err(Error::from_reqwest)?;
        let status = res.status();
        let bytes = res.bytes().await.map_err(Error::from_reqwest)?;
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(Error::Unauthorized);
        }
        if !status.is_success() {
            return Err(Error::Http { status: status.as_u16(), message: error_message(&bytes) });
        }
        serde_json::from_slice(&bytes).map_err(|e| Error::Decode(format!("{e}: {}", preview(&bytes))))
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str, timeout: Duration) -> Result<T, Error> {
        self.send(self.request(Method::GET, path).timeout(timeout)).await
    }

    pub async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        timeout: Duration,
    ) -> Result<T, Error> {
        self.send(self.request(Method::POST, path).json(body).timeout(timeout)).await
    }

    pub async fn put<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        timeout: Duration,
    ) -> Result<T, Error> {
        self.send(self.request(Method::PUT, path).json(body).timeout(timeout)).await
    }

    // ── typed endpoints ────────────────────────────────────────────────────

    /// `GET /api/v1/health` (no key needed). Ok = VCP's HTTP API is up.
    pub async fn health(&self) -> Result<(), Error> {
        let _: serde_json::Value = self.get("/api/v1/health", Duration::from_secs(3)).await?;
        Ok(())
    }

    /// `GET /api/v1/system/version` (no key needed), e.g. "2.4.12.0".
    pub async fn version(&self) -> Result<String, Error> {
        let v: VersionResponse = self.get("/api/v1/system/version", Duration::from_secs(3)).await?;
        Ok(v.version)
    }

    /// Checks that the key is accepted (cheapest endpoint that needs one).
    pub async fn check_key(&self) -> Result<(), Error> {
        let _: GenerationQueue = self.generation_queue().await?;
        Ok(())
    }

    pub async fn generation_queue(&self) -> Result<GenerationQueue, Error> {
        self.get("/api/v1/generation/queue", Duration::from_secs(5)).await
    }

    pub async fn generation_progress(&self) -> Result<GenerationProgress, Error> {
        self.get("/api/v1/generation/progress", Duration::from_secs(5)).await
    }

    pub async fn model_status(&self) -> Result<ModelStatus, Error> {
        self.get("/api/v1/model/status", Duration::from_secs(5)).await
    }

    pub async fn active_model(&self) -> Result<ModelCapabilities, Error> {
        self.get("/api/v1/models/active", Duration::from_secs(10)).await
    }

    pub async fn models(&self) -> Result<Vec<ModelCapabilities>, Error> {
        let r: ModelList = self.get("/api/v1/models", Duration::from_secs(10)).await?;
        Ok(r.models)
    }

    pub async fn languages(&self) -> Result<Vec<Language>, Error> {
        let r: LanguageList = self.get("/api/v1/tts/languages", Duration::from_secs(10)).await?;
        Ok(r.languages)
    }

    pub async fn voices(&self) -> Result<Vec<Voice>, Error> {
        let r: VoiceList = self.get("/api/v1/voices", Duration::from_secs(20)).await?;
        Ok(r.voices)
    }

    pub async fn system_info(&self) -> Result<String, Error> {
        let r: SystemInfo = self.get("/api/v1/system/info", Duration::from_secs(5)).await?;
        Ok(r.info)
    }

    pub async fn gpu_devices(&self) -> Result<GpuDevices, Error> {
        self.get("/api/v1/system/gpu-devices", Duration::from_secs(5)).await
    }

    pub async fn agent_settings(&self) -> Result<AgentSettings, Error> {
        self.get("/api/v1/settings/mcp", Duration::from_secs(5)).await
    }

    pub async fn http_api_settings(&self) -> Result<HttpApiSettings, Error> {
        self.get("/api/v1/settings/http-api", Duration::from_secs(5)).await
    }

    pub async fn model_quality(&self) -> Result<ModelQuality, Error> {
        self.get("/api/v1/settings/model-quality", Duration::from_secs(5)).await
    }

    pub async fn emotions(&self) -> Result<EmotionSettings, Error> {
        self.get("/api/v1/settings/emotional-clone-engine", Duration::from_secs(5)).await
    }

    /// `PUT /api/v1/settings/api-key`: sets VCP's API key (min. 8 characters),
    /// or clears it with `None`. Applies immediately.
    pub async fn set_api_key(&self, key: Option<&str>) -> Result<(), Error> {
        let mut rb = self.request(Method::PUT, "/api/v1/settings/api-key").timeout(Duration::from_secs(10));
        if let Some(k) = key {
            rb = rb.query(&[("key", k)]);
        }
        let _: serde_json::Value = self.send(rb).await?;
        Ok(())
    }

    /// The voice's reference sample (base64 WAV) from `GET /api/v1/voices/{id}`.
    pub async fn voice_audio(&self, id: &str) -> Result<String, Error> {
        #[derive(serde::Deserialize)]
        struct Audio {
            #[serde(default)]
            audio_base64: String,
        }
        let v: Audio = self.get(&format!("/api/v1/voices/{id}"), Duration::from_secs(30)).await?;
        if v.audio_base64.is_empty() {
            return Err(Error::Decode("voice without audio".into()));
        }
        Ok(v.audio_base64)
    }

    /// `DELETE /api/v1/voices/{id}`. A voice that is already gone counts as deleted.
    pub async fn delete_voice(&self, id: &str) -> Result<(), Error> {
        let res = self.request(Method::DELETE, &format!("/api/v1/voices/{id}")).timeout(Duration::from_secs(20)).send().await.map_err(Error::from_reqwest)?;
        match res.status() {
            s if s.is_success() || s == StatusCode::NOT_FOUND => Ok(()),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(Error::Unauthorized),
            s => Err(Error::Http { status: s.as_u16(), message: error_message(&res.bytes().await.unwrap_or_default()) }),
        }
    }

    /// `POST /api/v1/app/shutdown`: closes VCP.
    pub async fn shutdown(&self) -> Result<(), Error> {
        let _: serde_json::Value = self.post("/api/v1/app/shutdown", &serde_json::json!({}), Duration::from_secs(10)).await?;
        Ok(())
    }

    /// `POST /api/v1/generation/stop`.
    pub async fn stop_generation(&self) -> Result<(), Error> {
        let _: serde_json::Value = self.post("/api/v1/generation/stop", &serde_json::json!({}), Duration::from_secs(10)).await?;
        Ok(())
    }
}

/// FastAPI's `{"detail": "..."}` / `{"detail": [{msg}]}` / VCP's `{"error": "..."}`.
fn error_message(bytes: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) {
        let d = v.get("detail").or_else(|| v.get("error")).or_else(|| v.get("message"));
        match d {
            Some(serde_json::Value::String(s)) => return s.clone(),
            Some(serde_json::Value::Array(items)) => {
                return items
                    .iter()
                    .map(|i| i.get("msg").and_then(|m| m.as_str()).map(str::to_string).unwrap_or_else(|| i.to_string()))
                    .collect::<Vec<_>>()
                    .join("; ");
            }
            Some(other) => return other.to_string(),
            None => {}
        }
    }
    preview(bytes)
}

fn preview(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages() {
        assert_eq!(error_message(br#"{"detail":"No voice resolved"}"#), "No voice resolved");
        assert_eq!(error_message(br#"{"detail":[{"msg":"a"},{"msg":"b"}]}"#), "a; b");
        assert_eq!(error_message(br#"{"error":"Missing or invalid API key"}"#), "Missing or invalid API key");
        assert_eq!(error_message(b"plain"), "plain");
    }
}
