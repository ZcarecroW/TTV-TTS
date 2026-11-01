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
use ttv_link::{EngineStatus, JobRequest, JobState, JobView, LibraryChange, LibraryEntry, LibrarySnapshot};
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
        });
        Self { inner }
    }

    /// The worker loop; spawn it once on a tokio runtime.
    pub fn worker(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        worker(self.inner.clone())
    }

    pub fn set_vcp(&self, vcp: Option<ttv_vcp::Client>) {
        *self.inner.vcp.write() = vcp;
        self.inner.wake.notify_one();
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
        self.inner.wake.notify_one();
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
        self.inner.wake.notify_one();
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

    /// Removes library entries whose VCP voice no longer exists.
    pub async fn verify(&self) -> Result<u32, String> {
        let vcp = self.inner.vcp.read().clone().ok_or("Voice Creator Pro is not connected")?;
        let existing: std::collections::HashSet<String> = vcp.voices().await.map_err(|e| e.to_string())?.into_iter().map(|v| v.id).collect();
        let gone: Vec<String> = self.inner.library.entries().into_iter().filter(|e| !existing.contains(&e.voice_id)).map(|e| e.key).collect();
        if !gone.is_empty() {
            log::info!("library: {} voices no longer exist in VCP", gone.len());
            self.inner.change(vec![], &gone);
        }
        Ok(gone.len() as u32)
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

async fn transcribe(vcp: &ttv_vcp::Client, wav: &[u8], l: &langs::DesignLang) -> Result<String, String> {
    let body = serde_json::json!({ "audio_base64": base64::engine::general_purpose::STANDARD.encode(wav), "language": l.vcp });
    let t: Transcript = vcp.post("/api/v1/transcription/start?sync=true", &body, Duration::from_secs(300)).await.map_err(|e| e.to_string())?;
    if !t.success {
        return Err(t.error.unwrap_or_else(|| "speech recognition failed".into()));
    }
    Ok(t.text.unwrap_or_default())
}

struct Take {
    wav: Vec<u8>,
    duration: f32,
    wer: Option<f32>,
}

struct Item {
    index: u32,
    def: ttv_link::VoiceDef,
    lang: &'static langs::DesignLang,
    key: String,
    takes: Vec<Take>,
}

async fn worker(inner: Arc<Inner>) {
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
        if let Err(e) = batch(&inner, &vcp, &id).await {
            log::warn!("job {id}: {e}");
            inner.update(&id, |j| j.view.error = Some(e));
            // VCP may be restarting: try again a bit later.
            tokio::time::sleep(Duration::from_secs(10)).await;
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
            j.view.eta_secs = Some(0);
            j.view.finished = Some(chrono::Utc::now().timestamp());
        });
        inner.save_jobs();
        log::info!("job {id} finished");
        return Ok(());
    }
    inner.update(id, |j| {
        j.view.state = JobState::Running;
        j.view.error = None;
    });
    let t0 = Instant::now();
    let takes = req.options.takes.clamp(1, 3);
    let label = |it: &Item| format!("{} · {}", it.def.name, it.lang.code);

    // 1. design all takes (design checkpoint)
    inner.update(id, |j| j.view.phase = "design".into());
    for it in items.iter_mut() {
        for _ in 0..takes {
            if !inner.gap(id).await {
                return Ok(());
            }
            let l = label(it);
            inner.update(id, |j| j.view.current = Some(l));
            match design(vcp, &it.def.description, it.lang).await {
                Ok(wav) => {
                    let duration = ttv_vcp::wav::duration_secs(&wav).unwrap_or(0.0);
                    it.takes.push(Take { wav, duration, wer: None });
                }
                Err(e) => log::warn!("design {}: {e}", label(it)),
            }
        }
    }

    // 2. speech check (speech recognition model)
    if req.options.speech_check {
        inner.update(id, |j| j.view.phase = "check".into());
        for it in items.iter_mut() {
            if !it.lang.asr {
                continue;
            }
            let (name, lang) = (label(it), it.lang);
            for take in it.takes.iter_mut().filter(|t| (3.0..=10.5).contains(&t.duration)) {
                if !inner.gap(id).await {
                    return Ok(());
                }
                let l = name.clone();
                inner.update(id, |j| j.view.current = Some(l));
                match transcribe(vcp, &take.wav, lang).await {
                    Ok(heard) => take.wer = Some(langs::error_rate(lang.text, &heard, lang.code)),
                    Err(e) => log::warn!("speech check {name}: {e}"),
                }
            }
        }
    }

    // 3. pick the best take, save it, prime it (speaking checkpoint)
    inner.update(id, |j| j.view.phase = "save".into());
    let mut finished = 0u32;
    for it in items.iter_mut() {
        let max_wer = if matches!(it.lang.code, "ja" | "zh" | "ko") { MAX_WER_CJK } else { MAX_WER };
        let best = it
            .takes
            .iter()
            .enumerate()
            .filter(|(_, t)| (3.0..=10.5).contains(&t.duration) && t.wer.is_none_or(|w| w <= max_wer))
            .min_by(|(_, a), (_, b)| {
                let score = |t: &Take| t.wer.unwrap_or(0.0) + (t.duration - 7.0).abs() * 0.01;
                score(a).total_cmp(&score(b))
            })
            .map(|(i, _)| i);
        let Some(bi) = best else {
            let key = it.key.clone();
            inner.update(id, |j| {
                let a = j.attempts.entry(key.clone()).or_insert(0);
                *a += 1;
                if *a >= ATTEMPTS {
                    j.failed.push(key);
                    j.view.failed += 1;
                }
            });
            continue;
        };
        let take = it.takes.swap_remove(bi);
        if !inner.gap(id).await {
            return Ok(());
        }
        let l = label(it);
        inner.update(id, |j| j.view.current = Some(l));
        let voice_name = format!("TTV {} {} {:03} {}", req.preset_id, it.lang.code, it.index + 1, it.def.name);
        let save = serde_json::json!({
            "name": voice_name,
            "audio_base64": base64::engine::general_purpose::STANDARD.encode(&take.wav),
            "transcript": it.def.description,
            "ref_text": it.lang.text,
            "voice_type": "design",
            "transcript_manual": true,
        });
        let saved: ttv_vcp::VoiceSaveResponse = vcp.post("/api/v1/voices", &save, Duration::from_secs(60)).await.map_err(|e| e.to_string())?;
        let Some(voice) = saved.voice.filter(|_| saved.success) else {
            return Err(saved.message.unwrap_or_else(|| "VCP did not save the voice".into()));
        };
        // Prime: one normal generation builds VCP's prompt cache (new voices
        // don't stream before that) and tells us how loud the voice is.
        inner.update(id, |j| j.view.phase = "prime".into());
        if !inner.gap(id).await {
            return Ok(());
        }
        let prime = serde_json::json!({ "voice_id": voice.id, "target_text": it.lang.verify, "language": it.lang.vcp });
        let primed: GenerationResult = vcp.post("/api/v1/generation/clone?sync=true", &prime, Duration::from_secs(600)).await.map_err(|e| e.to_string())?;
        let gain = decode_result(primed).ok().and_then(|w| audio::decode(&w)).map(|(s, _)| audio::gain_for(&s)).unwrap_or(1.0);
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
        }], &[]);
        finished += 1;
        inner.update(id, |j| {
            j.view.done += 1;
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
