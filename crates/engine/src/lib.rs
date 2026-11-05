//! The voice computation engine: turns preset definitions into ready VCP
//! voices. Runs on TTV-TTS Proxy (so it keeps going while the gaming PC is
//! off) or inside the app when VCP runs on the same PC.
//!
//! Work happens in batches and phases – design all takes of a batch, then
//! check them, then save and prime the winners – because Qwen3 swaps
//! checkpoints between designing and speaking (~20 s each time): each model
//! does all of its work in one go. The benchmark measures what that costs
//! here and sets the batch size. Chat goes first: before every VCP call the
//! engine waits until nobody else used VCP for a few seconds. Jobs and the
//! library survive restarts.

pub(crate) mod audio;
mod bench;
pub mod langs;
mod models;
pub mod pack;
mod plan;
pub mod quality;
mod run;
mod store;
mod takes;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, broadcast};
use ttv_link::{BenchHistory, BenchProgress, BenchRequest, EngineStatus, ExportRequest, JobKind, JobRequest, JobState, JobView, LibraryChange, LibraryEntry, LibrarySnapshot, PackVoice, PlanRequest, PlanView, Verified};
use ttv_vcp::GenerationResult;

pub use bench::{BenchTools, Restart, Vram};
pub use store::Library;

/// Voices per batch (between two checkpoint swaps), until the benchmark has
/// measured what suits this PC.
pub(crate) const BATCH: usize = 16;
/// Ready-made packs (archive jobs) run in the background for days: a whole
/// preset in one go switches models a fifth as often.
pub(crate) const BATCH_ARCHIVE: usize = 240;
/// The engine only calls VCP after this much quiet (chat goes first).
const GAP: Duration = Duration::from_secs(6);
/// Designing needs VCP's design checkpoint and speech recognition pushes the
/// speaking model out of memory; switching back for chat takes ~16 s
/// (measured on an RTX 3070, 1.7B models). After chat, these steps wait for
/// this much quiet, so a chatty stream doesn't pay the switch with every
/// message; steps that speak, like chat, keep the short gap.
pub(crate) const SWITCH_GAP: Duration = Duration::from_secs(90);
/// Attempts per voice before it counts as failed.
pub(crate) const ATTEMPTS: u32 = 3;
/// Accepted speech-check error rate.
pub(crate) const MAX_WER: f32 = 0.15;
pub(crate) const MAX_WER_CJK: f32 = 0.25;

/// A design, check or prime takes seconds (a cold start ~25 s, a checkpoint
/// swap ~20 s, chat in between). Much longer means VCP hangs.
pub(crate) const STUCK: Duration = Duration::from_secs(180);

