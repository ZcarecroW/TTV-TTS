use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::types::*;

/// VCP works on one request at a time, and two arriving together can wedge
/// it until it is restarted (seen with a voice design in flight while a
/// clone came in over a second connection). Every work request of this
/// process takes turns here; the turn is held until the answer is read.
static TURN: LazyLock<Arc<tokio::sync::Mutex<()>>> = LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(())));

/// The benchmark has VCP to itself: other work is refused instead of waiting
/// minutes for its turn.
static EXCLUSIVE: AtomicBool = AtomicBool::new(false);

pub fn set_exclusive(on: bool) {
    EXCLUSIVE.store(on, Ordering::SeqCst);
}

pub fn exclusive() -> bool {
    EXCLUSIVE.load(Ordering::SeqCst)
}

type BeforeChat = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;
/// Work that has to happen before the next chat request reaches VCP.
static BEFORE_CHAT: std::sync::Mutex<Option<BeforeChat>> = std::sync::Mutex::new(None);

/// Sets (or clears) what the next chat request does first, while holding its
/// turn. Computing uses it to free the model it has loaded.
pub fn before_next_chat(work: Option<BeforeChat>) {
    *BEFORE_CHAT.lock().unwrap_or_else(|e| e.into_inner()) = work;
}

/// Chat requests waiting for their turn; background work lets them first.
static CHAT_WAITING: AtomicUsize = AtomicUsize::new(0);
/// When a chat request last gave its turn back (`now_ms`; 0: never).
static CHAT_DONE: AtomicU64 = AtomicU64::new(0);
static START: LazyLock<Instant> = LazyLock::new(Instant::now);
/// Milliseconds since START, from 1 (0 means "never" in CHAT_DONE: the
/// first chat of a run, right at START, used to look like no chat at all).
fn now_ms() -> u64 {
    START.elapsed().as_millis() as u64 + 1
}
/// Background work also waits this long after chat: the app sends a
/// message's next sentence right after the previous one came back.
const CHAT_PAUSE: Duration = Duration::from_millis(1500);

/// What VCP did last in this process: 0 nothing yet, 1 speaking with a
/// saved voice, 2 anything else (design, recognition, translation …), which
/// may have pushed the speaking model out of memory.
static LAST_KIND: AtomicU8 = AtomicU8::new(0);
const SPEAKING: u8 = 1;
const OTHER: u8 = 2;

fn kind_of(path: &str) -> u8 {
    if path.starts_with("/api/v1/generation/clone") || is_stream(path) { SPEAKING } else { OTHER }
}

/// The audio of a streaming session: VCP generates while it sends.
fn is_stream(path: &str) -> bool {
    path.starts_with("/api/v1/streaming/") && path.split('?').next().is_some_and(|p| p.ends_with("/audio"))
}

/// Holding it means VCP works for you; dropping it gives the turn back.
pub struct Turn {
    _guard: tokio::sync::OwnedMutexGuard<()>,
    chat: bool,
    kind: u8,
    after_speaking: bool,
}

impl Turn {
    /// Speaking right after speaking: VCP didn't have to switch models, so
    /// this request tells how fast VCP really is (see `perf`).
    pub fn measures_speed(&self) -> bool {
        self.kind == SPEAKING && self.after_speaking
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        LAST_KIND.store(self.kind, Ordering::Relaxed);
        if self.chat {
            CHAT_DONE.store(now_ms(), Ordering::Relaxed);
        }
    }
}

fn granted(guard: tokio::sync::OwnedMutexGuard<()>, chat: bool, path: &str) -> Turn {
    Turn { _guard: guard, chat, kind: kind_of(path), after_speaking: LAST_KIND.load(Ordering::Relaxed) == SPEAKING }
}

