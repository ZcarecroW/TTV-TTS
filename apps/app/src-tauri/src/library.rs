//! Computed preset voices. The engine that computes them runs on TTV-TTS
//! Proxy (remote) or inside this app when VCP runs on this PC (local). The
//! app keeps a copy of the library in memory – the reader looks voices up
//! per message – and patches it from change events instead of refetching.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::RwLock;
use serde::Serialize;
use tauri::Emitter;
use ttv_link::{EngineStatus, JobRequest, JobView, LibraryChange, LibraryEntry, LibrarySnapshot};
use ttv_vcp::Method;

use crate::config::HostConfig;
use crate::state::AppState;

const T_SHORT: Duration = Duration::from_secs(10);
const T_LONG: Duration = Duration::from_secs(180);

#[derive(Default)]
struct Cache {
    /// Which engine the copy came from ("proxy:<id>" or "local").
    source: String,
    rev: Option<u64>,
    map: HashMap<String, LibraryEntry>,
    /// def hash -> languages computed (for counts per preset).
    by_def: HashMap<String, Vec<String>>,
    /// def hash -> languages whose voice the quality check lists.
    odd: HashMap<String, Vec<String>>,
}

/// Listed by the quality check: may sound odd, and nobody kept it yet.
pub fn is_odd(e: &LibraryEntry) -> bool {
    e.quality.as_ref().is_some_and(|q| q.score < ttv_engine::quality::FLAG && !q.kept)
}

impl Cache {
    fn index(&mut self) {
        self.by_def.clear();
        self.odd.clear();
        for e in self.map.values() {
            self.by_def.entry(e.def_hash.clone()).or_default().push(e.lang.clone());
            if is_odd(e) {
                self.odd.entry(e.def_hash.clone()).or_default().push(e.lang.clone());
            }
        }
    }

    fn add(&mut self, e: LibraryEntry) {
        if self.map.contains_key(&e.key) {
            self.remove(&e.key.clone());
        }
        let langs = self.by_def.entry(e.def_hash.clone()).or_default();
        if !langs.contains(&e.lang) {
            langs.push(e.lang.clone());
        }
        if is_odd(&e) {
            self.odd.entry(e.def_hash.clone()).or_default().push(e.lang.clone());
        }
        self.map.insert(e.key.clone(), e);
    }

    fn remove(&mut self, key: &str) {
        if let Some(e) = self.map.remove(key) {
            if let Some(l) = self.by_def.get_mut(&e.def_hash) {
                l.retain(|x| *x != e.lang);
            }
            if let Some(l) = self.odd.get_mut(&e.def_hash) {
                l.retain(|x| *x != e.lang);
            }
        }
    }
}

/// What the window's preset views need from the library.
pub struct Counts<'a> {
    pub by_def: &'a HashMap<String, Vec<String>>,
    pub odd: &'a HashMap<String, Vec<String>>,
}

/// What the window gets on `app://library`.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryNotice {
    pub rev: u64,
    pub size: u32,
}

pub struct VoiceLibrary {
    /// Used when VCP runs on this PC (direct mode).
    pub local: ttv_engine::Engine,
    local_dir: std::path::PathBuf,
    cache: RwLock<Cache>,
    loading: AtomicBool,
}

enum Backend {
    Remote(ttv_vcp::Client),
    Local,
}