/// When somebody else (chat TTS, listening tests) last used VCP.
pub type Activity = Arc<dyn Fn() -> Option<Instant> + Send + Sync>;

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Job(JobView),
    Library(LibraryChange),
    /// VCP's API answers, but its work requests time out: it hangs. The
    /// proxy restarts it (at most every 30 minutes).
    Stuck(String),
    /// VCP works, but much slower than it should. The proxy restarts it if
    /// that's switched on.
    Slow(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StoredJob {
    view: JobView,
    request: JobRequest,
    /// key -> attempts (improve: rounds) so far
    attempts: HashMap<String, u32>,
    /// keys that failed for good
    failed: Vec<String>,
    /// Improve and review: keys that are done.
    processed: Vec<String>,
    /// Compute: keys this job created (for improving the odd ones after).
    created: Vec<String>,
    /// The follow-up improve job was queued.
    followed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Stats {
    /// Measured seconds per voice (moving average over real jobs).
    secs_per_voice: Option<f32>,
    /// Measured seconds per design (benchmark), for estimates.
    design_secs: Option<f32>,
    /// Measured seconds per voice of quality checks of computed voices.
    review_secs: Option<f32>,
    /// Voices per batch that suit this PC (benchmark).
    batch_voices: Option<u32>,
    /// Slower than this counts as a drop in speed: design, listen (benchmark).
    limits: Option<[f32; 2]>,
}

/// The engine's own work tells when VCP has slowed down (chat speed alone
/// doesn't: nobody chats at night while computing crawls along).
pub(crate) const DESIGNING: usize = 0;
pub(crate) const LISTENING: usize = 1;
/// Median of this many calls, against the limit.
const WATCH: usize = 12;
const WATCH_EVERY: Duration = Duration::from_secs(1800);

#[derive(Default)]
struct SpeedWatch {
    recent: [std::collections::VecDeque<f32>; 2],
    last: Option<usize>,
    best: [Option<f32>; 2],
    fired: Option<Instant>,
}

/// What the cached plan view was worked out from.
type PlanRevision = (u64, u64, u64);

pub(crate) struct Inner {
    dir: PathBuf,
    vcp: RwLock<Option<ttv_vcp::Client>>,
    library: Library,
    /// Finished voices of archive jobs that left VCP: lossless sample in
    /// samples/, data here; exportable as packs.
    archive: Library,
    /// Voices of archive jobs that go to the archive once no job needs them.
    pending: Mutex<std::collections::HashSet<String>>,
    jobs: Mutex<Vec<StoredJob>>,
    stats: Mutex<Stats>,
    events: broadcast::Sender<EngineEvent>,
    activity: Activity,
    wake: Notify,
    /// Counts wake-ups, so a stale Notify permit doesn't cut a backoff short.
    wakes: std::sync::atomic::AtomicU64,
    /// The benchmark has VCP to itself: nothing starts, jobs wait where they are.
    hold: std::sync::atomic::AtomicBool,
    bench: Mutex<Option<BenchProgress>>,
    bench_stop: std::sync::atomic::AtomicBool,
    plan: Mutex<Option<plan::Plan>>,
    plan_rev: std::sync::atomic::AtomicU64,
    plan_view: Mutex<Option<(PlanRevision, PlanView)>>,
    /// Batches that stopped halfway (VCP restarted, job paused): what's
    /// designed and checked isn't done again.
    parked: Mutex<HashMap<String, Vec<run::Item>>>,
    watch: Mutex<SpeedWatch>,
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
        let saved_plan = plan::load(&dir);
        let inner = Arc::new(Inner {
            library: Library::load(dir.join("library.jsonl")),
            archive: Library::load(dir.join("archive.jsonl")),
            pending: Mutex::new(read_json(&dir.join("archive-pending.json")).unwrap_or_default()),
            stats: Mutex::new(read_json(&dir.join("stats.json")).unwrap_or_default()),
            jobs: Mutex::new(jobs),
            dir,
            vcp: RwLock::new(None),
            events: broadcast::channel(256).0,
            activity,
            wake: Notify::new(),
            wakes: Default::default(),
            hold: std::sync::atomic::AtomicBool::new(false),
            bench: Mutex::new(None),
            bench_stop: std::sync::atomic::AtomicBool::new(false),
            plan: Mutex::new(saved_plan),
            plan_rev: std::sync::atomic::AtomicU64::new(0),
            plan_view: Mutex::new(None),
            parked: Mutex::new(HashMap::new()),
            watch: Mutex::new(SpeedWatch::default()),
        });
        let _ = inner.relate_all();
        Self { inner }
    }

    /// The worker loop; spawn it once on a tokio runtime.
    pub fn worker(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        worker(self.inner.clone())
    }

    pub fn set_vcp(&self, vcp: Option<ttv_vcp::Client>) {
        // chat goes first at VCP
        *self.inner.vcp.write() = vcp.map(|c| c.background());
        self.inner.wake_up();
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.inner.events.subscribe()
    }

    /// Some job is waiting to run or running (paused ones don't count), the
    /// benchmark runs, or the plan has more to do.
    pub fn working(&self) -> bool {
        self.inner.jobs.lock().iter().any(|j| matches!(j.view.state, JobState::Queued | JobState::Running | JobState::Waiting))
            || self.inner.hold.load(std::sync::atomic::Ordering::SeqCst)
            || self.inner.plan.lock().as_ref().is_some_and(plan::Plan::running)
    }

    pub fn status(&self) -> EngineStatus {
        EngineStatus {
            jobs: self.inner.jobs.lock().iter().map(|j| j.view.clone()).collect(),
            bench: self.inner.bench.lock().clone(),
            plan: plan::view(&self.inner),
            secs_per_voice: self.inner.stats.lock().secs_per_voice,
            library_size: self.inner.library.len() as u32,
            library_rev: self.inner.library.rev(),
            archived: self.inner.archive.len() as u32,
        }
    }

    /// Voices kept only in the archive (for ready-made packs).
    pub fn archived(&self) -> LibrarySnapshot {
        self.inner.archive.snapshot()
    }

    /// Moves computed voices to the archive now: their sample stays here
    /// (lossless) and they leave VCP. Returns how many were archived.
    pub async fn archive(&self, keys: &[String]) -> Result<u32, String> {
        let vcp = self.inner.vcp.read().clone().ok_or("Voice Creator Pro is not connected")?;
        self.inner.archive_keys(&vcp, keys).await
    }

    /// Whether a VCP voice is one of the computed (library) voices.
    pub fn has_voice(&self, voice_id: &str) -> bool {
        self.inner.library.has_voice(voice_id)
    }

    pub fn library(&self) -> LibrarySnapshot {
        self.inner.library.snapshot()
    }

    /// One computed voice (not archived) by key.
    pub fn entry(&self, key: &str) -> Option<LibraryEntry> {
        self.inner.library.get(key)
    }

    /// Queues a job: computing what isn't computed yet, improving or
    /// checking computed voices.
    pub fn submit(&self, req: JobRequest) -> Result<JobView, String> {
        self.inner.submit(req)
    }

    /// Marks voices as listened to and fine as they are (or not), so the
    /// quality check doesn't list them any more.
    pub fn keep(&self, keys: &[String], kept: bool) -> u32 {
        let changed: Vec<LibraryEntry> = keys
            .iter()
            .filter_map(|k| self.inner.library.get(k))
            .filter_map(|e| {
                let mut q = e.quality.clone()?;
                (q.kept != kept).then(|| {
                    q.kept = kept;
                    LibraryEntry { quality: Some(q), ..e }
                })
            })
            .collect();
        let n = changed.len() as u32;
        self.inner.change(changed, &[]);
        n
    }

    /// Measures VCP with nothing else running (see `bench`). Returns at once;
    /// `status().bench` shows the progress.
    pub fn bench_start(&self, req: BenchRequest, tools: BenchTools) -> Result<(), String> {
        bench::start(&self.inner, req, tools)
    }

    pub fn bench_stop(&self) {
        self.inner.bench_stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The last benchmarks, newest first.
    pub fn bench_history(&self) -> BenchHistory {
        bench::history(&self.inner)
    }

    /// Starts the automatic plan (see `plan`).
    pub fn plan_start(&self, req: PlanRequest) -> Result<PlanView, String> {
        plan::start(&self.inner, req)
    }

    /// pause, resume or stop.
    pub fn plan_control(&self, action: &str) -> Result<(), String> {
        plan::control(&self.inner, action)
    }

    /// VCP was restarted: the speed measured so far no longer applies.
    pub fn speed_reset(&self) {
        self.inner.speed_reset();
    }

    /// pause, resume, cancel or remove (finished jobs).
    pub fn control(&self, id: &str, action: &str) -> Result<(), String> {
        if matches!(action, "cancel" | "remove") {
            self.inner.parked.lock().remove(id);
        }
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

    /// Replaces library entries by others for the same VCP voices (when an
    /// adopted preset's definitions change). Nothing is deleted in VCP.
    pub fn rekey(&self, entries: Vec<LibraryEntry>, remove: &[String]) -> u32 {
        let n = entries.len() as u32;
        let keep: std::collections::HashSet<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        let remove: Vec<String> = remove.iter().filter(|k| !keep.contains(k.as_str())).cloned().collect();
        self.inner.change(entries, &remove);
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
        self.inner.estimate(voices, takes, check)
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

    /// Writes a voice pack of computed voices to the engine's folder and
    /// returns its path. Samples come from the lossless backups, else from
    /// VCP. Old exports (> 1 h) are cleaned up.
    pub async fn export(&self, req: ExportRequest, app: &str) -> Result<PathBuf, String> {
        let vcp = self.inner.vcp.read().clone();
        let dir = self.inner.dir.join("packs");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for f in rd.flatten() {
                let old = f.metadata().and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|e| e > Duration::from_secs(3600)));
                if old && f.file_name().to_string_lossy().starts_with("export-") {
                    let _ = std::fs::remove_file(f.path());
                }
            }
        }
        let path = dir.join(format!("export-{:08x}.{}", rand::random::<u32>(), pack::EXTENSION));
        let mut w = pack::PackWriter::create(&path, &req.title, app, req.presets.clone())?;
        for v in &req.voices {
            let (e, archived) = match self.inner.library.get(&v.key) {
                Some(e) => (e, false),
                None => match self.inner.archive.get(&v.key) {
                    Some(e) => (e, true),
                    None => continue,
                },
            };
            let Some(lang) = langs::get(&e.lang) else { continue };
            let (ref_text, flac) = match (archived, self.sample(&e.key)) {
                // the archive keeps what the sample says
                (true, Some(f)) => (e.ref_text.clone().unwrap_or_else(|| lang.text.to_string()), f),
                (true, None) => return Err(format!("{}: the archived sample is missing", e.voice_name)),
                // packs built away from VCP: designed voices say the language's sentence
                (false, Some(f)) if vcp.is_none() => (e.ref_text.clone().unwrap_or_else(|| lang.text.to_string()), f),
                (false, backup) => {
                    let vcp = vcp.as_ref().ok_or("Voice Creator Pro is not connected")?;
                    // VCP knows what the sample says (1.x voices say other sentences)
                    let sample = vcp.voice_sample(&e.voice_id).await.map_err(|err| format!("{}: {err}", e.voice_name))?;
                    let ref_text = sample.spoken_text().unwrap_or(lang.text).to_string();
                    let flac = match backup {
                        Some(f) => f,
                        None => {
                            let wav = base64::engine::general_purpose::STANDARD.decode(sample.audio_base64.as_bytes()).map_err(|e| e.to_string())?;
                            let (s, r) = audio::decode(&wav).ok_or_else(|| format!("{}: unreadable sample", e.voice_name))?;
                            audio::flac(&s, r).ok_or("FLAC encoding failed")?
                        }
                    };
                    (ref_text, flac)
                }
            };
            let voice = PackVoice {
                key: e.key.clone(),
                def_hash: e.def_hash.clone(),
                lang: e.lang.clone(),
                preset_id: v.preset_id.clone(),
                index: v.index,
                name: v.name.clone(),
                description: v.description.clone(),
                file: String::new(),
                ref_text,
                duration: e.duration,
                wer: e.wer,
                gain: e.gain,
                level: e.level,
                quality: e.quality.clone(),
            };
            w.add(voice, &flac)?;
        }
        if w.voices() == 0 {
            let _ = std::fs::remove_file(&path);
            return Err("none of these voices is computed".into());
        }
        let n = w.voices();
        let path = w.finish()?;
        log::info!("pack exported: {n} voices, {}", path.display());
        Ok(path)
    }

    /// Lossless backup of a voice's sample, if one was kept.
    pub fn sample(&self, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.inner.sample_path(key)).ok()
    }
}

