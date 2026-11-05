//! The benchmark: what this PC really manages with Voice Creator Pro, and
//! the numbers the engine works with from then on.
//!
//! It has VCP to itself (no chat, no computing, no friends). It measures how
//! VCP runs right now, restarts it, and measures it fresh: the first start of
//! every model, each model at work (with short and long audio), what every
//! switch between two models costs, and whether VCP gets slower as it works.
//! The thorough level also runs all of that with VCP's low-VRAM mode switched
//! and with a lighter speech recognition model, and keeps the best setting.
//! The big speech recognition model (4 GB) and a voice model (4.6 GB) don't fit
//! on an 8 GB card together: when VCP doesn't unload one in time, both spill
//! into system memory and everything gets ten times slower. How well each
//! model hears is measured on real voices in all languages.
//!
//! From that come the batch size (long enough that switching models hardly
//! counts) and the limits beyond which VCP counts as having slowed down.
//!
//! Requests go to VCP directly: the benchmark holds the turn all the time.
//! The low-VRAM setting only changes right before a restart: changed while
//! models are loaded, it left VCP's memory full and everything ten times
//! slower until the next restart (measured 2026-11-05).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use base64::Engine as _;
use ttv_link::{BenchAdvice, BenchDrift, BenchHistory, BenchLevel, BenchProgress, BenchReport, BenchRequest, BenchSeries, BenchSwitch, BenchVariant};
use ttv_vcp::{GenerationResult, Method};

use crate::{Inner, decode_result, langs, models};

pub type Restart = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> + Send + Sync>;
/// Video memory: used and total MB.
pub type Vram = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Option<(u32, u32)>> + Send>> + Send + Sync>;

/// What the benchmark may use besides VCP's API.
#[derive(Clone, Default)]
pub struct BenchTools {
    /// Restarts VCP and returns once its API answers (TTV-TTS Proxy can).
    pub restart: Option<Restart>,
    pub vram: Option<Vram>,
}

const KEEP: usize = 12;
/// A VCP call takes seconds; minutes mean it hangs.
const TIMEOUT: Duration = Duration::from_secs(300);
/// Switching models may cost this share of the time.
const SWITCH_SHARE: f32 = 0.02;
const MIN_BATCH: u32 = 48;
/// A preset's size: more would mix presets without saving anything.
const MAX_BATCH: u32 = 240;
/// Takes per voice of real jobs.
const TAKES: f32 = 2.0;
/// Real voices per language for measuring how well a model hears.
const HEARING_CLIPS: usize = 4;
/// A lighter speech recognition model may mishear this much more.
const WER_MARGIN: f32 = 0.02;
/// Free video memory below this means the models are crowding each other.
const VRAM_MARGIN: u32 = 200;

const DESIGNS: [&str; 4] = [
    "A 45-year-old man. Calm, warm baritone with a clear, steady voice.",
    "A 28-year-old woman. Bright, quick and friendly voice.",
    "A 70-year-old man. Gravelly, slow and thoughtful voice.",
    "A 35-year-old woman. Smooth, low alto with a measured pace.",
];
/// Short, medium and long, like chat messages.
const SENTENCES: [&str; 3] = [
    "Thanks for the follow, and welcome to the stream!",
    "That was a really close round, but I think the next one will go much better for us.",
    "If you want to join the next game, just type your name in the chat, wait for the countdown, and then pick a team before the round begins.",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Design,
    Speak,
    Listen,
}

impl Kind {
    const ALL: [Kind; 3] = [Kind::Design, Kind::Listen, Kind::Speak];

    fn name(self) -> &'static str {
        match self {
            Kind::Design => "design",
            Kind::Speak => "speak",
            Kind::Listen => "listen",
        }
    }
}

/// Repetitions per level.
#[derive(Clone, Copy)]
struct Sizes {
    before: u32,
    design: u32,
    speak: u32,
    listen: u32,
    /// Measurements of each of the six switches between two models.
    switch: u32,
    /// Rounds through all models, for the drift.
    cycles: u32,
    per_cycle: u32,
    variant: bool,
}

const fn sizes(level: BenchLevel) -> Sizes {
    match level {
        BenchLevel::Quick => Sizes { before: 3, design: 6, speak: 9, listen: 9, switch: 1, cycles: 3, per_cycle: 3, variant: false },
        BenchLevel::Standard => Sizes { before: 4, design: 16, speak: 30, listen: 30, switch: 2, cycles: 6, per_cycle: 4, variant: false },
        BenchLevel::Thorough => Sizes { before: 4, design: 40, speak: 60, listen: 60, switch: 3, cycles: 12, per_cycle: 5, variant: true },
    }
}

