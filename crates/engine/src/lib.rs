//! The voice computation engine: turns preset definitions into ready VCP
//! voices. Runs on TTV-TTS Proxy (so it keeps going while the gaming PC is
//! off) or inside the app when VCP runs on the same PC.
//!
//! Work happens in batches and phases – design all takes of a batch, then
//! check them, then save and prime the winners – because Qwen3 swaps
//! checkpoints between designing and speaking (~20 s each time). Chat goes
//! first: before every VCP call the engine waits until nobody else used VCP
//! for a few seconds. Jobs and the library survive restarts.

mod audio;
pub mod langs;
mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, broadcast};
use ttv_link::{EngineStatus, JobRequest, JobState, JobView, LibraryChange, LibraryEntry, LibrarySnapshot, Verified};
use ttv_vcp::GenerationResult;

pub use store::Library;

/// Voices per batch (between two checkpoint swaps).
const BATCH: usize = 16;
/// The engine only calls VCP after this much quiet (chat goes first).
const GAP: Duration = Duration::from_secs(6);
/// Attempts per voice before it counts as failed.
const ATTEMPTS: u32 = 3;
/// Accepted speech-check error rate.
const MAX_WER: f32 = 0.15;
const MAX_WER_CJK: f32 = 0.25;

/// When somebody else (chat TTS, listening tests) last used VCP.
pub type Activity = Arc<dyn Fn() -> Option<Instant> + Send + Sync>;

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Job(JobView),
    Library(LibraryChange),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredJob {
    view: JobView,
    request: JobRequest,
    /// key -> attempts so far
    attempts: HashMap<String, u32>,
    /// keys that failed for good
    failed: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Stats {
    /// Measured seconds per voice (moving average over real jobs).
    secs_per_voice: Option<f32>,
    /// Measured seconds per design (benchmark), for estimates.
    design_secs: Option<f32>,
}

struct Inner {
    dir: PathBuf,
    vcp: RwLock<Option<ttv_vcp::Client>>,
    library: Library,
    jobs: Mutex<Vec<StoredJob>>,
    stats: Mutex<Stats>,
    events: broadcast::Sender<EngineEvent>,
    activity: Activity,
    wake: Notify,
    /// Counts wake-ups, so a stale Notify permit doesn't cut a backoff short.
    wakes: std::sync::atomic::AtomicU64,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl Engine {
    /// Loads state from `dir`. Spawn [`Engine::worker`] to start working.
    pub fn new(dir: PathBuf, activity: Activity) -> Self {
        let _ = std::fs::create_dir_all(dir.join("samples"));
        let mut jobs: Vec<StoredJob> = read_json(&dir.join("jobs.json")).unwrap_or_default();
        for j in &mut jobs {
            if matches!(j.view.state, JobState::Running | JobState::Waiting) {
                j.view.state = JobState::Queued;
            }
        }
        let inner = Arc::new(Inner {
            library: Library::load(dir.join("library.jsonl")),
            stats: Mutex::new(read_json(&dir.join("stats.json")).unwrap_or_default()),
            jobs: Mutex::new(jobs),
            dir,
            vcp: RwLock::new(None),
            events: broadcast::channel(256).0,
            activity,
            wake: Notify::new(),
            wakes: Default::default(),
        });
        Self { inner }
    }

    /// The worker loop; spawn it once on a tokio runtime.
    pub fn worker(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        worker(self.inner.clone())
    }

    pub fn set_vcp(&self, vcp: Option<ttv_vcp::Client>) {
        *self.inner.vcp.write() = vcp;
        self.inner.wake_up();
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.inner.events.subscribe()
    }

    pub fn status(&self) -> EngineStatus {
        EngineStatus {
            jobs: self.inner.jobs.lock().iter().map(|j| j.view.clone()).collect(),
            secs_per_voice: self.inner.stats.lock().secs_per_voice,
            library_size: self.inner.library.len() as u32,
            library_rev: self.inner.library.rev(),
        }
    }

    pub fn library(&self) -> LibrarySnapshot {
        self.inner.library.snapshot()
    }

    /// Queues a job for everything of `req` that isn't computed yet.
    pub fn submit(&self, req: JobRequest) -> Result<JobView, String> {
        if req.defs.is_empty() || req.languages.is_empty() {
            return Err("nothing to compute".into());
        }
        if let Some(l) = req.languages.iter().find(|l| langs::get(l).is_none()) {
            return Err(format!("language {l} can't be computed"));
        }
        let total = req.defs.iter().flat_map(|d| req.languages.iter().map(move |l| d.def.key(l))).filter(|k| !self.inner.library.contains(k)).count() as u32;
        let view = JobView {
            id: format!("job{}", rand::random::<u32>()),
            preset_id: req.preset_id.clone(),
            preset_name: req.preset_name.clone(),
            languages: req.languages.clone(),
            options: req.options.clone(),
            total,
            state: if total == 0 { JobState::Done } else { JobState::Queued },
            created: chrono::Utc::now().timestamp(),
            eta_secs: self.estimate(total, req.options.takes, req.options.speech_check),
            secs_per_voice: self.inner.stats.lock().secs_per_voice,
            ..Default::default()
        };
        self.inner.jobs.lock().push(StoredJob { view: view.clone(), request: req, ..Default::default() });
        self.inner.save_jobs();
        self.inner.emit_job(&view);
        self.inner.wake_up();
        log::info!("job {} queued: {} voices of {}", view.id, total, view.preset_name);
        Ok(view)
    }

    /// pause, resume, cancel or remove (finished jobs).
    pub fn control(&self, id: &str, action: &str) -> Result<(), String> {
        let mut jobs = self.inner.jobs.lock();
        let i = jobs.iter().position(|j| j.view.id == id).ok_or("unknown job")?;
        match action {
            "pause" => jobs[i].view.state = JobState::Paused,
            "resume" => jobs[i].view.state = JobState::Queued,
            "cancel" => {
                jobs[i].view.state = JobState::Cancelled;
                jobs[i].view.finished = Some(chrono::Utc::now().timestamp());
            }
            "remove" => {
                if matches!(jobs[i].view.state, JobState::Running | JobState::Waiting) {
                    return Err("stop the job first".into());
                }
                jobs.remove(i);
                drop(jobs);
                self.inner.save_jobs();
                return Ok(());
            }
            _ => return Err("unknown action".into()),
        }
        let v = jobs[i].view.clone();
        drop(jobs);
        self.inner.save_jobs();
        self.inner.emit_job(&v);
        self.inner.wake_up();
        Ok(())
    }

    /// Adds existing VCP voices to the library without computing them.
    pub fn adopt(&self, entries: Vec<LibraryEntry>) -> u32 {
        let new: Vec<LibraryEntry> = entries.into_iter().filter(|e| !self.inner.library.contains(&e.key)).collect();
        let n = new.len() as u32;
        self.inner.change(new, &[]);
        n
    }

    /// Deletes computed voices (in VCP too). Returns how many were removed.
    pub async fn delete(&self, keys: &[String]) -> Result<u32, String> {
        let vcp = self.inner.vcp.read().clone().ok_or("Voice Creator Pro is not connected")?;
        let mut gone = Vec::new();
        let mut failure = None;
        for k in keys {
            let Some(e) = self.inner.library.get(k) else { continue };
            if let Err(err) = vcp.delete_voice(&e.voice_id).await {
                failure = Some(format!("VCP refused to delete {}: {err}", e.voice_name));
                break;
            }
            let _ = std::fs::remove_file(self.inner.sample_path(k));
            gone.push(k.clone());
        }
        // What VCP already deleted leaves the library even if a later one failed.
        let n = gone.len() as u32;
        self.inner.change(vec![], &gone);
        match failure {
            Some(f) => Err(f),
            None => Ok(n),
        }
    }

    /// Checks the library against VCP: removes entries whose VCP voice no
    /// longer exists and measures the loudness of voices from older versions
    /// once (reference sample + how loud it spoke), correcting their gain.
    pub async fn verify(&self) -> Result<Verified, String> {
        let vcp = self.inner.vcp.read().clone().ok_or("Voice Creator Pro is not connected")?;
        let existing: std::collections::HashSet<String> = vcp.voices().await.map_err(|e| e.to_string())?.into_iter().map(|v| v.id).collect();
        let entries = self.inner.library.entries();
        let gone: Vec<String> = entries.iter().filter(|e| !existing.contains(&e.voice_id)).map(|e| e.key.clone()).collect();
        if !gone.is_empty() {
            log::info!("library: {} voices no longer exist in VCP", gone.len());
            self.inner.change(vec![], &gone);
        }
        let mut fixed = Vec::new();
        for e in entries.iter().filter(|e| e.level.is_none() && existing.contains(&e.voice_id)) {
            match reference_level(&vcp, &e.voice_id).await {
                Ok(db) => {
                    // How loud it spoke when it was primed: measured gains lie
                    // strictly inside the limits (1.0 = adopted, never measured).
                    let spoken = (e.gain > 0.41 && e.gain < 2.49 && e.gain != 1.0).then(|| -20.0 - 20.0 * e.gain.log10());
                    let gain = mix_levels(Some(db), spoken).map_or(e.gain, audio::gain_from_db);
                    fixed.push(LibraryEntry { gain, level: Some(db), ..e.clone() });
                }
                Err(err) => log::warn!("library: no reference sample for {}: {err}", e.voice_name),
            }
        }
        let report = Verified { checked: entries.len() as u32, removed: gone.len() as u32, measured: fixed.len() as u32 };
        if !fixed.is_empty() {
            log::info!("library: measured the loudness of {} voices", fixed.len());
            self.inner.change(fixed, &[]);
        }
        Ok(report)
    }

    /// Estimated seconds for `voices` voices, from earlier jobs or the
    /// benchmark; None if nothing was measured yet.
    pub fn estimate(&self, voices: u32, takes: u8, check: bool) -> Option<u64> {
        let s = self.inner.stats.lock().clone();
        let per = match (s.secs_per_voice, s.design_secs) {
            (Some(spv), _) => spv,
            // design + speech check per take, plus save + prime, plus the
            // checkpoint swaps spread over a batch.
            (None, Some(d)) => takes.max(1) as f32 * (d + if check { 1.5 } else { 0.0 }) + d * 0.9 + 60.0 / BATCH as f32,
            (None, None) => return None,
        };
        Some((voices as f32 * per).round() as u64)
    }

    /// Designs two short samples (discarded) to measure this PC's speed.
    /// Returns seconds per design.
    pub async fn benchmark(&self) -> Result<f32, String> {
        let vcp = self.inner.vcp.read().clone().ok_or("Voice Creator Pro is not connected")?;
        let l = langs::get("en").expect("en");
        let mut times = Vec::new();
        for desc in ["A calm man in his forties with a warm, clear baritone.", "A cheerful young woman with a bright, quick voice."] {
            let t = Instant::now();
            design(&vcp, desc, l).await?;
            times.push(t.elapsed().as_secs_f32());
        }
        // The first design may include loading the model: use the faster one.
        let d = times.iter().cloned().fold(f32::MAX, f32::min);
        self.inner.stats.lock().design_secs = Some(d);
        self.inner.save_stats();
        log::info!("benchmark: {d:.1} s per design");
        Ok(d)
    }

    /// Lossless backup of a voice's sample, if one was kept.
    pub fn sample(&self, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.inner.sample_path(key)).ok()
    }
}

impl Inner {
    fn wake_up(&self) {
        self.wakes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.wake.notify_one();
    }

    fn save_jobs(&self) {
        let jobs = self.jobs.lock().clone();
        write_json(&self.dir.join("jobs.json"), &jobs);
    }

    fn save_stats(&self) {
        let s = self.stats.lock().clone();
        write_json(&self.dir.join("stats.json"), &s);
    }

    fn emit_job(&self, v: &JobView) {
        let _ = self.events.send(EngineEvent::Job(v.clone()));
    }

    /// Changes the library and tells listeners what changed.
    fn change(&self, added: Vec<LibraryEntry>, removed: &[String]) {
        if let Some(c) = self.library.apply(added, removed) {
            let _ = self.events.send(EngineEvent::Library(c));
        }
    }

    fn sample_path(&self, key: &str) -> PathBuf {
        self.dir.join("samples").join(format!("{}.flac", key.replace(':', "_")))
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut StoredJob)) -> Option<JobView> {
        let mut jobs = self.jobs.lock();
        let j = jobs.iter_mut().find(|j| j.view.id == id)?;
        f(j);
        let v = j.view.clone();
        drop(jobs);
        self.emit_job(&v);
        Some(v)
    }

    /// One attempt for `key` didn't give a usable voice; after ATTEMPTS it
    /// counts as failed for good.
    fn attempt_failed(&self, id: &str, key: &str, why: Option<String>) {
        self.update(id, |j| {
            let a = j.attempts.entry(key.to_string()).or_insert(0);
            *a += 1;
            if *a >= ATTEMPTS {
                j.failed.push(key.to_string());
                j.view.failed += 1;
            }
            j.view.step += 1;
            if why.is_some() {
                j.view.error = why;
            }
        });
    }

    fn state_of(&self, id: &str) -> Option<JobState> {
        self.jobs.lock().iter().find(|j| j.view.id == id).map(|j| j.view.state)
    }

    /// Waits for a quiet moment. Returns false if the job was paused or
    /// cancelled meanwhile.
    async fn gap(&self, id: &str) -> bool {
        let mut waiting = false;
        loop {
            match self.state_of(id) {
                Some(JobState::Running | JobState::Waiting | JobState::Queued) => {}
                _ => return false,
            }
            let busy = (self.activity)().is_some_and(|t| t.elapsed() < GAP);
            if !busy {
                if waiting {
                    self.update(id, |j| j.view.state = JobState::Running);
                }
                return true;
            }
            if !waiting {
                waiting = true;
                self.update(id, |j| j.view.state = JobState::Waiting);
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_json<T: Serialize>(path: &std::path::Path, value: &T) {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, serde_json::to_vec(value).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// What VCP's voice designer gets: gender and age first, then the
/// description. Without them some "baritone" men came out at 280–370 Hz and
/// "low alto" women at ~120 Hz; naming them moved 3 of 4 such voices into
/// their range (measured on the VCP PC, 2026-11-01).
fn instruct(def: &ttv_link::VoiceDef) -> String {
    let who = match (def.gender.as_str(), def.age) {
        ("m", 0..13) => "boy",
        ("m", 13..18) => "teenage boy",
        ("m", _) => "man",
        ("f", 0..13) => "girl",
        ("f", 13..18) => "teenage girl",
        ("f", _) => "woman",
        // Neutral voices (creatures, machines) are fully described.
        _ => return def.description.trim().to_string(),
    };
    format!("A {}-year-old {who}. {}", def.age, def.description.trim())
}

async fn design(vcp: &ttv_vcp::Client, description: &str, l: &langs::DesignLang) -> Result<Vec<u8>, String> {
    let body = serde_json::json!({ "instruct_text": format!("{} {}", description.trim(), l.accent), "target_text": l.text, "language": l.vcp });
    let r: GenerationResult = vcp.post("/api/v1/generation/design?sync=true", &body, Duration::from_secs(600)).await.map_err(|e| e.to_string())?;
    decode_result(r)
}

fn decode_result(r: GenerationResult) -> Result<Vec<u8>, String> {
    if !r.success {
        return Err(r.error_detail.or(r.error_code).unwrap_or(r.status));
    }
    base64::engine::general_purpose::STANDARD.decode(r.audio_base64.ok_or("no audio")?.as_bytes()).map_err(|e| e.to_string())
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Transcript {
    success: bool,
    text: Option<String>,
    error: Option<String>,
}

/// Why a VCP call didn't give what it should.
enum Trouble {
    /// VCP didn't answer (restarting, gone): retry the batch later.
    Down(String),
    /// VCP answered, but it didn't work for this take or voice.
    Failed(String),
}

impl From<ttv_vcp::Error> for Trouble {
    fn from(e: ttv_vcp::Error) -> Self {
        match e {
            ttv_vcp::Error::Unreachable(_) | ttv_vcp::Error::Timeout | ttv_vcp::Error::Unauthorized => Trouble::Down(e.to_string()),
            other => Trouble::Failed(other.to_string()),
        }
    }
}

impl From<String> for Trouble {
    fn from(e: String) -> Self {
        Trouble::Failed(e)
    }
}

impl From<&str> for Trouble {
    fn from(e: &str) -> Self {
        Trouble::Failed(e.into())
    }
}

async fn transcribe(vcp: &ttv_vcp::Client, wav: &[u8], l: &langs::DesignLang) -> Result<String, Trouble> {
    let body = serde_json::json!({ "audio_base64": base64::engine::general_purpose::STANDARD.encode(wav), "language": l.vcp });
    let t: Transcript = vcp.post("/api/v1/transcription/start?sync=true", &body, Duration::from_secs(300)).await?;
    if !t.success {
        return Err(Trouble::Failed(t.error.unwrap_or_else(|| "speech recognition failed".into())));
    }
    Ok(t.text.unwrap_or_default())
}

/// One normal generation with a saved voice. Returns how loud it spoke
/// (dBFS), or why VCP couldn't speak with it.
async fn prime(vcp: &ttv_vcp::Client, voice_id: &str, l: &langs::DesignLang) -> Result<f32, Trouble> {
    let body = serde_json::json!({ "voice_id": voice_id, "target_text": l.verify, "language": l.vcp });
    let r: GenerationResult = vcp.post("/api/v1/generation/clone?sync=true", &body, Duration::from_secs(600)).await?;
    let (samples, rate) = audio::decode(&decode_result(r)?).ok_or("unreadable audio")?;
    if (samples.len() as f32) < rate as f32 * 0.8 {
        return Err("audio too short".into());
    }
    Ok(audio::loudness_db(&samples).ok_or("silent audio")?)
}

/// Speech loudness of a saved voice's reference sample (dBFS).
async fn reference_level(vcp: &ttv_vcp::Client, voice_id: &str) -> Result<f32, String> {
    let b64 = vcp.voice_audio(voice_id).await.map_err(|e| e.to_string())?;
    let wav = base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()).map_err(|e| e.to_string())?;
    let (samples, _) = audio::decode(&wav).ok_or("unreadable audio")?;
    audio::loudness_db(&samples).ok_or_else(|| "silent audio".into())
}

/// Expected speaking loudness of a voice from its reference sample and a
/// generation with it. Measured on 120 real voices: each alone predicts the
/// loudness of new sentences within ±1.2–1.4 dB (sd); the mean of both is
/// best (sd 1.16 dB, worst ±3.5 dB instead of ±6 dB for one short sentence).
fn mix_levels(reference: Option<f32>, spoken: Option<f32>) -> Option<f32> {
    match (reference, spoken) {
        (Some(r), Some(s)) => Some((r + s) / 2.0),
        (r, s) => r.or(s),
    }
}

/// Saves a take as a VCP voice and primes it: one normal generation makes
/// the voice ready for streaming (VCP streams a new voice only after that)
/// and tells how loud it speaks (returned in dBFS). If VCP can't speak with
/// the voice, the voice is deleted again – VCP never keeps voices the
/// library doesn't know.
///
/// (VCP's `has_voice_clone_prompt` stays false for a voice saved while the
/// speaking model isn't loaded – the first of each batch. Measured: such
/// voices stream and speak exactly as fast, so it isn't worth extra work.)
async fn save_voice(vcp: &ttv_vcp::Client, body: &serde_json::Value, l: &langs::DesignLang) -> Result<(ttv_vcp::Voice, f32), Trouble> {
    let saved: ttv_vcp::VoiceSaveResponse = vcp.post("/api/v1/voices", body, Duration::from_secs(60)).await?;
    let Some(voice) = saved.voice.filter(|_| saved.success) else {
        return Err(Trouble::Failed(saved.message.unwrap_or_else(|| "VCP did not save the voice".into())));
    };
    let mut level = prime(vcp, &voice.id, l).await;
    if level.is_err() {
        tokio::time::sleep(Duration::from_secs(1)).await;
        level = prime(vcp, &voice.id, l).await;
    }
    match level {
        Ok(db) => Ok((voice, db)),
        Err(t) => {
            let _ = vcp.delete_voice(&voice.id).await;
            Err(match t {
                Trouble::Down(e) => Trouble::Down(e),
                Trouble::Failed(e) => Trouble::Failed(format!("VCP could not speak with the new voice: {e}")),
            })
        }
    }
}

struct Take {
    wav: Vec<u8>,
    duration: f32,
    wer: Option<f32>,
    /// Speech loudness (dBFS).
    level: Option<f32>,
}

struct Item {
    index: u32,
    def: ttv_link::VoiceDef,
    lang: &'static langs::DesignLang,
    key: String,
    takes: Vec<Take>,
}

async fn worker(inner: Arc<Inner>) {
    // Consecutive failed batches: wait longer each time (VCP restarting,
    // speech recognition broken), so a broken setup doesn't keep the GPU busy.
    let mut failures = 0u32;
    loop {
        let next = {
            let jobs = inner.jobs.lock();
            jobs.iter().find(|j| matches!(j.view.state, JobState::Queued | JobState::Running | JobState::Waiting)).map(|j| j.view.id.clone())
        };
        let vcp = inner.vcp.read().clone();
        let (Some(id), Some(vcp)) = (next, vcp) else {
            tokio::select! {
                _ = inner.wake.notified() => {}
                // Idle: submit, resume and a new VCP connection wake the worker.
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            }
            continue;
        };
        let wakes = inner.wakes.load(std::sync::atomic::Ordering::SeqCst);
        match batch(&inner, &vcp, &id).await {
            Ok(()) => failures = 0,
            Err(e) => {
                log::warn!("job {id}: {e}");
                inner.update(&id, |j| j.view.error = Some(e));
                inner.save_jobs();
                failures += 1;
                let until = tokio::time::Instant::now() + Duration::from_secs((10u64 << (failures - 1).min(6)).min(600));
                loop {
                    tokio::select! {
                        _ = inner.wake.notified() => {
                            if inner.wakes.load(std::sync::atomic::Ordering::SeqCst) != wakes {
                                break;
                            }
                        }
                        _ = tokio::time::sleep_until(until) => break,
                    }
                }
            }
        }
    }
}

/// Runs one batch of a job. Ok(()) also when the job was paused/cancelled.
async fn batch(inner: &Arc<Inner>, vcp: &ttv_vcp::Client, id: &str) -> Result<(), String> {
    let Some(job) = inner.jobs.lock().iter().find(|j| j.view.id == id).cloned() else { return Ok(()) };
    let req = &job.request;
    let mut items: Vec<Item> = Vec::new();
    'outer: for d in &req.defs {
        for code in &req.languages {
            let Some(lang) = langs::get(code) else { continue };
            let key = d.def.key(code);
            if inner.library.contains(&key) || job.failed.contains(&key) {
                continue;
            }
            items.push(Item { index: d.index, def: d.def.clone(), lang, key, takes: Vec::new() });
            if items.len() >= BATCH {
                break 'outer;
            }
        }
    }
    if items.is_empty() {
        inner.update(id, |j| {
            j.view.state = JobState::Done;
            j.view.phase.clear();
            j.view.current = None;
            j.view.batch = 0;
            j.view.step = 0;
            j.view.steps = 0;
            j.view.eta_secs = Some(0);
            j.view.finished = Some(chrono::Utc::now().timestamp());
        });
        inner.save_jobs();
        log::info!("job {id} finished");
        return Ok(());
    }
    let t0 = Instant::now();
    let takes = req.options.takes.clamp(1, 3);
    let label = |it: &Item| format!("{} · {}", it.def.name, it.lang.code);
    let phase = |name: &str, steps: usize| {
        let name = name.to_string();
        inner.update(id, |j| {
            j.view.state = JobState::Running;
            j.view.phase = name;
            j.view.step = 0;
            j.view.steps = steps as u32;
        });
    };
    let batch_size = items.len() as u32;
    inner.update(id, |j| {
        j.view.state = JobState::Running;
        j.view.error = None;
        j.view.batch = batch_size;
    });

    // 1. design all takes (design checkpoint)
    phase("design", items.len() * takes as usize);
    let mut last_error = None;
    for it in items.iter_mut() {
        for _ in 0..takes {
            if !inner.gap(id).await {
                return Ok(());
            }
            let l = label(it);
            inner.update(id, |j| j.view.current = Some(l));
            match design(vcp, &instruct(&it.def), it.lang).await {
                Ok(wav) => {
                    let duration = ttv_vcp::wav::duration_secs(&wav).unwrap_or(0.0);
                    let level = audio::decode(&wav).and_then(|(s, _)| audio::loudness_db(&s));
                    it.takes.push(Take { wav, duration, wer: None, level });
                }
                Err(e) => {
                    log::warn!("design {}: {e}", label(it));
                    last_error = Some(e);
                }
            }
            inner.update(id, |j| j.view.step += 1);
        }
    }
    // Nothing designed at all: VCP is down or restarting. Try the batch again
    // later (the worker waits) instead of counting a failed attempt per voice.
    if items.iter().all(|it| it.takes.is_empty()) {
        return Err(format!("Voice Creator Pro designed nothing: {}", last_error.unwrap_or_else(|| "no answer".into())));
    }

    // 2. speech check (speech recognition model). A take only counts as
    // checked when VCP heard it; one VCP couldn't transcribe is not accepted.
    if req.options.speech_check {
        let todo: usize = items.iter().filter(|it| it.lang.asr).map(|it| it.takes.iter().filter(|t| plausible(t)).count()).sum();
        phase("check", todo);
        let (mut heard_any, mut last_failure) = (false, None);
        for it in items.iter_mut() {
            if !it.lang.asr {
                continue;
            }
            let (name, lang) = (label(it), it.lang);
            for take in it.takes.iter_mut().filter(|t| plausible(t)) {
                if !inner.gap(id).await {
                    return Ok(());
                }
                let l = name.clone();
                inner.update(id, |j| j.view.current = Some(l));
                // One dropped connection shouldn't cost the batch: try once more.
                let mut heard = transcribe(vcp, &take.wav, lang).await;
                if matches!(heard, Err(Trouble::Down(_))) {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    heard = transcribe(vcp, &take.wav, lang).await;
                }
                match heard {
                    Ok(heard) => {
                        heard_any = true;
                        take.wer = Some(langs::error_rate(lang.text, &heard, lang.code));
                    }
                    Err(Trouble::Down(e)) => return Err(format!("speech check: {e}")),
                    Err(Trouble::Failed(e)) => {
                        log::warn!("speech check {name}: {e}");
                        take.wer = Some(1.0);
                        last_failure = Some(e);
                    }
                }
                inner.update(id, |j| j.view.step += 1);
            }
        }
        // Speech recognition failed for every take: it's broken, not the voices.
        if todo > 0 && !heard_any {
            return Err(format!("VCP's speech recognition doesn't work: {}", last_failure.unwrap_or_default()));
        }
    }

    // 3. pick the best take, save it, prime it (speaking checkpoint)
    phase("save", items.len());
    let mut finished = 0u32;
    for it in items.iter_mut() {
        let max_wer = if matches!(it.lang.code, "ja" | "zh" | "ko") { MAX_WER_CJK } else { MAX_WER };
        let checked = req.options.speech_check && it.lang.asr;
        // Best take: understood, near 7 s, and at a normal loudness (VCP
        // clones the loudness too; a whispered sample stays too quiet even
        // with the maximum gain).
        let best = it
            .takes
            .iter()
            .enumerate()
            .filter(|(_, t)| plausible(t) && if checked { t.wer.is_some_and(|w| w <= max_wer) } else { true })
            .min_by(|(_, a), (_, b)| {
                let score = |t: &Take| t.wer.unwrap_or(0.0) + (t.duration - 7.0).abs() * 0.01 + t.level.map_or(0.5, |db| ((db + 20.0).abs() - 6.0).max(0.0) * 0.05);
                score(a).total_cmp(&score(b))
            })
            .map(|(i, _)| i);
        let Some(bi) = best else {
            inner.attempt_failed(id, &it.key, None);
            continue;
        };
        let take = it.takes.swap_remove(bi);
        if !inner.gap(id).await {
            return Ok(());
        }
        let l = label(it);
        inner.update(id, |j| {
            j.view.current = Some(l);
            j.view.phase = "prime".into();
        });
        let voice_name = format!("TTV {} {} {:03} {}", req.preset_id, it.lang.code, it.index + 1, it.def.name);
        let save = serde_json::json!({
            "name": voice_name,
            "audio_base64": base64::engine::general_purpose::STANDARD.encode(&take.wav),
            "transcript": it.def.description,
            "ref_text": it.lang.text,
            "voice_type": "design",
            "transcript_manual": true,
        });
        let (voice, spoken) = match save_voice(vcp, &save, it.lang).await {
            Ok(v) => v,
            Err(Trouble::Down(e)) => return Err(e),
            Err(Trouble::Failed(e)) => {
                log::warn!("{voice_name}: {e}");
                inner.attempt_failed(id, &it.key, Some(format!("{}: {e}", label(it))));
                continue;
            }
        };
        let gain = audio::gain_from_db(mix_levels(take.level, Some(spoken)).unwrap_or(-20.0));
        let mut backup = false;
        if req.options.backup
            && let Some((samples, rate)) = audio::decode(&take.wav)
            && let Some(flac) = audio::flac(&samples, rate)
        {
            backup = std::fs::write(inner.sample_path(&it.key), flac).is_ok();
        }
        inner.change(vec![LibraryEntry {
            key: it.key.clone(),
            def_hash: it.def.hash(),
            lang: it.lang.code.to_string(),
            preset_id: req.preset_id.clone(),
            index: it.index,
            name: it.def.name.clone(),
            voice_id: voice.id.clone(),
            voice_name,
            created: chrono::Utc::now().timestamp(),
            duration: take.duration,
            wer: take.wer,
            gain,
            backup,
            level: take.level,
        }], &[]);
        finished += 1;
        inner.update(id, |j| {
            j.view.done += 1;
            j.view.step += 1;
            j.view.phase = "save".into();
        });
    }

    // speed: moving average over real batches
    if finished > 0 {
        let per = t0.elapsed().as_secs_f32() / finished as f32;
        let spv = {
            let mut s = inner.stats.lock();
            let v = match s.secs_per_voice {
                Some(old) => old * 0.6 + per * 0.4,
                None => per,
            };
            s.secs_per_voice = Some(v);
            v
        };
        inner.save_stats();
        inner.update(id, |j| {
            j.view.secs_per_voice = Some(spv);
            let left = j.view.total.saturating_sub(j.view.done + j.view.failed);
            j.view.eta_secs = Some((left as f32 * spv).round() as u64);
        });
    }
    inner.save_jobs();
    Ok(())
}

/// Reference samples must be 3–10.5 s long (VCP clones best from that).
fn plausible(t: &Take) -> bool {
    (3.0..=10.5).contains(&t.duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_name_gender_and_age() {
        let def = |gender: &str, age| ttv_link::VoiceDef { name: "X".into(), gender: gender.into(), age, description: " Low, warm voice. ".into(), fx: None };
        assert_eq!(instruct(&def("m", 35)), "A 35-year-old man. Low, warm voice.");
        assert_eq!(instruct(&def("f", 16)), "A 16-year-old teenage girl. Low, warm voice.");
        assert_eq!(instruct(&def("m", 9)), "A 9-year-old boy. Low, warm voice.");
        assert_eq!(instruct(&def("n", 40)), "Low, warm voice.");
    }
}