impl Inner {
    fn submit(&self, mut req: JobRequest) -> Result<JobView, String> {
        // Checking or improving a voice twice at the same time is wasted GPU
        // time: leave out what an open job of the same kind covers already.
        if matches!(req.kind, JobKind::Review | JobKind::Improve) && !req.only.is_empty() {
            let busy: std::collections::HashSet<String> = self
                .jobs
                .lock()
                .iter()
                .filter(|j| j.request.kind == req.kind && matches!(j.view.state, JobState::Queued | JobState::Running | JobState::Waiting | JobState::Paused))
                .flat_map(|j| j.request.only.iter().filter(|k| !j.processed.contains(k)).cloned())
                .collect();
            req.only.retain(|k| !busy.contains(k));
            if req.only.is_empty() {
                return Err("These voices are being checked or improved already.".into());
            }
        }
        if req.kind == JobKind::Import {
            let m = pack::read_manifest(std::path::Path::new(&req.pack))?;
            let total = m.voices.iter().filter(|v| !self.library.contains(&v.key)).count() as u32;
            let mut langs: Vec<String> = m.voices.iter().map(|v| v.lang.clone()).collect();
            langs.sort();
            langs.dedup();
            let view = JobView {
                id: format!("job{}", rand::random::<u32>()),
                preset_id: m.presets.first().map(|p| p.id.clone()).unwrap_or_default(),
                preset_name: if m.title.is_empty() { m.presets.first().map(|p| p.name.clone()).unwrap_or_default() } else { m.title.clone() },
                languages: langs,
                options: req.options.clone(),
                kind: JobKind::Import,
                total,
                state: if total == 0 { JobState::Done } else { JobState::Queued },
                created: chrono::Utc::now().timestamp(),
                // saving + priming: ~3 s per voice
                eta_secs: Some(total as u64 * 3),
                ..Default::default()
            };
            self.jobs.lock().push(StoredJob { view: view.clone(), request: req, ..Default::default() });
            self.save_jobs();
            self.emit_job(&view);
            self.wake_up();
            log::info!("job {} queued: importing {} voices of {}", view.id, total, view.preset_name);
            return Ok(view);
        }
        if req.defs.is_empty() || req.languages.is_empty() {
            return Err("nothing to compute".into());
        }
        if let Some(l) = req.languages.iter().find(|l| langs::get(l).is_none()) {
            return Err(format!("language {l} can't be computed"));
        }
        let only: std::collections::HashSet<&str> = req.only.iter().map(String::as_str).collect();
        let total = req
            .defs
            .iter()
            .flat_map(|d| req.languages.iter().map(move |l| d.def.key(l)))
            .filter(|k| only.is_empty() || only.contains(k.as_str()))
            .filter(|k| match req.kind {
                JobKind::Compute => !self.library.contains(k) && !self.archive.contains(k),
                JobKind::Restore => self.archive.contains(k),
                _ => self.library.contains(k),
            })
            .count() as u32;
        let o = &req.options;
        let eta = match req.kind {
            JobKind::Compute => self.estimate(total, o.takes, o.speech_check).map(|s| if o.quality { s + s / 7 } else { s }),
            // 2–3 takes plus the review, on average 1.5 rounds
            JobKind::Improve => self.estimate(total, o.takes.max(2), true).map(|s| s * 2),
            JobKind::Review => Some((total as f32 * self.stats.lock().review_secs.unwrap_or(8.0)).round() as u64),
            JobKind::Import | JobKind::Restore => Some(total as u64 * 3),
        };
        let view = JobView {
            id: format!("job{}", rand::random::<u32>()),
            preset_id: req.preset_id.clone(),
            preset_name: req.preset_name.clone(),
            languages: req.languages.clone(),
            options: req.options.clone(),
            kind: req.kind,
            total,
            state: if total == 0 { JobState::Done } else { JobState::Queued },
            created: chrono::Utc::now().timestamp(),
            eta_secs: eta,
            secs_per_voice: self.stats.lock().secs_per_voice,
            ..Default::default()
        };
        self.jobs.lock().push(StoredJob { view: view.clone(), request: req, ..Default::default() });
        self.save_jobs();
        self.emit_job(&view);
        self.wake_up();
        log::info!("job {} queued ({:?}): {} voices of {}", view.id, view.kind, total, view.preset_name);
        Ok(view)
    }