fn steps(s: Sizes, restart: bool) -> u32 {
    let before = if restart { 3 * (s.before + 1) + 1 } else { 0 };
    let round = 3 + s.design + s.speak + s.listen + 6 * s.switch * 2 + s.cycles * s.per_cycle * 3;
    let tried = 3 + s.design / 2 + s.speak / 2 + s.listen / 2 + 12;
    let hearing = (HEARING_CLIPS * langs::LANGS.len()) as u32;
    // low VRAM, the lighter speech recognition and the way back
    let variant = if s.variant && restart { 2 * tried + 3 * hearing + 6 } else { 0 };
    before + round + variant + 1
}

fn median(v: &[f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s[s.len() / 2]
}

fn percentile(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s[((s.len() as f32 * p) as usize).min(s.len() - 1)]
}

/// Least squares: seconds = a + b × audio. Returns a, the cost of a request
/// before its first second of audio.
fn overhead(secs: &[f32], audio: &[f32]) -> Option<f32> {
    let n = secs.len().min(audio.len());
    if n < 3 {
        return None;
    }
    let (mx, my) = (audio[..n].iter().sum::<f32>() / n as f32, secs[..n].iter().sum::<f32>() / n as f32);
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for i in 0..n {
        sxy += (audio[i] - mx) * (secs[i] - my);
        sxx += (audio[i] - mx) * (audio[i] - mx);
    }
    if sxx < 0.01 {
        return None;
    }
    Some((my - sxy / sxx * mx).max(0.0))
}

#[derive(Default)]
struct Samples {
    secs: Vec<f32>,
    audio: Vec<f32>,
    failed: u32,
    first: Option<f32>,
}

impl Samples {
    fn add(&mut self, secs: f32, audio: Option<f32>) {
        self.secs.push(secs);
        if let Some(a) = audio.filter(|a| *a > 0.1) {
            self.audio.push(a);
        }
    }

    fn series(&self, kind: Kind, vram_mb: Option<u32>) -> BenchSeries {
        let rtf: Vec<f32> = self.secs.iter().zip(&self.audio).map(|(s, a)| s / a).collect();
        BenchSeries {
            kind: kind.name().into(),
            runs: self.secs.len() as u32,
            failed: self.failed,
            first_secs: self.first,
            median_secs: median(&self.secs),
            p90_secs: percentile(&self.secs, 0.9),
            worst_secs: self.secs.iter().copied().fold(0.0, f32::max),
            rtf: median(&rtf),
            overhead_secs: if kind == Kind::Speak { overhead(&self.secs, &self.audio) } else { None },
            vram_mb,
        }
    }
}

/// One round of measurements with VCP as it is.
#[derive(Default)]
struct Round {
    series: Vec<BenchSeries>,
    switches: Vec<BenchSwitch>,
}

fn secs_of(series: &[BenchSeries], kind: Kind) -> f32 {
    series.iter().find(|s| s.kind == kind.name()).map_or(0.0, |s| s.median_secs)
}

fn switch_cost(switches: &[BenchSwitch], from: Kind, to: Kind) -> f32 {
    switches.iter().find(|s| s.from == from.name() && s.to == to.name()).map_or(0.0, |s| s.secs)
}

/// Seconds per voice of a real job (design all takes, check them, speak with
/// the best one, listen to that), and the model switches of one batch.
fn work(series: &[BenchSeries], switches: &[BenchSwitch]) -> (f32, f32) {
    let (d, s, l) = (secs_of(series, Kind::Design), secs_of(series, Kind::Speak), secs_of(series, Kind::Listen));
    let voice = TAKES * (d + l) + s + l;
    let cycle = switch_cost(switches, Kind::Design, Kind::Listen) + switch_cost(switches, Kind::Listen, Kind::Speak) + switch_cost(switches, Kind::Speak, Kind::Listen) + switch_cost(switches, Kind::Listen, Kind::Design);
    (voice, cycle)
}

fn advise(series: &[BenchSeries], switches: &[BenchSwitch]) -> BenchAdvice {
    let (voice, cycle) = work(series, switches);
    let wanted = if voice > 0.0 { (cycle / (SWITCH_SHARE * voice)).ceil() as u32 } else { MAX_BATCH };
    let batch_voices = wanted.next_multiple_of(12).clamp(MIN_BATCH, MAX_BATCH);
    let voice_secs = voice + cycle / batch_voices as f32;
    let limit = |k: Kind| {
        let s = series.iter().find(|s| s.kind == k.name());
        s.map_or(0.0, |s| (s.median_secs * 2.5).max(s.p90_secs * 1.8))
    };
    BenchAdvice {
        batch_voices,
        voice_secs,
        switch_secs: cycle,
        switch_percent: if voice_secs > 0.0 { cycle / batch_voices as f32 / voice_secs * 100.0 } else { 0.0 },
        low_vram: None,
        asr_model: None,
        design_limit: limit(Kind::Design),
        speak_limit: limit(Kind::Speak),
        listen_limit: limit(Kind::Listen),
    }
}

/// Does a kind of work get slower: the first against the last third.
fn drift(kind: Kind, secs: &[f32]) -> Option<BenchDrift> {
    if secs.len() < 6 {
        return None;
    }
    let third = secs.len() / 3;
    let (first, last) = (median(&secs[..third]), median(&secs[secs.len() - third..]));
    (first > 0.0).then(|| BenchDrift { kind: kind.name().into(), first_secs: first, last_secs: last, percent: (last - first) / first * 100.0 })
}

struct Bench<'a> {
    inner: &'a Inner,
    vcp: ttv_vcp::Client,
    tools: BenchTools,
    voice: Option<String>,
    /// Audio to listen to: the last designs and sentences.
    clips: Vec<Vec<u8>>,
    next: usize,
    vram_free: Option<u32>,
    vram_total: Option<u32>,
    /// The model at work, and whether the engine has to free memory itself.
    last: Option<Kind>,
    managed: bool,
}