/// Waits for this process's turn at VCP to do `path`. Chat (speech,
/// previews, guests) goes in order of arrival and before background work.
pub async fn turn(path: &str) -> Turn {
    /// Counts this request as waiting until it has its turn or gives up: a
    /// request dropped while it waited (its client timed out, the connection
    /// closed) used to leave the count up for good, and computing waited
    /// for that chat forever.
    struct Waiting;
    impl Drop for Waiting {
        fn drop(&mut self) {
            CHAT_WAITING.fetch_sub(1, Ordering::SeqCst);
        }
    }
    CHAT_WAITING.fetch_add(1, Ordering::SeqCst);
    let waiting = Waiting;
    let guard = TURN.clone().lock_owned().await;
    drop(waiting);
    let before = BEFORE_CHAT.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(work) = before {
        work.await;
    }
    granted(guard, true, path)
}

/// The turn for background work (computing jobs): it steps aside while chat
/// waits or has just been served, so a job request that was queued first
/// doesn't make VCP switch models between two chat messages.
pub async fn background_turn(path: &str) -> Turn {
    loop {
        let guard = TURN.clone().lock_owned().await;
        let since_chat = now_ms().saturating_sub(CHAT_DONE.load(Ordering::Relaxed));
        let chat_done = CHAT_DONE.load(Ordering::Relaxed);
        if CHAT_WAITING.load(Ordering::SeqCst) == 0 && (chat_done == 0 || since_chat >= CHAT_PAUSE.as_millis() as u64) {
            return granted(guard, false, path);
        }
        drop(guard);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Requests that make VCP work: generating, designing, transcribing,
/// saving voices. Stopping and shutting down never wait for a turn.
pub fn is_work(method: &Method, path: &str) -> bool {
    (*method == Method::POST && path.starts_with("/api/v1/") && !path.starts_with("/api/v1/generation/stop") && !path.starts_with("/api/v1/app/"))
        || (*method == Method::GET && is_stream(path))
}

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
    /// Work of a computing job: chat goes first (see `background_turn`).
    background: bool,
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
            background: false,
        }
    }

    /// The same client for background work: its requests let chat go first.
    pub fn background(&self) -> Self {
        Self { background: true, ..self.clone() }
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
        let bytes = self.send_bytes(rb).await?;
        serde_json::from_slice(&bytes).map_err(|e| Error::Decode(format!("{e}: {}", preview(&bytes))))
    }

    /// The answer's body, or VCP's error.
    async fn send_bytes(&self, rb: reqwest::RequestBuilder) -> Result<bytes::Bytes, Error> {
        let res = rb.send().await.map_err(Error::from_reqwest)?;
        let status = res.status();
        let bytes = res.bytes().await.map_err(Error::from_reqwest)?;
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(Error::Unauthorized);
        }
        if !status.is_success() {
            return Err(Error::Http { status: status.as_u16(), message: error_message(&bytes) });
        }
        Ok(bytes)
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
        // the timeout starts once it is this request's turn
        let turn = match (is_work(&Method::POST, path), self.background) {
            (false, _) => None,
            (true, false) => Some(turn(path).await),
            (true, true) => Some(background_turn(path).await),
        };
        let t0 = Instant::now();
        let bytes = self.send_bytes(self.request(Method::POST, path).json(body).timeout(timeout)).await?;
        // chat and previews tell how fast VCP is; computing jobs don't (their
        // first sentence with a new voice does more work)
        if !self.background
            && turn.as_ref().is_some_and(Turn::measures_speed)
            && let Some(audio) = crate::perf::audio_secs(&bytes[..bytes.len().min(1024)], &bytes[bytes.len().saturating_sub(512)..], bytes.len())
        {
            crate::perf::record(t0.elapsed().as_secs_f32(), audio);
        }
        drop(turn);
        serde_json::from_slice(&bytes).map_err(|e| Error::Decode(format!("{e}: {}", preview(&bytes))))
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

    /// The voice's reference sample from `GET /api/v1/voices/{id}`.
    pub async fn voice_sample(&self, id: &str) -> Result<VoiceSample, Error> {
        let v: VoiceSample = self.get(&format!("/api/v1/voices/{id}"), Duration::from_secs(30)).await?;
        if v.audio_base64.is_empty() {
            return Err(Error::Decode("voice without audio".into()));
        }
        Ok(v)
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