    fn estimate(&self, voices: u32, takes: u8, check: bool) -> Option<u64> {
        let s = self.stats.lock().clone();
        let per = match (s.secs_per_voice, s.design_secs) {
            (Some(spv), _) => spv,
            // design + speech check per take, plus save + prime, plus the
            // checkpoint swaps spread over a batch.
            (None, Some(d)) => takes.max(1) as f32 * (d + if check { 1.5 } else { 0.0 }) + d * 0.9 + 60.0 / BATCH as f32,
            (None, None) => return None,
        };
        Some((voices as f32 * per).round() as u64)
    }

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

    /// Voices per batch: what the benchmark found, else the defaults.
    pub(crate) fn batch_size(&self, archive: bool) -> usize {
        match self.stats.lock().batch_voices {
            Some(n) if n > 0 => n as usize,
            _ if archive => BATCH_ARCHIVE,
            _ => BATCH,
        }
    }

    pub(crate) fn speed_reset(&self) {
        *self.watch.lock() = SpeedWatch::default();
    }

    /// One call of the engine's own work. The first call after another model
    /// doesn't count: it pays for the switch.
    pub(crate) fn speed_sample(&self, kind: usize, secs: f32) {
        let limit = self.stats.lock().limits.map(|l| l[kind]).filter(|l| *l > 0.0);
        let mut w = self.watch.lock();
        if w.last != Some(kind) {
            w.last = Some(kind);
            return;
        }
        let recent = &mut w.recent[kind];
        recent.push_back(secs);
        if recent.len() > WATCH {
            recent.pop_front();
        }
        if recent.len() < WATCH {
            return;
        }
        let mut sorted: Vec<f32> = recent.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        let median = sorted[WATCH / 2];
        let best = w.best[kind].map_or(median, |b| b.min(median));
        w.best[kind] = Some(best);
        // without a benchmark: against the best this run has seen
        let limit = limit.unwrap_or(best * 3.0 + 1.0);
        if median > limit && w.fired.is_none_or(|t| t.elapsed() > WATCH_EVERY) {
            w.fired = Some(Instant::now());
            w.recent = Default::default();
            drop(w);
            let what = if kind == DESIGNING { "designing" } else { "listening" };
            let _ = self.events.send(EngineEvent::Slow(format!("{what} takes {median:.1} s, normal is under {limit:.1} s")));
        }
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
        self.gap_for(id, GAP).await
    }