impl Bench<'_> {
    /// One model in memory at a time, as in real work (see `models`).
    async fn switch_to(&mut self, kind: Kind) {
        let was = self.last.replace(kind);
        if !self.managed || was == Some(kind) {
            return;
        }
        if was == Some(Kind::Listen)
            && let Some(clip) = self.clips.last()
        {
            models::release(&self.vcp, clip, langs::get("en").expect("en")).await;
        }
        if kind == Kind::Listen {
            models::clear(&self.vcp).await;
        }
    }

    /// The next step of `stage`; Err when somebody stopped the benchmark.
    fn tick(&self, stage: &str) -> Result<(), String> {
        if self.inner.bench_stop.load(Ordering::SeqCst) {
            return Err("stopped".into());
        }
        if let Some(p) = self.inner.bench.lock().as_mut() {
            p.stage = stage.to_string();
            p.step = (p.step + 1).min(p.steps);
        }
        Ok(())
    }

    async fn vram(&mut self) -> Option<u32> {
        let v = self.tools.vram.clone()?;
        let (used, total) = v().await?;
        self.vram_total = Some(total);
        let free = total.saturating_sub(used);
        self.vram_free = Some(self.vram_free.map_or(free, |f| f.min(free)));
        Some(used)
    }

    async fn generation(&self, path: &str, body: &serde_json::Value) -> Result<(f32, Vec<u8>), String> {
        let t = Instant::now();
        let r = self.vcp.request(Method::POST, path).json(body).timeout(TIMEOUT).send().await.map_err(|e| e.to_string())?;
        let status = r.status();
        let bytes = r.bytes().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("HTTP {status}"));
        }
        let r: GenerationResult = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let wav = decode_result(r)?;
        Ok((t.elapsed().as_secs_f32(), wav))
    }

    fn keep(&mut self, wav: Vec<u8>) {
        if self.clips.len() >= KEEP {
            self.clips.remove(0);
        }
        self.clips.push(wav);
    }

    /// One call of a kind of work: seconds, seconds of audio.
    async fn call(&mut self, kind: Kind, i: usize) -> Result<(f32, Option<f32>), String> {
        let l = langs::get("en").expect("en");
        let t0 = Instant::now();
        self.switch_to(kind).await;
        let switching = t0.elapsed().as_secs_f32();
        let (secs, audio) = self.work(kind, i, l).await?;
        Ok((secs + switching, audio))
    }

    async fn work(&mut self, kind: Kind, i: usize, l: &'static langs::DesignLang) -> Result<(f32, Option<f32>), String> {
        match kind {
            Kind::Design => {
                let body = serde_json::json!({ "instruct_text": format!("{} {}", DESIGNS[i % DESIGNS.len()], l.accent), "target_text": l.text, "language": l.vcp });
                let (t, wav) = self.generation("/api/v1/generation/design?sync=true", &body).await?;
                let a = ttv_vcp::wav::duration_secs(&wav);
                self.keep(wav);
                Ok((t, a))
            }
            Kind::Speak => {
                let voice = self.voice.clone().ok_or("no voice to speak with")?;
                let body = serde_json::json!({ "voice_id": voice, "target_text": SENTENCES[i % SENTENCES.len()], "language": l.vcp });
                let (t, wav) = self.generation("/api/v1/generation/clone?sync=true", &body).await?;
                let a = ttv_vcp::wav::duration_secs(&wav);
                self.keep(wav);
                Ok((t, a))
            }
            Kind::Listen => {
                if self.clips.is_empty() {
                    return Err("nothing to listen to".into());
                }
                self.next = (self.next + 1) % self.clips.len();
                let wav = &self.clips[self.next];
                let a = ttv_vcp::wav::duration_secs(wav);
                let body = serde_json::json!({ "audio_base64": base64::engine::general_purpose::STANDARD.encode(wav), "language": l.vcp });
                let t = Instant::now();
                let r = self.vcp.request(Method::POST, "/api/v1/transcription/start?sync=true").json(&body).timeout(TIMEOUT).send().await.map_err(|e| e.to_string())?;
                let status = r.status();
                let bytes = r.bytes().await.map_err(|e| e.to_string())?;
                if !status.is_success() {
                    return Err(format!("HTTP {status}"));
                }
                let answer: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                if answer["success"] == false {
                    return Err(answer["error"].as_str().unwrap_or("speech recognition failed").to_string());
                }
                Ok((t.elapsed().as_secs_f32(), a))
            }
        }
    }

    /// `n` calls of one kind, in one go. The first one may switch models or
    /// load one: it's kept apart from the series.
    async fn series(&mut self, stage: &str, kind: Kind, n: u32) -> Result<Samples, String> {
        let mut s = Samples::default();
        self.tick(stage)?;
        // right after a start VCP may refuse work for a few seconds
        for attempt in 1.. {
            match self.call(kind, 0).await {
                Ok((t, _)) => {
                    s.first = Some(t);
                    break;
                }
                Err(e) if attempt < 4 => {
                    log::info!("benchmark: first {}: {e} - trying again", kind.name());
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Err(e) => {
                    log::warn!("benchmark: first {}: {e}", kind.name());
                    return Err(format!("{}: {e}", kind.name()));
                }
            }
        }
        for i in 1..=n as usize {
            self.tick(stage)?;
            match self.call(kind, i).await {
                Ok((t, a)) => s.add(t, a),
                Err(e) => {
                    log::warn!("benchmark: {}: {e}", kind.name());
                    s.failed += 1;
                    if s.failed * 2 > n {
                        return Err(format!("{}: {e}", kind.name()));
                    }
                }
            }
        }
        Ok(s)
    }

    /// Each model at work, in the order a real job uses them.
    async fn round(&mut self, s: Sizes, scale: u32) -> Result<Round, String> {
        let mut series = Vec::new();
        for (kind, n) in [(Kind::Design, s.design), (Kind::Speak, s.speak), (Kind::Listen, s.listen)] {
            let samples = self.series(kind.name(), kind, (n / scale).max(2)).await?;
            let vram = self.vram().await;
            log::info!("benchmark: {} {:.2} s (first {:.1} s), {:?} MB", kind.name(), median(&samples.secs), samples.first.unwrap_or(0.0), vram);
            series.push(samples.series(kind, vram));
        }
        // What every switch between two models costs: the first call after
        // the other model, against that model's usual time.
        let mut switches = Vec::new();
        for from in Kind::ALL {
            for to in Kind::ALL {
                if from == to {
                    continue;
                }
                let usual = secs_of(&series, to);
                let mut extra = Vec::new();
                for i in 0..(s.switch / scale).max(1) {
                    self.tick("switch")?;
                    self.call(from, i as usize).await?;
                    self.tick("switch")?;
                    let (t, _) = self.call(to, i as usize).await?;
                    extra.push((t - usual).max(0.0));
                }
                switches.push(BenchSwitch { from: from.name().into(), to: to.name().into(), secs: median(&extra) });
            }
        }
        Ok(Round { series, switches })
    }

    /// Rounds through all models like a job does: does VCP get slower?
    async fn drift(&mut self, s: Sizes) -> Result<Vec<BenchDrift>, String> {
        let mut all: [(Kind, Vec<f32>); 3] = [(Kind::Design, Vec::new()), (Kind::Listen, Vec::new()), (Kind::Speak, Vec::new())];
        for c in 0..s.cycles as usize {
            for (kind, times) in all.iter_mut() {
                for i in 0..s.per_cycle as usize {
                    self.tick("drift")?;
                    let (t, _) = self.call(*kind, c + i).await?;
                    // the first call pays for the switch
                    if i > 0 {
                        times.push(t);
                    }
                }
            }
        }
        Ok(all.iter().filter_map(|(k, t)| drift(*k, t)).collect())
    }

    async fn restart(&self) -> Result<f32, String> {
        let restart = self.tools.restart.clone().ok_or("VCP can't be restarted from here")?;
        let t = Instant::now();
        let m = restart().await?;
        log::info!("benchmark: {m}");
        Ok(t.elapsed().as_secs_f32())
    }

    /// After a restart: nothing is loaded, and the setting may be another one.
    async fn fresh(&mut self) {
        self.last = None;
        self.managed = models::low_vram(&self.vcp).await == Some(false);
    }

    async fn low_vram(&self) -> Option<bool> {
        let v: serde_json::Value = self.vcp.get("/api/v1/settings/low-vram", Duration::from_secs(10)).await.ok()?;
        v["enabled"].as_bool()
    }

    /// Only right before a restart (see the top of this file).
    async fn set_low_vram(&self, on: bool) -> Result<(), String> {
        let _: serde_json::Value = self.vcp.put(&format!("/api/v1/settings/low-vram?enabled={on}"), &serde_json::json!({}), Duration::from_secs(30)).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Share of the words misheard on real voices. None without clips.
    async fn hearing(&mut self, clips: &[Clip]) -> Result<Option<f32>, String> {
        if clips.is_empty() {
            return Ok(None);
        }
        self.switch_to(Kind::Listen).await;
        let mut wrong = Vec::with_capacity(clips.len());
        for c in clips {
            self.tick("hearing")?;
            let body = serde_json::json!({ "audio_base64": base64::engine::general_purpose::STANDARD.encode(&c.wav), "language": c.lang.vcp });
            let r = self.vcp.request(Method::POST, "/api/v1/transcription/start?sync=true").json(&body).timeout(TIMEOUT).send().await.map_err(|e| e.to_string())?;
            let answer: serde_json::Value = r.json().await.map_err(|e| e.to_string())?;
            wrong.push(langs::error_rate(&c.text, answer["text"].as_str().unwrap_or_default(), c.lang.code));
        }
        Ok(Some(wrong.iter().sum::<f32>() / wrong.len() as f32))
    }

    /// What's selected and a lighter model of the same family, if there is one.
    async fn asr_models(&self) -> Option<(String, Option<String>)> {
        let v: serde_json::Value = self.vcp.get("/api/v1/settings/asr-model", Duration::from_secs(10)).await.ok()?;
        let now = v["current"].as_str()?.to_string();
        let options = v["options"].as_array()?;
        let mine = options.iter().find(|o| o["id"] == now.as_str())?;
        let lighter = options
            .iter()
            .filter(|o| o["family"] == mine["family"] && o["vram_mb"].as_u64() < mine["vram_mb"].as_u64())
            .filter(|o| langs::LANGS.iter().filter(|l| l.asr).all(|l| o["languages"].as_array().is_some_and(|a| a.iter().any(|x| x == l.vcp))))
            .min_by_key(|o| o["vram_mb"].as_u64())
            .and_then(|o| o["id"].as_str().map(str::to_string));
        Some((now, lighter))
    }

    /// Only right before a restart, like the low-VRAM setting.
    async fn set_asr_model(&self, id: &str) -> Result<(), String> {
        let _: serde_json::Value = self.vcp.put(&format!("/api/v1/settings/asr-model?model_id={id}"), &serde_json::json!({}), Duration::from_secs(30)).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// A voice to speak with, of the library if possible.
    async fn test_voice(&mut self) -> Result<(), String> {
        let voices = self.vcp.voices().await.map_err(|e| e.to_string())?;
        let known: std::collections::HashSet<String> = self.inner.library.entries().into_iter().filter(|e| e.lang == "en" && e.quality.as_ref().is_some_and(|q| q.score >= 0.9)).map(|e| e.voice_id).collect();
        let v = voices.iter().find(|v| known.contains(&v.id)).or_else(|| voices.iter().find(|v| v.has_voice_clone_prompt)).or_else(|| voices.first());
        self.voice = v.map(|v| v.id.clone());
        if self.voice.is_none() {
            return Err("Voice Creator Pro has no saved voice to speak with yet.".into());
        }
        Ok(())
    }
}

/// A real voice's sample and what it says.
struct Clip {
    wav: Vec<u8>,
    lang: &'static langs::DesignLang,
    text: String,
}

/// Good voices of every language, from their lossless samples.
fn hearing_clips(inner: &Inner) -> Vec<Clip> {
    let mut clips = Vec::new();
    let entries: Vec<ttv_link::LibraryEntry> = inner.archive.entries().into_iter().chain(inner.library.entries()).filter(|e| e.preset_id != "classic" && e.quality.as_ref().is_some_and(|q| q.score >= 0.9)).collect();
    for lang in langs::LANGS.iter().filter(|l| l.asr) {
        let mut found = 0;
        for e in entries.iter().filter(|e| e.lang == lang.code) {
            if found == HEARING_CLIPS {
                break;
            }
            let Some((samples, rate)) = std::fs::read(inner.sample_path(&e.key)).ok().and_then(|f| crate::pack::decode_flac(&f).ok()) else { continue };
            clips.push(Clip { wav: crate::audio::wav_pcm16(&samples, rate), lang, text: e.ref_text.clone().unwrap_or_else(|| lang.text.to_string()) });
            found += 1;
        }
    }
    clips
}

fn path(inner: &Inner) -> std::path::PathBuf {
    inner.dir.join("benchmark.json")
}

pub(crate) fn history(inner: &Inner) -> BenchHistory {
    crate::read_json(&path(inner)).unwrap_or_default()
}

async fn measure(inner: &Inner, vcp: ttv_vcp::Client, req: &BenchRequest, tools: BenchTools) -> Result<BenchReport, String> {
    let t0 = Instant::now();
    let s = sizes(req.level);
    let restart = req.restart && tools.restart.is_some();
    let mut b = Bench { inner, vcp, tools, voice: None, clips: Vec::new(), next: 0, vram_free: None, vram_total: None, last: None, managed: false };
    let mut report = BenchReport { at: chrono::Utc::now().timestamp(), level: req.level, restarted: restart, ..Default::default() };
    b.test_voice().await?;
    b.fresh().await;
    report.vcp_version = b.vcp.version().await.ok();
    report.hardware = b.vcp.system_info().await.ok();
    report.low_vram = b.low_vram().await;

    // As it runs now: that's the drop in speed a restart would cure.
    if restart {
        for kind in Kind::ALL {
            let samples = b.series("before", kind, s.before).await?;
            let vram = b.vram().await;
            report.before.push(samples.series(kind, vram));
        }
        b.tick("restart")?;
        report.restart_secs = Some(b.restart().await?);
        // the old models are gone: the next calls show what loading costs
        b.fresh().await;
    }
    let round = b.round(s, 1).await?;
    report.drift = b.drift(s).await?;
    let mut advice = advise(&round.series, &round.switches);
    let (voice, _) = work(&round.series, &round.switches);
    let free_now = b.vram_free;

    // Other settings, each from a fresh start; the best one stays.
    if s.variant && restart {
        let clips = hearing_clips(inner);
        report.listen_wer = b.hearing(&clips).await?;
        let models = b.asr_models().await;
        report.asr_model = models.as_ref().map(|m| m.0.clone());
        let mut low_vram = report.low_vram;
        if let Some(now) = report.low_vram {
            b.tick("variant")?;
            let tried = variant(&mut b, s, !now, None, &[]).await;
            let better = tried.error.is_none() && tried.voice_secs > 0.0 && tried.voice_secs < voice * 0.97;
            advice.low_vram = Some(if better { !now } else { now });
            low_vram = advice.low_vram;
            if better {
                log::info!("benchmark: low VRAM {} is faster ({:.1} instead of {voice:.1} s per voice)", !now, tried.voice_secs);
            }
            b.set_low_vram(low_vram.unwrap_or(now)).await?;
            report.variants.push(tried);
        }
        // A lighter speech recognition model: does it hear well enough, and
        // do the models then fit into the video memory together?
        if let Some((now, Some(lighter))) = models {
            b.tick("variant")?;
            let tried = variant(&mut b, s, low_vram.unwrap_or(false), Some(lighter.clone()), &clips).await;
            let hears = matches!((tried.listen_wer, report.listen_wer), (Some(a), Some(b)) if a <= b + WER_MARGIN);
            let crowded = free_now.is_some_and(|f| f < VRAM_MARGIN);
            let fits = tried.vram_free_mb.is_some_and(|f| f >= VRAM_MARGIN);
            let better = tried.error.is_none() && hears && (tried.voice_secs < voice * 0.97 || (crowded && fits));
            advice.asr_model = Some(if better { lighter } else { now });
            log::info!("benchmark: speech recognition: {:?}", advice.asr_model);
            if let Some(id) = &advice.asr_model {
                b.set_asr_model(id).await?;
            }
            report.variants.push(tried);
        }
        b.tick("variant")?;
        b.restart().await?;
        b.fresh().await;
    }
    report.series = round.series;
    report.switches = round.switches;
    report.advice = advice;
    report.vram_total_mb = b.vram_total;
    report.vram_free_mb = free_now;
    report.secs = t0.elapsed().as_secs() as u32;

    // VCP stays warm for the next chat message.
    b.tick("done")?;
    if let Err(e) = b.call(Kind::Speak, 0).await {
        log::warn!("benchmark: warming up: {e}");
    }
    Ok(report)
}

async fn variant(b: &mut Bench<'_>, s: Sizes, low_vram: bool, asr_model: Option<String>, clips: &[Clip]) -> BenchVariant {
    let mut v = BenchVariant { low_vram, asr_model: asr_model.clone(), ..Default::default() };
    b.vram_free = None;
    let result: Result<(Round, Option<f32>), String> = async {
        b.set_low_vram(low_vram).await?;
        if let Some(id) = &asr_model {
            b.set_asr_model(id).await?;
        }
        b.tick("variant")?;
        b.restart().await?;
        b.fresh().await;
        let heard = b.hearing(clips).await?;
        Ok((b.round(s, 2).await?, heard))
    }
    .await;
    v.vram_free_mb = b.vram_free;
    match result {
        Ok((r, heard)) => {
            let (voice, cycle) = work(&r.series, &r.switches);
            v.voice_secs = voice + cycle / MAX_BATCH as f32;
            v.listen_wer = heard;
            v.series = r.series;
            v.switches = r.switches;
        }
        Err(e) => {
            log::warn!("benchmark: with low VRAM {low_vram}: {e}");
            v.error = Some(e);
        }
    }
    v
}

/// Ends the exclusive mode, also if the benchmark fails or panics.
struct Exclusive(Arc<Inner>);

impl Drop for Exclusive {
    fn drop(&mut self) {
        ttv_vcp::set_exclusive(false);
        self.0.hold.store(false, Ordering::SeqCst);
        self.0.wake_up();
    }
}

pub(crate) fn start(inner: &Arc<Inner>, req: BenchRequest, tools: BenchTools) -> Result<(), String> {
    let vcp = inner.vcp.read().clone().ok_or("Voice Creator Pro is not connected")?;
    {
        let mut slot = inner.bench.lock();
        if slot.as_ref().is_some_and(|p| p.running) {
            return Err("The benchmark is running already.".into());
        }
        *slot = Some(BenchProgress { running: true, level: req.level, stage: "wait".into(), step: 0, steps: steps(sizes(req.level), req.restart && tools.restart.is_some()), started: chrono::Utc::now().timestamp(), error: None });
    }
    inner.bench_stop.store(false, Ordering::SeqCst);
    inner.hold.store(true, Ordering::SeqCst);
    ttv_vcp::set_exclusive(true);
    let inner = inner.clone();
    tokio::spawn(async move {
        let _exclusive = Exclusive(inner.clone());
        log::info!("benchmark ({:?}) starts: nothing else uses VCP until it ends", req.level);
        // what runs right now finishes first
        let turn = ttv_vcp::turn("/ttv/benchmark").await;
        let result = measure(&inner, vcp, &req, tools).await;
        drop(turn);
        let error = match result {
            Ok(report) => {
                log::info!(
                    "benchmark done in {} s: {:.1} s per voice, batches of {}, design {:.2} s, speak {:.2} s, listen {:.2} s",
                    report.secs,
                    report.advice.voice_secs,
                    report.advice.batch_voices,
                    secs_of(&report.series, Kind::Design),
                    secs_of(&report.series, Kind::Speak),
                    secs_of(&report.series, Kind::Listen)
                );
                adopt(&inner, report);
                None
            }
            Err(e) => {
                log::warn!("benchmark: {e}");
                Some(e)
            }
        };
        if let Some(p) = inner.bench.lock().as_mut() {
            p.running = false;
            p.stage = "done".into();
            if error.is_none() {
                p.step = p.steps;
            }
            p.error = error;
        }
    });
    Ok(())
}

/// The engine works with these numbers from now on.
fn adopt(inner: &Inner, report: BenchReport) {
    {
        let mut s = inner.stats.lock();
        s.batch_voices = Some(report.advice.batch_voices);
        s.limits = Some([report.advice.design_limit, report.advice.listen_limit]);
    }
    inner.save_stats();
    inner.speed_reset();
    if let Some(r) = report.series.iter().find(|s| s.kind == "speak").map(|s| s.rtf).filter(|r| *r > 0.0) {
        ttv_vcp::perf::set_usual_now(r);
    }
    let mut h = history(inner);
    h.reports.insert(0, report);
    h.reports.truncate(KEEP);
    crate::write_json(&path(inner), &h);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(kind: Kind, median_secs: f32) -> BenchSeries {
        BenchSeries { kind: kind.name().into(), median_secs, p90_secs: median_secs * 1.1, ..Default::default() }
    }

    #[test]
    fn the_batch_grows_with_what_switching_costs() {
        let s = [series(Kind::Design, 3.3), series(Kind::Speak, 1.7), series(Kind::Listen, 0.6)];
        let sw = |secs: f32| Kind::ALL.iter().flat_map(|a| Kind::ALL.iter().map(move |b| (*a, *b))).filter(|(a, b)| a != b).map(|(a, b)| BenchSwitch { from: a.name().into(), to: b.name().into(), secs }).collect::<Vec<_>>();
        // 2 × (3.3 + 0.6) + 1.7 + 0.6 = 10.1 s per voice
        let cheap = advise(&s, &sw(1.0));
        assert_eq!(cheap.batch_voices, MIN_BATCH, "{cheap:?}");
        assert!((cheap.voice_secs - (10.1 + 4.0 / 48.0)).abs() < 0.01, "{cheap:?}");
        // 4 × 12 s per batch: 48 / (0.02 × 10.1) = 238 voices
        let dear = advise(&s, &sw(12.0));
        assert_eq!(dear.batch_voices, MAX_BATCH, "{dear:?}");
        assert!(dear.switch_percent < 2.5, "{dear:?}");
        // a drop of more than 2.5 times counts
        assert!((dear.design_limit - 8.25).abs() < 0.01, "{dear:?}");
    }

    #[test]
    fn drift_compares_the_first_and_the_last_third() {
        let d = drift(Kind::Design, &[3.0, 3.0, 3.1, 3.2, 4.4, 4.5]).unwrap();
        assert!((d.percent - 50.0).abs() < 0.1, "{d:?}");
        assert!(drift(Kind::Design, &[3.0, 3.0]).is_none());
    }

    #[test]
    fn the_cost_of_a_request_comes_from_the_fit() {
        // 0.3 s plus half a second per second of audio
        let audio = [2.0, 4.0, 8.0, 2.0, 4.0, 8.0];
        let secs: Vec<f32> = audio.iter().map(|a| 0.3 + a * 0.5).collect();
        assert!((overhead(&secs, &audio).unwrap() - 0.3).abs() < 0.01);
        assert_eq!(overhead(&[1.0, 2.0], &[1.0, 2.0]), None, "too few");
        assert_eq!(overhead(&[1.0, 1.1, 0.9], &[3.0, 3.0, 3.0]), None, "all the same length");
    }

    #[test]
    fn the_levels_get_more_thorough() {
        let q = sizes(BenchLevel::Quick);
        let s = sizes(BenchLevel::Standard);
        let t = sizes(BenchLevel::Thorough);
        assert!(steps(q, true) < steps(s, true) && steps(s, true) < steps(t, true));
        assert!(steps(s, false) < steps(s, true));
        assert!(t.variant && !s.variant);
    }
}