impl VoiceLibrary {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let activity: ttv_engine::Activity = Arc::new(crate::speech::last_vcp_use);
        Self { local: ttv_engine::Engine::new(data_dir.join("engine"), activity), local_dir: data_dir.join("engine"), cache: RwLock::new(Cache::default()), loading: AtomicBool::new(false) }
    }

    fn source(state: &AppState) -> String {
        match &state.cfg.read().host {
            HostConfig::Proxy { id, .. } => format!("proxy:{id}"),
            HostConfig::Direct { .. } => "local".into(),
            HostConfig::None => String::new(),
        }
    }

    fn backend(state: &AppState) -> Result<Backend, String> {
        let direct = matches!(state.cfg.read().host, HostConfig::Direct { .. });
        if direct {
            return Ok(Backend::Local);
        }
        match state.vcp() {
            Some(c) => Ok(Backend::Remote(c)),
            None => Err("No voice host set up yet.".into()),
        }
    }

    /// Called after the host setting changed.
    pub fn host_changed(&self, state: &AppState) {
        let direct = matches!(state.cfg.read().host, HostConfig::Direct { .. });
        self.local.set_vcp(if direct { state.vcp() } else { None });
        let src = Self::source(state);
        let mut c = self.cache.write();
        if c.source != src {
            *c = Cache { source: src, ..Default::default() };
        }
    }

    // ── the in-memory copy ───────────────────────────────────────────────

    /// Looks a computed voice up (per message, cheap).
    pub fn get(&self, key: &str) -> Option<LibraryEntry> {
        self.cache.read().map.get(key).cloned()
    }

    /// Runs `f` with the copy (no cloning of the whole library).
    pub fn with<R>(&self, f: impl FnOnce(&HashMap<String, LibraryEntry>, &HashMap<String, Vec<String>>) -> R) -> R {
        let c = self.cache.read();
        f(&c.map, &c.by_def)
    }

    /// Runs `f` with the counts per definition (computed and odd languages).
    pub fn counts<R>(&self, f: impl FnOnce(Counts) -> R) -> R {
        let c = self.cache.read();
        f(Counts { by_def: &c.by_def, odd: &c.odd })
    }

    pub fn size(&self) -> u32 {
        self.cache.read().map.len() as u32
    }

    pub fn loaded(&self) -> bool {
        self.cache.read().rev.is_some()
    }

    /// Fetches the whole library (on connect, or after missing a change).
    pub async fn refresh(&self, state: &Arc<AppState>) -> Result<(), String> {
        if self.loading.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let r = async {
            let src = Self::source(state);
            let snap: LibrarySnapshot = match Self::backend(state)? {
                Backend::Local => self.local.library(),
                Backend::Remote(c) => c.get("/ttv/v1/library", T_LONG).await.map_err(|e| e.to_string())?,
            };
            let mut c = self.cache.write();
            c.source = src;
            c.rev = Some(snap.rev);
            c.map = snap.entries.into_iter().map(|e| (e.key.clone(), e)).collect();
            c.index();
            Ok::<_, String>(())
        }
        .await;
        self.loading.store(false, Ordering::SeqCst);
        if r.is_ok() {
            log::info!("library: {} computed voices", self.size());
            self.notify(state);
        }
        r
    }

    /// Applies a change event; refetches if it doesn't fit the copy.
    pub fn on_change(&self, state: &Arc<AppState>, ch: LibraryChange) {
        let fits = {
            let mut c = self.cache.write();
            if c.rev == Some(ch.base) {
                for k in &ch.removed {
                    c.remove(k);
                }
                for e in ch.added {
                    c.add(e);
                }
                c.rev = Some(ch.rev);
                true
            } else {
                false
            }
        };
        if fits {
            self.notify(state);
        } else {
            let st = state.clone();
            tauri::async_runtime::spawn(async move {
                let _ = st.library.refresh(&st).await;
            });
        }
    }

    fn notify(&self, state: &AppState) {
        let c = self.cache.read();
        if let Some(app) = state.app() {
            let _ = app.emit("app://library", LibraryNotice { rev: c.rev.unwrap_or(0), size: c.map.len() as u32 });
        }
    }

    pub fn on_job(state: &AppState, job: &JobView) {
        if let Some(app) = state.app() {
            let _ = app.emit("app://job", job);
        }
    }

    // ── engine calls ─────────────────────────────────────────────────────

    pub async fn status(&self, state: &AppState) -> Result<EngineStatus, String> {
        match Self::backend(state)? {
            Backend::Local => Ok(self.local.status()),
            Backend::Remote(c) => c.get("/ttv/v1/engine", T_SHORT).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn submit(&self, state: &AppState, req: JobRequest) -> Result<JobView, String> {
        match Self::backend(state)? {
            Backend::Local => self.local.submit(req),
            Backend::Remote(c) => c.post("/ttv/v1/jobs", &req, T_SHORT).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn control(&self, state: &AppState, id: &str, action: &str) -> Result<(), String> {
        match Self::backend(state)? {
            Backend::Local => self.local.control(id, action),
            Backend::Remote(c) => c.post::<_, serde_json::Value>(&format!("/ttv/v1/jobs/{id}/{action}"), &(), T_SHORT).await.map(|_| ()).map_err(|e| e.to_string()),
        }
    }

    pub async fn delete(&self, state: &AppState, keys: Vec<String>) -> Result<u32, String> {
        match Self::backend(state)? {
            Backend::Local => self.local.delete(&keys).await,
            Backend::Remote(c) => {
                let v: serde_json::Value = c.post("/ttv/v1/library/delete", &serde_json::json!({ "keys": keys }), T_LONG).await.map_err(|e| e.to_string())?;
                Ok(v["deleted"].as_u64().unwrap_or(0) as u32)
            }
        }
    }

    pub async fn adopt(&self, state: &AppState, entries: Vec<LibraryEntry>) -> Result<u32, String> {
        match Self::backend(state)? {
            Backend::Local => Ok(self.local.adopt(entries)),
            Backend::Remote(c) => {
                let v: serde_json::Value = c.post("/ttv/v1/library/adopt", &entries, T_LONG).await.map_err(|e| e.to_string())?;
                Ok(v["adopted"].as_u64().unwrap_or(0) as u32)
            }
        }
    }

    /// Replaces entries by re-keyed ones for the same VCP voices (nothing is
    /// deleted in VCP). Needs TTV-TTS Proxy 2.0.0-alpha.9 or newer.
    pub async fn rekey(&self, state: &AppState, entries: Vec<LibraryEntry>, remove: Vec<String>) -> Result<u32, String> {
        match Self::backend(state)? {
            Backend::Local => Ok(self.local.rekey(entries, &remove)),
            Backend::Remote(c) => {
                let v: serde_json::Value = c.post("/ttv/v1/library/adopt", &serde_json::json!({ "entries": entries, "remove": remove }), T_LONG).await.map_err(|e| e.to_string())?;
                Ok(v["adopted"].as_u64().unwrap_or(0) as u32)
            }
        }
    }

    /// Builds a voice pack on the engine's PC and saves it at `to`. Returns its size.
    pub async fn export_pack(&self, state: &AppState, req: ttv_link::ExportRequest, to: &std::path::Path) -> Result<u64, String> {
        use tokio::io::AsyncWriteExt;
        match Self::backend(state)? {
            Backend::Local => {
                let made = self.local.export(req, env!("CARGO_PKG_VERSION")).await?;
                let n = std::fs::copy(&made, to).map_err(|e| e.to_string())?;
                let _ = std::fs::remove_file(made);
                Ok(n)
            }
            Backend::Remote(c) => {
                let mut r = c.request(Method::POST, "/ttv/v1/packs/export").json(&req).timeout(Duration::from_secs(3600)).send().await.map_err(|e| e.to_string())?;
                if !r.status().is_success() {
                    let body = r.text().await.unwrap_or_default();
                    let msg = serde_json::from_str::<serde_json::Value>(&body).ok().and_then(|v| v["message"].as_str().map(str::to_string));
                    return Err(msg.unwrap_or_else(|| format!("The proxy couldn't build the pack ({body}).")));
                }
                let part = to.with_extension("part");
                let mut f = tokio::fs::File::create(&part).await.map_err(|e| e.to_string())?;
                let mut n = 0u64;
                while let Some(chunk) = r.chunk().await.map_err(|e| e.to_string())? {
                    n += chunk.len() as u64;
                    f.write_all(&chunk).await.map_err(|e| e.to_string())?;
                }
                f.sync_all().await.map_err(|e| e.to_string())?;
                drop(f);
                tokio::fs::rename(&part, to).await.map_err(|e| e.to_string())?;
                Ok(n)
            }
        }
    }

    /// Sends a pack to the engine and queues its import.
    pub async fn import_pack(&self, state: &AppState, path: &std::path::Path) -> Result<JobView, String> {
        match Self::backend(state)? {
            Backend::Local => {
                // a copy: the user may move or delete the file meanwhile
                let dir = self.local_dir.join("packs");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let copy = dir.join(format!("incoming-{:08x}.{}", rand::random::<u32>(), ttv_engine::pack::EXTENSION));
                std::fs::copy(path, &copy).map_err(|e| e.to_string())?;
                let job = self.local.submit(JobRequest { kind: ttv_link::JobKind::Import, pack: copy.to_string_lossy().into(), ..Default::default() });
                if job.as_ref().map_or(true, |j| j.total == 0) {
                    let _ = std::fs::remove_file(&copy);
                }
                job
            }
            Backend::Remote(c) => {
                let file = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
                let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
                let stream = futures_util::stream::unfold(file, |mut f| async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = vec![0u8; 256 * 1024];
                    match f.read(&mut buf).await {
                        Ok(0) => None,
                        Ok(n) => {
                            buf.truncate(n);
                            Some((Ok::<_, std::io::Error>(buf), f))
                        }
                        Err(e) => Some((Err(e), f)),
                    }
                });
                let r = c
                    .request(Method::POST, "/ttv/v1/packs/import")
                    .header(reqwest::header::CONTENT_LENGTH, len)
                    .header(reqwest::header::CONTENT_TYPE, "application/zip")
                    .body(reqwest::Body::wrap_stream(stream))
                    .timeout(Duration::from_secs(3600))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                let status = r.status();
                let body = r.text().await.map_err(|e| e.to_string())?;
                if !status.is_success() {
                    let msg = serde_json::from_str::<serde_json::Value>(&body).ok().and_then(|v| v["message"].as_str().map(str::to_string));
                    return Err(msg.unwrap_or_else(|| if status == reqwest::StatusCode::NOT_FOUND { "The proxy is too old for voice packs – update it on the Voice host page.".into() } else { format!("The proxy refused the pack ({status}).") }));
                }
                serde_json::from_str(&body).map_err(|e| e.to_string())
            }
        }
    }

    /// Marks voices as listened to and fine (or not).
    pub async fn keep(&self, state: &AppState, keys: Vec<String>, kept: bool) -> Result<u32, String> {
        match Self::backend(state)? {
            Backend::Local => Ok(self.local.keep(&keys, kept)),
            Backend::Remote(c) => {
                let v: serde_json::Value = c.post("/ttv/v1/library/keep", &serde_json::json!({ "keys": keys, "kept": kept }), T_SHORT).await.map_err(|e| e.to_string())?;
                Ok(v["changed"].as_u64().unwrap_or(0) as u32)
            }
        }
    }

    pub async fn verify(&self, state: &AppState) -> Result<ttv_link::Verified, String> {
        match Self::backend(state)? {
            Backend::Local => self.local.verify().await,
            // Older proxies answer only {"removed": n}; the rest stays 0.
            // Measuring older voices downloads each reference sample once.
            Backend::Remote(c) => c.post("/ttv/v1/library/verify", &(), Duration::from_secs(1800)).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn estimate(&self, state: &AppState, voices: u32, takes: u8, check: bool) -> Result<Option<u64>, String> {
        match Self::backend(state)? {
            Backend::Local => Ok(self.local.estimate(voices, takes, check)),
            Backend::Remote(c) => {
                let v: serde_json::Value = c.get(&format!("/ttv/v1/engine/estimate?voices={voices}&takes={takes}&check={check}"), T_SHORT).await.map_err(|e| e.to_string())?;
                Ok(v["secs"].as_u64())
            }
        }
    }

    pub async fn benchmark(&self, state: &AppState) -> Result<f32, String> {
        match Self::backend(state)? {
            Backend::Local => self.local.benchmark().await,
            Backend::Remote(c) => {
                let v: serde_json::Value = c.post("/ttv/v1/engine/benchmark", &(), Duration::from_secs(600)).await.map_err(|e| e.to_string())?;
                v["design_secs"].as_f64().map(|d| d as f32).ok_or_else(|| "The proxy sent no measurement.".into())
            }
        }
    }

    pub async fn bench_start(&self, state: &AppState, req: ttv_link::BenchRequest) -> Result<(), String> {
        match Self::backend(state)? {
            // VCP on this PC: nobody here can restart it
            Backend::Local => self.local.bench_start(req, Default::default()),
            Backend::Remote(c) => c.post::<_, serde_json::Value>("/ttv/v1/engine/bench", &req, T_SHORT).await.map(|_| ()).map_err(|e| e.to_string()),
        }
    }

    pub async fn bench_stop(&self, state: &AppState) -> Result<(), String> {
        match Self::backend(state)? {
            Backend::Local => {
                self.local.bench_stop();
                Ok(())
            }
            Backend::Remote(c) => c.post::<_, serde_json::Value>("/ttv/v1/engine/bench/stop", &(), T_SHORT).await.map(|_| ()).map_err(|e| e.to_string()),
        }
    }

    pub async fn bench_history(&self, state: &AppState) -> Result<ttv_link::BenchHistory, String> {
        match Self::backend(state)? {
            Backend::Local => Ok(self.local.bench_history()),
            Backend::Remote(c) => c.get("/ttv/v1/engine/bench", T_SHORT).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn plan_start(&self, state: &AppState, req: ttv_link::PlanRequest) -> Result<ttv_link::PlanView, String> {
        match Self::backend(state)? {
            Backend::Local => self.local.plan_start(req),
            Backend::Remote(c) => c.post("/ttv/v1/plan", &req, T_LONG).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn plan_control(&self, state: &AppState, action: &str) -> Result<(), String> {
        match Self::backend(state)? {
            Backend::Local => self.local.plan_control(action),
            Backend::Remote(c) => c.post::<_, serde_json::Value>(&format!("/ttv/v1/plan/{action}"), &(), T_SHORT).await.map(|_| ()).map_err(|e| e.to_string()),
        }
    }

    /// Settings of TTV-TTS Proxy; None with VCP on this PC.
    pub async fn vcp_settings(&self, state: &AppState) -> Result<Option<ttv_link::VcpSettings>, String> {
        match Self::backend(state)? {
            Backend::Local => Ok(None),
            Backend::Remote(c) => c.get("/ttv/v1/vcp/settings", T_SHORT).await.map(Some).map_err(|e| e.to_string()),
        }
    }

    pub async fn set_vcp_settings(&self, state: &AppState, s: ttv_link::VcpSettings) -> Result<(), String> {
        match Self::backend(state)? {
            Backend::Local => Err("Only TTV-TTS Proxy can restart Voice Creator Pro.".into()),
            Backend::Remote(c) => c.put::<_, serde_json::Value>("/ttv/v1/vcp/settings", &s, T_SHORT).await.map(|_| ()).map_err(|e| e.to_string()),
        }
    }

    /// The lossless backup of a voice's reference sample (FLAC); used by pack export.
    #[allow(dead_code)]
    pub async fn takes(&self, state: &AppState) -> Result<Vec<ttv_link::TakeChoice>, String> {
        match Self::backend(state)? {
            Backend::Local => Ok(self.local.takes()),
            // an older proxy has no takes
            Backend::Remote(c) => Ok(c.get("/ttv/v1/takes", T_SHORT).await.unwrap_or_default()),
        }
    }

    pub async fn take_wav(&self, state: &AppState, key: &str, n: u32) -> Result<Vec<u8>, String> {
        match Self::backend(state)? {
            Backend::Local => self.local.take_wav(key, n).ok_or_else(|| "This take doesn't exist.".into()),
            Backend::Remote(c) => {
                let r = c.request(Method::GET, &format!("/ttv/v1/takes/{key}/{n}")).timeout(T_LONG).send().await.map_err(|e| e.to_string())?;
                if !r.status().is_success() {
                    return Err("This take doesn't exist.".into());
                }
                Ok(r.bytes().await.map_err(|e| e.to_string())?.to_vec())
            }
        }
    }

    pub async fn choose_take(&self, state: &AppState, key: &str, n: u32) -> Result<ttv_link::LibraryEntry, String> {
        match Self::backend(state)? {
            Backend::Local => self.local.choose_take(key, n).await,
            Backend::Remote(c) => c.post(&format!("/ttv/v1/takes/{key}/choose"), &serde_json::json!({ "n": n }), Duration::from_secs(300)).await.map_err(|e| e.to_string()),
        }
    }

    pub async fn discard_takes(&self, state: &AppState, key: &str) -> Result<(), String> {
        match Self::backend(state)? {
            Backend::Local => {
                self.local.discard_takes(key);
                Ok(())
            }
            Backend::Remote(c) => c.post::<_, ttv_link::Ack>(&format!("/ttv/v1/takes/{key}/discard"), &serde_json::json!({}), T_SHORT).await.map(|_| ()).map_err(|e| e.to_string()),
        }
    }
}

/// Forwards the local engine's events (direct mode) like the proxy's.
pub async fn local_events(state: Arc<AppState>) {
    let mut rx = state.library.local.subscribe();
    loop {
        let ev = match rx.recv().await {
            Ok(ev) => ev,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                let _ = state.library.refresh(&state).await;
                continue;
            }
            Err(_) => return,
        };
        if !matches!(state.cfg.read().host, HostConfig::Direct { .. }) {
            continue;
        }
        match ev {
            ttv_engine::EngineEvent::Job(j) => VoiceLibrary::on_job(&state, &j),
            ttv_engine::EngineEvent::Library(c) => state.library.on_change(&state, c),
            // VCP on this PC hangs: the app can't restart it, the job shows the error
            ttv_engine::EngineEvent::Stuck(why) => log::warn!("Voice Creator Pro hangs: {why}"),
            ttv_engine::EngineEvent::Slow(why) => log::warn!("Voice Creator Pro has slowed down: {why}"),
        }
    }
}