    /// Like `gap`, with `quiet` of no chat.
    async fn gap_for(&self, id: &str, quiet: Duration) -> bool {
        let mut waiting = false;
        loop {
            match self.state_of(id) {
                Some(JobState::Running | JobState::Waiting | JobState::Queued) => {}
                _ => return false,
            }
            let hold = self.hold.load(std::sync::atomic::Ordering::SeqCst);
            let busy = hold || (self.activity)().is_some_and(|t| t.elapsed() < quiet);
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
pub(crate) fn instruct(def: &ttv_link::VoiceDef) -> String {
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

pub(crate) async fn design(vcp: &ttv_vcp::Client, description: &str, l: &langs::DesignLang) -> Result<Vec<u8>, String> {
    let body = serde_json::json!({ "instruct_text": format!("{} {}", description.trim(), l.accent), "target_text": l.text, "language": l.vcp });
    let r: GenerationResult = vcp.post("/api/v1/generation/design?sync=true", &body, STUCK).await.map_err(|e| e.to_string())?;
    decode_result(r)
}

pub(crate) fn decode_result(r: GenerationResult) -> Result<Vec<u8>, String> {
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
pub(crate) enum Trouble {
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

pub(crate) async fn transcribe(vcp: &ttv_vcp::Client, wav: &[u8], l: &langs::DesignLang) -> Result<String, Trouble> {
    let body = serde_json::json!({ "audio_base64": base64::engine::general_purpose::STANDARD.encode(wav), "language": l.vcp });
    let t: Transcript = vcp.post("/api/v1/transcription/start?sync=true", &body, STUCK).await?;
    if !t.success {
        return Err(Trouble::Failed(t.error.unwrap_or_else(|| "speech recognition failed".into())));
    }
    Ok(t.text.unwrap_or_default())
}

/// What a voice said when it spoke the priming sentence.
pub(crate) struct Spoken {
    pub wav: Vec<u8>,
    pub analysis: quality::Analysis,
}

/// One normal generation with a saved voice (the priming sentence), or why
/// VCP couldn't speak with it.
pub(crate) async fn prime(vcp: &ttv_vcp::Client, voice_id: &str, l: &langs::DesignLang) -> Result<Spoken, Trouble> {
    speak(vcp, voice_id, l.verify, l).await
}

/// A saved voice speaks `text`.
pub(crate) async fn speak(vcp: &ttv_vcp::Client, voice_id: &str, text: &str, l: &langs::DesignLang) -> Result<Spoken, Trouble> {
    let body = serde_json::json!({ "voice_id": voice_id, "target_text": text, "language": l.vcp });
    let r: GenerationResult = vcp.post("/api/v1/generation/clone?sync=true", &body, STUCK).await?;
    let wav = decode_result(r)?;
    let (samples, rate) = audio::decode(&wav).ok_or("unreadable audio")?;
    if (samples.len() as f32) < rate as f32 * 0.8 {
        return Err("audio too short".into());
    }
    if audio::loudness_db(&samples).is_none() {
        return Err("silent audio".into());
    }
    Ok(Spoken { analysis: quality::analyze(&samples, rate), wav })
}

/// Speech loudness of a saved voice's reference sample (dBFS).
async fn reference_level(vcp: &ttv_vcp::Client, voice_id: &str) -> Result<f32, String> {
    let sample = vcp.voice_sample(voice_id).await.map_err(|e| e.to_string())?;
    let wav = base64::engine::general_purpose::STANDARD.decode(sample.audio_base64.as_bytes()).map_err(|e| e.to_string())?;
    let (samples, _) = audio::decode(&wav).ok_or("unreadable audio")?;
    audio::loudness_db(&samples).ok_or_else(|| "silent audio".into())
}

/// Expected speaking loudness of a voice from its reference sample and a
/// generation with it. Measured on 120 real voices: each alone predicts the
/// loudness of new sentences within ±1.2–1.4 dB (sd); the mean of both is
/// best (sd 1.16 dB, worst ±3.5 dB instead of ±6 dB for one short sentence).
pub(crate) fn mix_levels(reference: Option<f32>, spoken: Option<f32>) -> Option<f32> {
    match (reference, spoken) {
        (Some(r), Some(s)) => Some((r + s) / 2.0),
        (r, s) => r.or(s),
    }
}

/// Saves a take as a VCP voice and primes it: one normal generation makes
/// the voice ready for streaming (VCP streams a new voice only after that)
/// and shows how it speaks a new sentence. If VCP can't speak with
/// the voice, the voice is deleted again – VCP never keeps voices the
/// library doesn't know.
///
/// (VCP's `has_voice_clone_prompt` stays false for a voice saved while the
/// speaking model isn't loaded – the first of each batch. Measured: such
/// voices stream and speak exactly as fast, so it isn't worth extra work.)
pub(crate) async fn save_voice(vcp: &ttv_vcp::Client, body: &serde_json::Value, l: &langs::DesignLang) -> Result<(ttv_vcp::Voice, Spoken), Trouble> {
    let saved: ttv_vcp::VoiceSaveResponse = vcp.post("/api/v1/voices", body, Duration::from_secs(60)).await?;
    let Some(voice) = saved.voice.filter(|_| saved.success) else {
        return Err(Trouble::Failed(saved.message.unwrap_or_else(|| "VCP did not save the voice".into())));
    };
    let mut spoken = prime(vcp, &voice.id, l).await;
    if spoken.is_err() {
        tokio::time::sleep(Duration::from_secs(1)).await;
        spoken = prime(vcp, &voice.id, l).await;
    }
    match spoken {
        Ok(s) => Ok((voice, s)),
        Err(t) => {
            let _ = vcp.delete_voice(&voice.id).await;
            Err(match t {
                Trouble::Down(e) => Trouble::Down(e),
                Trouble::Failed(e) => Trouble::Failed(format!("VCP could not speak with the new voice: {e}")),
            })
        }
    }
}

/// Working order: a pack being imported first (voices someone asked for, a
/// few seconds each), then checks and improvements (short, and they finish
/// voices that exist), then what was computed by hand. Archive jobs go last:
/// they fill ready-made packs for weeks, and nothing should wait for them.
/// The app's job list shows the same order (`rank` in JobList.vue).
fn rank(j: &StoredJob) -> u8 {
    match j.request.kind {
        JobKind::Import => 0,
        JobKind::Review | JobKind::Improve | JobKind::Restore => 1,
        JobKind::Compute if !j.request.options.archive => 2,
        JobKind::Compute => 3,
    }
}

/// The job to work on now (the first of its rank). One that was at work and
/// has to step aside goes back to waiting: it continues later where it is.
fn next_job(inner: &Inner) -> Option<String> {
    if inner.hold.load(std::sync::atomic::Ordering::SeqCst) {
        return None;
    }
    let mut jobs = inner.jobs.lock();
    let next = jobs.iter().filter(|j| matches!(j.view.state, JobState::Queued | JobState::Running | JobState::Waiting)).min_by_key(|j| rank(j)).map(|j| j.view.id.clone())?;
    let mut aside = Vec::new();
    for j in jobs.iter_mut().filter(|j| j.view.id != next && matches!(j.view.state, JobState::Running | JobState::Waiting)) {
        j.view.state = JobState::Queued;
        j.view.phase.clear();
        j.view.current = None;
        j.view.batch = 0;
        j.view.step = 0;
        j.view.steps = 0;
        aside.push(j.view.clone());
    }
    drop(jobs);
    for v in &aside {
        inner.emit_job(v);
    }
    Some(next)
}

async fn worker(inner: Arc<Inner>) {
    // Consecutive failed batches: wait longer each time (VCP restarting,
    // speech recognition broken), so a broken setup doesn't keep the GPU busy.
    let mut failures = 0u32;
    loop {
        let vcp = inner.vcp.read().clone();
        // nothing open: the plan may have more to do (with VCP there to do it)
        let next = next_job(&inner).or_else(|| (vcp.is_some() && !inner.hold.load(std::sync::atomic::Ordering::SeqCst) && plan::advance(&inner)).then(|| next_job(&inner)).flatten());
        let (Some(id), Some(vcp)) = (next, vcp) else {
            tokio::select! {
                _ = inner.wake.notified() => {}
                // Idle: submit, resume and a new VCP connection wake the worker.
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            }
            continue;
        };
        let wakes = inner.wakes.load(std::sync::atomic::Ordering::SeqCst);
        match run::batch(&inner, &vcp, &id).await {
            Ok(()) => {
                failures = 0;
                inner.archive_finished(&vcp).await;
            }
            Err(e) => {
                log::warn!("job {id}: {e}");
                if e.contains("timed out") {
                    let _ = inner.events.send(EngineEvent::Stuck(e.clone()));
                }
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

/// Mono samples and rate of a WAV file (for tools and tests).
pub fn decode_wav(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
    audio::decode(bytes)
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

    fn slow(rx: &mut tokio::sync::broadcast::Receiver<EngineEvent>) -> Option<String> {
        std::iter::from_fn(|| rx.try_recv().ok()).find_map(|ev| if let EngineEvent::Slow(why) = ev { Some(why) } else { None })
    }

    #[test]
    fn a_drop_in_speed_is_noticed_once() {
        let dir = std::env::temp_dir().join(format!("ttv-engine-watch-{}", rand::random::<u64>()));
        let e = Engine::new(dir, Arc::new(|| None));
        e.inner.stats.lock().limits = Some([8.0, 2.0]);
        let mut rx = e.subscribe();
        // normal speed, with the odd long call
        for i in 0..40 {
            e.inner.speed_sample(DESIGNING, if i % 7 == 0 { 20.0 } else { 3.3 });
        }
        assert_eq!(slow(&mut rx), None);
        // much slower: the median of a dozen calls crosses the limit, once
        for _ in 0..40 {
            e.inner.speed_sample(DESIGNING, 27.0);
        }
        assert!(slow(&mut rx).is_some_and(|w| w.contains("designing")));
        assert_eq!(slow(&mut rx), None, "at most every 30 minutes");
        // after a restart it's watched anew
        e.speed_reset();
        for _ in 0..14 {
            e.inner.speed_sample(DESIGNING, 27.0);
        }
        assert!(slow(&mut rx).is_some());
    }

    #[test]
    fn the_first_call_after_another_model_does_not_count() {
        let dir = std::env::temp_dir().join(format!("ttv-engine-switch-{}", rand::random::<u64>()));
        let e = Engine::new(dir, Arc::new(|| None));
        e.inner.stats.lock().limits = Some([8.0, 2.0]);
        let mut rx = e.subscribe();
        // batches of one call per model: every call pays for a switch
        for _ in 0..30 {
            e.inner.speed_sample(0, 30.0);
            e.inner.speed_sample(1, 30.0);
        }
        assert_eq!(slow(&mut rx), None);
    }

    #[test]
    fn imports_and_improvements_go_before_computing_and_archive_jobs_last() {
        let job = |id: &str, kind, archive, state| StoredJob {
            view: JobView { id: id.into(), state, phase: "design".into(), step: 5, steps: 9, batch: 48, ..Default::default() },
            request: JobRequest { kind, options: ttv_link::JobOptions { archive, ..Default::default() }, ..Default::default() },
            ..Default::default()
        };
        let dir = std::env::temp_dir().join(format!("ttv-engine-order-{}", rand::random::<u64>()));
        let e = Engine::new(dir, Arc::new(|| None));
        e.inner.jobs.lock().extend([
            job("packs", JobKind::Compute, true, JobState::Running),
            job("packs2", JobKind::Compute, true, JobState::Queued),
            job("mine", JobKind::Compute, false, JobState::Queued),
            job("improve", JobKind::Improve, true, JobState::Queued),
            job("review", JobKind::Review, false, JobState::Queued),
            job("held", JobKind::Import, false, JobState::Paused),
            job("gone", JobKind::Import, false, JobState::Done),
            job("import", JobKind::Import, false, JobState::Queued),
        ]);
        let state = |id: &str| e.inner.state_of(id).unwrap();

        assert_eq!(next_job(&e.inner).as_deref(), Some("import"));
        // the job that was at work waits its turn, it doesn't stay "computing"
        assert_eq!(state("packs"), JobState::Queued);
        let v = e.status().jobs.into_iter().find(|j| j.id == "packs").unwrap();
        assert!(v.phase.is_empty() && v.batch == 0 && v.step == 0 && v.steps == 0, "{v:?}");
        // jobs that aren't open stay as they are
        assert_eq!(state("held"), JobState::Paused);
        assert_eq!(state("gone"), JobState::Done);

        for id in ["import", "improve", "review", "mine", "packs", "packs2"] {
            assert_eq!(next_job(&e.inner).as_deref(), Some(id));
            e.inner.update(id, |j| j.view.state = JobState::Done);
        }
        assert_eq!(next_job(&e.inner), None);
    }
}


/// Real data: `TTV_REAL_ENGINE=<folder with library.jsonl and archive.jsonl>
/// cargo test -p ttv-engine relate_all_settles -- --ignored --nocapture`.
#[cfg(test)]
mod real_data {
    /// The archive holds the only copy of its voices (they left VCP). Copy
    /// archive.jsonl and samples/ of a proxy into a folder, then
    /// `TTV_REAL_ENGINE=<folder> cargo test -p ttv-engine archived_samples -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn archived_samples_are_whole() {
        let Ok(src) = std::env::var("TTV_REAL_ENGINE") else { return };
        let src = std::path::PathBuf::from(src);
        let entries = crate::Library::load(src.join("archive.jsonl")).snapshot().entries;
        let next = std::sync::atomic::AtomicUsize::new(0);
        let bad = parking_lot::Mutex::new(Vec::new());
        let seconds = parking_lot::Mutex::new(0f64);
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    while let Some(e) = entries.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                        let path = src.join("samples").join(format!("{}.flac", e.key.replace(':', "_")));
                        let problem = match std::fs::read(&path) {
                            Err(err) => Some(format!("no sample: {err}")),
                            Ok(flac) => match crate::pack::decode_flac(&flac) {
                                Err(err) => Some(err),
                                Ok((samples, rate)) => {
                                    let secs = samples.len() as f32 / rate as f32;
                                    let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt();
                                    *seconds.lock() += secs as f64;
                                    if (secs - e.duration).abs() > 0.25 {
                                        Some(format!("{secs:.2} s of audio, the entry says {:.2} s", e.duration))
                                    } else if rms < 0.003 {
                                        Some(format!("silent (rms {rms})"))
                                    } else if e.ref_text.as_deref().is_none_or(|t| t.trim().is_empty()) {
                                        Some("no spoken text".into())
                                    } else {
                                        None
                                    }
                                }
                            },
                        };
                        if let Some(p) = problem {
                            bad.lock().push(format!("{} ({}): {p}", e.key, e.name));
                        }
                    }
                });
            }
        });
        let bad = bad.into_inner();
        println!("{} archived voices, {:.1} hours of audio, {} with a problem", entries.len(), seconds.into_inner() / 3600.0, bad.len());
        for b in bad.iter().take(30) {
            println!("  {b}");
        }
        assert!(bad.is_empty());
    }

    #[test]
    #[ignore]
    fn relate_all_settles_on_real_data() {
        let Ok(src) = std::env::var("TTV_REAL_ENGINE") else { return };
        let dir = std::env::temp_dir().join(format!("ttv-relate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["library.jsonl", "archive.jsonl"] {
            std::fs::copy(std::path::Path::new(&src).join(f), dir.join(f)).unwrap();
        }
        let lines = || ["library.jsonl", "archive.jsonl"].map(|f| std::fs::read_to_string(dir.join(f)).unwrap_or_default().lines().count());
        let before = lines();
        let activity: crate::Activity = std::sync::Arc::new(|| None);
        let first = crate::Engine::new(dir.clone(), activity.clone());
        let after = lines();
        println!("first run appended {} library + {} archive lines", after[0] - before[0], after[1] - before[1]);
        let again = first.inner.relate_all();
        drop(first);
        let mut reloads = Vec::new();
        for _ in 0..10 {
            let e = crate::Engine::new(dir.clone(), activity.clone());
            let after = lines();
            reloads.push(after[0] + after[1]);
            drop(e);
        }
        println!("lines after each reload: {reloads:?}");
        assert!(reloads.windows(2).all(|w| w[0] == w[1]), "a reload changed voices again: {reloads:?}");
        let reloaded = crate::Engine::new(dir.clone(), activity).inner.relate_all();
        let _ = std::fs::remove_dir_all(&dir);
        println!("second run changed {again}, after reloading {reloaded}");
        assert_eq!((again, reloaded), (0, 0), "comparing the languages again must change nothing");
    }
}
