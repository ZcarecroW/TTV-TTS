//! The engine against a fake Voice Creator Pro: a whole job, broken speech
//! recognition, a voice VCP can't speak with, checking the library, and the
//! quality check (odd voices, improving them, checking existing ones).
//!
//! The fake sounds a bit like the real thing: designs are sines at a pitch
//! that follows the gender in the instruction (a man 115 Hz, a woman 210 Hz);
//! a description containing "WEIRD" makes the designer produce a 400 Hz
//! voice (as often as `weird` allows); speech recognition hears the reference
//! sentence in long clips and the priming sentence in short ones.

use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use parking_lot::Mutex;
use serde_json::{Value, json};
use ttv_engine::Engine;
use ttv_engine::quality::FLAG;
use ttv_link::{JobDef, JobKind, JobOptions, JobRequest, JobState, VoiceDef};

struct FakeVoice {
    name: String,
    hz: f32,
}

#[derive(Default)]
struct Fake {
    voices: Mutex<BTreeMap<String, FakeVoice>>,
    /// pitch of every designed sample, by a hash of its base64
    designed: Mutex<HashMap<u64, f32>>,
    next: AtomicU32,
    asr_broken: AtomicBool,
    clone_broken: AtomicBool,
    clones: AtomicU32,
    designs: AtomicU32,
    /// How many more "WEIRD" designs come out odd (negative: always).
    weird: AtomicI32,
    /// How many more transcriptions are refused as if VCP had gone away.
    asr_down: AtomicU32,
    low_vram: AtomicBool,
    /// The lighter speech recognition model is selected.
    light_asr: AtomicBool,
    /// How much the lighter model mishears: words it gets wrong per 10.
    light_asr_errors: AtomicU32,
    transcriptions: AtomicU32,
    /// What's in the video memory, and how often two models were at once.
    voice_loaded: AtomicBool,
    asr_loaded: AtomicBool,
    collisions: AtomicU32,
    /// New sentences come out much higher, but not what the sample says.
    flaky_new: AtomicBool,
    /// Everything a saved voice says comes out much higher.
    off_pitch: AtomicBool,
}

type S = State<Arc<Fake>>;

/// Pitch of designs whose description contains "WHISPER".
const WHISPER_HZ: f32 = 333.0;

fn hash(s: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Mono PCM16 "speech" at −20 dBFS RMS (gain 1.0): a sine in phrases of
/// 1.2 s with 0.2 s gaps, 0.25 s of silence before and after (like VCP output;
/// without gaps the quality check rightly hears noise everywhere).
fn wav(secs: f32, hz: f32) -> String {
    let rate = 24_000u32;
    let n = (secs * rate as f32) as u32;
    let mut b = Vec::with_capacity(44 + n as usize * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 2).to_le_bytes());
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let talking = t > 0.25 && t < secs - 0.25 && (t - 0.25) % 1.4 < 1.2;
        let s = if talking { (t * hz * std::f32::consts::TAU).sin() * 0.1414 } else { 0.0 };
        b.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(b)
}

fn lang_of(b: &Value) -> &'static ttv_engine::langs::DesignLang {
    let name = b["language"].as_str().unwrap_or("English");
    ttv_engine::langs::LANGS.iter().find(|l| l.vcp == name).unwrap_or(&ttv_engine::langs::LANGS[0])
}

// Handlers read the body like VCP does (an unread upload resets the connection).
/// A voice model comes into memory, where speech recognition may still be.
fn load_voice_model(f: &Fake) {
    f.voice_loaded.store(true, Ordering::SeqCst);
    if f.asr_loaded.load(Ordering::SeqCst) {
        f.collisions.fetch_add(1, Ordering::SeqCst);
    }
}

async fn design(State(f): S, Json(b): Json<Value>) -> Json<Value> {
    f.designs.fetch_add(1, Ordering::SeqCst);
    load_voice_model(&f);
    let instruct = b["instruct_text"].as_str().unwrap_or_default();
    let weird = instruct.contains("WEIRD") && f.weird.try_update(Ordering::SeqCst, Ordering::SeqCst, |w| if w == 0 { None } else { Some(if w > 0 { w - 1 } else { w }) }).is_ok();
    let hz = if weird {
        400.0
    } else if instruct.contains("WHISPER") {
        WHISPER_HZ
    } else if instruct.contains("-year-old man.") {
        115.0
    } else if instruct.contains("-year-old woman.") {
        210.0
    } else {
        150.0
    };
    let audio = wav(7.0, hz);
    f.designed.lock().insert(hash(&audio), hz);
    Json(json!({ "success": true, "status": "completed", "audio_base64": audio }))
}

async fn low_vram(State(f): S) -> Json<Value> {
    Json(json!({ "enabled": f.low_vram.load(Ordering::SeqCst) }))
}

async fn set_low_vram(State(f): S, Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    f.low_vram.store(q.get("enabled").is_some_and(|v| v == "true"), Ordering::SeqCst);
    // "Toggle low VRAM mode. Unloads the current TTS model if one is loaded."
    f.voice_loaded.store(false, Ordering::SeqCst);
    Json(json!({ "success": true, "changed": true, "needs_reload": true }))
}

async fn asr_models(State(f): S) -> Json<Value> {
    let langs: Vec<&str> = ttv_engine::langs::LANGS.iter().map(|l| l.vcp).collect();
    Json(json!({
        "current": if f.light_asr.load(Ordering::SeqCst) { "standard" } else { "higher_quality" },
        "options": [
            { "id": "standard", "family": "Qwen3-ASR", "vram_mb": 1700, "languages": langs },
            { "id": "higher_quality", "family": "Qwen3-ASR", "vram_mb": 4000, "languages": langs },
            { "id": "parakeet_v3", "family": "Parakeet v3", "vram_mb": 5000, "languages": ["English"] },
        ],
    }))
}

async fn set_asr_model(State(f): S, Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    f.light_asr.store(q.get("model_id").is_some_and(|v| v == "standard"), Ordering::SeqCst);
    Json(json!({ "success": true, "changed": true }))
}

async fn transcribe(State(f): S, Json(b): Json<Value>) -> axum::response::Response {
    // like the real thing: low VRAM mode reloads speech recognition every time
    if f.low_vram.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
    if f.asr_down.try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    transcribed(f, b).await.into_response()
}

async fn transcribed(f: Arc<Fake>, b: Value) -> Json<Value> {
    // with low VRAM on, speech recognition is released right after a clip
    let low = f.low_vram.load(Ordering::SeqCst);
    if !low {
        f.transcriptions.fetch_add(1, Ordering::SeqCst);
    }
    if f.voice_loaded.load(Ordering::SeqCst) {
        f.collisions.fetch_add(1, Ordering::SeqCst);
    }
    f.asr_loaded.store(!low, Ordering::SeqCst);
    if f.asr_broken.load(Ordering::SeqCst) {
        return Json(json!({ "success": false, "text": null, "error": "ASR model failed to load" }));
    }
    let audio = b["audio_base64"].as_str().unwrap_or_default();
    // a whispered design: the recognizer hears mumbling
    if f.designed.lock().get(&hash(audio)) == Some(&WHISPER_HZ) {
        return Json(json!({ "success": true, "text": "mm hm" }));
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(audio).unwrap_or_default();
    let secs = bytes.len().saturating_sub(44) as f32 / 48_000.0;
    let l = lang_of(&b);
    if f.light_asr.load(Ordering::SeqCst) && f.light_asr_errors.load(Ordering::SeqCst) > 0 {
        return Json(json!({ "success": true, "text": "something completely different" }));
    }
    Json(json!({ "success": true, "text": if secs > 5.0 { l.text } else { l.verify } }))
}

async fn save(State(f): S, Json(b): Json<Value>) -> Json<Value> {
    let id = format!("voice_{:08x}", f.next.fetch_add(1, Ordering::SeqCst));
    let name = b["name"].as_str().unwrap_or_default().to_string();
    let hz = f.designed.lock().get(&hash(b["audio_base64"].as_str().unwrap_or_default())).copied().unwrap_or(150.0);
    f.voices.lock().insert(id.clone(), FakeVoice { name: name.clone(), hz });
    Json(json!({ "success": true, "voice": { "id": id, "name": name }, "message": "saved" }))
}

async fn clone(State(f): S, Json(b): Json<Value>) -> Json<Value> {
    f.clones.fetch_add(1, Ordering::SeqCst);
    load_voice_model(&f);
    if f.clone_broken.load(Ordering::SeqCst) {
        return Json(json!({ "success": false, "status": "failed", "error_detail": "Generation failed" }));
    }
    let hz = f.voices.lock().get(b["voice_id"].as_str().unwrap_or_default()).map_or(150.0, |v| v.hz);
    // what the sample says: as long as the sample
    let again = b["target_text"].as_str() == Some(lang_of(&b).text);
    let off = f.off_pitch.load(Ordering::SeqCst) || (f.flaky_new.load(Ordering::SeqCst) && !again);
    Json(json!({ "success": true, "status": "completed", "audio_base64": wav(if again { 7.0 } else { 2.5 }, if off { hz * 2.2 } else { hz }) }))
}

async fn list(State(f): S) -> Json<Value> {
    let voices: Vec<Value> = f.voices.lock().iter().map(|(id, v)| json!({ "id": id, "name": v.name })).collect();
    Json(json!({ "voices": voices }))
}

async fn one(State(f): S, Path(id): Path<String>) -> Json<Value> {
    // Every fake reference sample is at −20 dBFS.
    let hz = f.voices.lock().get(&id).map_or(150.0, |v| v.hz);
    Json(json!({ "id": id, "audio_base64": wav(7.0, hz), "ref_text": ttv_engine::langs::get("en").unwrap().text }))
}

async fn remove(State(f): S, Path(id): Path<String>) -> Json<Value> {
    f.voices.lock().remove(&id);
    Json(json!({ "success": true }))
}

async fn serve() -> (Arc<Fake>, String) {
    let fake = Arc::new(Fake::default());
    let app = Router::new()
        .route("/api/v1/generation/design", post(design))
        .route("/api/v1/transcription/start", post(transcribe))
        .route("/api/v1/voices", post(save).get(list))
        .route("/api/v1/voices/{id}", get(one).delete(remove))
        .route("/api/v1/generation/clone", post(clone))
        .route("/api/v1/settings/low-vram", get(low_vram).put(set_low_vram))
        .route("/api/v1/settings/asr-model", get(asr_models).put(set_asr_model))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (fake, base)
}

fn engine(base: &str) -> Engine {
    let dir = std::env::temp_dir().join(format!("ttv-engine-test-{}", rand::random::<u64>()));
    let e = Engine::new(dir, Arc::new(|| None));
    e.set_vcp(Some(ttv_vcp::Client::new(base, None)));
    tokio::spawn(e.worker());
    e
}

fn def(i: u32, gender: &str, description: &str) -> JobDef {
    JobDef { index: i, def: VoiceDef { name: format!("Voice {i}"), gender: gender.into(), age: 30, description: description.into(), fx: None } }
}

fn request(voices: u32) -> JobRequest {
    JobRequest {
        preset_id: "test".into(),
        preset_name: "Test".into(),
        defs: (0..voices).map(|i| def(i, "f", &format!("A clear, friendly test voice number {i}."))).collect(),
        languages: vec!["en".into()],
        options: JobOptions { speech_check: true, takes: 1, backup: false, ..Default::default() },
        ..Default::default()
    }
}

/// A man and a man whose design comes out like a child (400 Hz).
fn odd_request(quality: bool, improve: bool) -> JobRequest {
    JobRequest {
        defs: vec![def(0, "m", "Calm, warm baritone of a radio host."), def(1, "m", "Punchy action-hero baritone. WEIRD")],
        options: JobOptions { speech_check: true, takes: 1, backup: false, quality, improve, ..Default::default() },
        ..request(0)
    }
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(120), "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

fn done(e: &Engine) -> bool {
    let s = e.status();
    !s.jobs.is_empty() && s.jobs.iter().all(|j| j.state == JobState::Done)
}

#[tokio::test(flavor = "multi_thread")]
async fn computes_checks_saves_and_primes_every_voice() {
    let (fake, base) = serve().await;
    let e = engine(&base);
    e.submit(request(3)).unwrap();
    until("the job", || e.status().jobs[0].state == JobState::Done).await;

    let job = &e.status().jobs[0];
    assert_eq!((job.done, job.failed, job.error.as_deref()), (3, 0, None));
    let lib = e.library().entries;
    let voices = fake.voices.lock();
    // VCP holds exactly the library's voices – no orphans, none missing.
    assert_eq!(voices.len(), 3);
    assert!(lib.iter().all(|x| voices.contains_key(&x.voice_id)));
    assert_eq!(fake.clones.load(Ordering::SeqCst), 3, "one prime per voice");
    assert!(lib.iter().all(|x| x.wer == Some(0.0) && (x.gain - 1.0).abs() < 0.1 && x.level.is_some()), "{lib:?}");
    assert!(lib.iter().all(|x| x.quality.is_none()), "no quality check unless asked for");
}

#[tokio::test(flavor = "multi_thread")]
async fn broken_speech_recognition_saves_nothing() {
    let (fake, base) = serve().await;
    fake.asr_broken.store(true, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(request(2)).unwrap();
    until("the error", || e.status().jobs[0].error.is_some()).await;

    let job = &e.status().jobs[0];
    assert!(job.error.as_deref().unwrap().contains("speech recognition"), "{:?}", job.error);
    // Nothing unchecked was saved, and no voice counts as failed.
    assert_eq!((job.done, job.failed), (0, 0));
    assert!(e.library().entries.is_empty());
    assert!(fake.voices.lock().is_empty());
    // The worker backs off (10 s, then longer) instead of designing the batch
    // again and again. A slow CI runner may still have one batch in flight
    // when the error shows: allow that one, then nothing more.
    let designs = fake.designs.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let later = fake.designs.load(Ordering::SeqCst);
    assert!(later <= designs + 2, "{designs} → {later}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(fake.designs.load(Ordering::SeqCst), later, "no retry loop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_voice_vcp_cannot_speak_with_is_deleted_again() {
    let (fake, base) = serve().await;
    fake.clone_broken.store(true, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(request(1)).unwrap();
    until("the job", || e.status().jobs[0].state == JobState::Done).await;
    let job = &e.status().jobs[0];
    assert!(job.error.as_deref().unwrap().contains("could not speak"), "{:?}", job.error);
    // Three attempts, each saved, primed twice, deleted: the voice counts as
    // failed, the job ends, and VCP keeps no orphan.
    assert_eq!((job.done, job.failed), (0, 1));
    assert_eq!(fake.clones.load(Ordering::SeqCst), 6);
    assert!(fake.voices.lock().is_empty());
    assert!(e.library().entries.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_removes_voices_deleted_in_vcp() {
    let (fake, base) = serve().await;
    let e = engine(&base);
    e.submit(request(2)).unwrap();
    until("the job", || e.status().jobs[0].state == JobState::Done).await;
    let first = fake.voices.lock().keys().next().cloned().unwrap();
    fake.voices.lock().remove(&first);

    let r = e.verify().await.unwrap();
    assert_eq!((r.checked, r.removed, r.measured), (2, 1, 0));
    assert_eq!(e.library().entries.len(), 1);
    assert_eq!(e.verify().await.unwrap().removed, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn verify_measures_the_loudness_of_adopted_voices_once() {
    let (fake, base) = serve().await;
    let e = engine(&base);
    fake.voices.lock().insert("voice_old".into(), FakeVoice { name: "TTS-de 01 Anna".into(), hz: 200.0 });
    let adopted = ttv_link::LibraryEntry { key: "vabc:de".into(), lang: "de".into(), voice_id: "voice_old".into(), gain: 1.0, ..Default::default() };
    assert_eq!(e.adopt(vec![adopted]), 1);

    let r = e.verify().await.unwrap();
    assert_eq!((r.removed, r.measured), (0, 1));
    let x = &e.library().entries[0];
    assert!(x.level.is_some_and(|db| (db + 20.0).abs() < 0.5), "{x:?}");
    assert!((x.gain - 1.0).abs() < 0.06);
    assert_eq!(e.verify().await.unwrap().measured, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_quality_check_lists_odd_voices() {
    let (fake, base) = serve().await;
    fake.weird.store(-1, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(odd_request(true, false)).unwrap();
    until("the job", || done(&e)).await;

    let job = &e.status().jobs[0];
    assert_eq!((job.done, job.flagged), (2, 1));
    let lib = e.library().entries;
    let fine = lib.iter().find(|x| x.index == 0).unwrap().quality.clone().unwrap();
    let odd = lib.iter().find(|x| x.index == 1).unwrap().quality.clone().unwrap();
    assert!(fine.score >= FLAG && fine.new_text, "{fine:?}");
    assert!((fine.f0.unwrap() - 115.0).abs() < 3.0);
    assert!(odd.score < FLAG && odd.issues.iter().any(|i| i.code == "pitch_high"), "{odd:?}");
    // keeping it takes it off the list
    assert_eq!(e.keep(&[lib.iter().find(|x| x.index == 1).unwrap().key.clone()], true), 1);
    assert!(e.library().entries.iter().find(|x| x.index == 1).unwrap().quality.as_ref().unwrap().kept);
    assert_eq!(fake.voices.lock().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn odd_voices_are_improved_automatically() {
    let (fake, base) = serve().await;
    // only the first design of the odd voice comes out odd
    fake.weird.store(5, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(odd_request(true, true)).unwrap();
    until("both jobs", || e.status().jobs.len() == 2 && done(&e)).await;

    let jobs = e.status().jobs;
    let improve = jobs.iter().find(|j| j.kind == JobKind::Improve).expect("follow-up job");
    assert_eq!((improve.total, improve.done, improve.improved), (1, 1, 1));
    let lib = e.library().entries;
    let odd = lib.iter().find(|x| x.index == 1).unwrap();
    let q = odd.quality.as_ref().unwrap();
    assert!(q.score >= FLAG && (q.f0.unwrap() - 115.0).abs() < 3.0, "{q:?}");
    // The old voice is gone from VCP, the new one is in the library: no orphans.
    let voices = fake.voices.lock();
    assert_eq!(voices.len(), 2);
    assert!(lib.iter().all(|x| voices.contains_key(&x.voice_id)));
}

#[tokio::test(flavor = "multi_thread")]
async fn improving_keeps_the_old_voice_unless_the_new_one_is_better() {
    let (fake, base) = serve().await;
    fake.weird.store(-1, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(odd_request(true, false)).unwrap();
    until("the job", || done(&e)).await;
    let before = e.library().entries.into_iter().find(|x| x.index == 1).unwrap();

    let req = JobRequest { kind: JobKind::Improve, only: vec![before.key.clone()], rounds: 2, ..odd_request(true, false) };
    let v = e.submit(req).unwrap();
    assert_eq!(v.total, 1);
    until("the improve job", || done(&e)).await;
    let job = e.status().jobs.into_iter().find(|j| j.kind == JobKind::Improve).unwrap();
    assert_eq!((job.done, job.improved), (1, 0));
    // two rounds of 2 takes, nothing better: the old voice stays, the
    // candidates are deleted again
    let after = e.library().entries.into_iter().find(|x| x.index == 1).unwrap();
    assert_eq!(after.voice_id, before.voice_id);
    assert_eq!(fake.voices.lock().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn checks_the_quality_of_existing_voices() {
    let (fake, base) = serve().await;
    let e = engine(&base);
    // a 1.x voice: a man who sounds like a child, never checked
    fake.voices.lock().insert("voice_old".into(), FakeVoice { name: "TTS-en 01 Jack (Detektiv)".into(), hz: 390.0 });
    let d = def(0, "m", "Hard-boiled detective.");
    let key = d.def.key("en");
    e.adopt(vec![ttv_link::LibraryEntry { key: key.clone(), def_hash: d.def.hash(), lang: "en".into(), voice_id: "voice_old".into(), gain: 1.0, ..Default::default() }]);

    let req = JobRequest { kind: JobKind::Review, defs: vec![d], ..request(0) };
    assert_eq!(e.submit(req).unwrap().total, 1);
    until("the review", || done(&e)).await;
    let job = &e.status().jobs[0];
    assert_eq!((job.done, job.flagged), (1, 1));
    let x = e.library().entries.into_iter().next().unwrap();
    let q = x.quality.unwrap();
    assert!(q.new_text && q.score < FLAG && q.issues.iter().any(|i| i.code == "pitch_high"), "{q:?}");
    assert_eq!(x.wer, Some(0.0), "the sample was transcribed");
    assert!(x.level.is_some() && x.voice_id == "voice_old");
    assert_eq!(fake.designs.load(Ordering::SeqCst), 0, "checking designs nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pack_moves_voices_to_another_vcp() {
    // A computes two voices (with lossless backups) and exports them
    let (_fake_a, base_a) = serve().await;
    let a = engine(&base_a);
    let mut req = request(2);
    req.options.backup = true;
    a.submit(req.clone()).unwrap();
    until("the job", || done(&a)).await;
    let voices: Vec<ttv_link::ExportVoice> = req
        .defs
        .iter()
        .map(|d| ttv_link::ExportVoice { key: d.def.key("en"), preset_id: "test".into(), index: d.index, name: d.def.name.clone(), description: d.def.description.clone() })
        .collect();
    let preset = ttv_link::PackPreset { id: "test".into(), name: "Test".into(), builtin: false, text: "#ttv-tts preset v1\nname: Test\n---\n".into() };
    let path = a.export(ttv_link::ExportRequest { title: "Two test voices".into(), presets: vec![preset], voices }, "test").await.unwrap();
    let m = ttv_engine::pack::read_manifest(&path).unwrap();
    assert_eq!((m.voices.len(), m.title.as_str()), (2, "Two test voices"));
    assert!(m.voices.iter().all(|v| v.ref_text == ttv_engine::langs::get("en").unwrap().text && v.description.starts_with("A clear")));

    // B imports the pack: saved in its VCP, primed, in its library
    let (fake_b, base_b) = serve().await;
    let b = engine(&base_b);
    let job = b.submit(JobRequest { kind: JobKind::Import, pack: path.to_string_lossy().into(), ..Default::default() }).unwrap();
    assert_eq!(job.total, 2);
    until("the import", || done(&b)).await;
    let lib = b.library().entries;
    assert_eq!(lib.len(), 2);
    let vb = fake_b.voices.lock();
    assert!(lib.iter().all(|x| vb.contains_key(&x.voice_id) && x.backup && (x.gain - 1.0).abs() < 0.1), "{lib:?}");
    assert_eq!(fake_b.clones.load(Ordering::SeqCst), 2, "each imported voice was primed");
    assert_eq!(fake_b.designs.load(Ordering::SeqCst), 0, "nothing designed");
    drop(vb);
    // the same pack again: nothing to do
    assert_eq!(b.submit(JobRequest { kind: JobKind::Import, pack: path.to_string_lossy().into(), ..Default::default() }).unwrap().total, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn voices_left_unchecked_get_a_check_job() {
    let (fake, base) = serve().await;
    fake.weird.store(-1, Ordering::SeqCst);
    let e = engine(&base);
    // as if a restart had cut the review: the voices exist, without reports
    let mut req = odd_request(true, true);
    req.options.quality = false;
    e.submit(req).unwrap();
    until("the job", || done(&e)).await;
    assert!(e.library().entries.iter().all(|x| x.quality.is_none()));
    // the same voices, now a quality job that "created" them: emulate by
    // checking them directly the way the follow-up would
    let keys: Vec<String> = e.library().entries.iter().map(|x| x.key.clone()).collect();
    let review = JobRequest { kind: JobKind::Review, only: keys, options: JobOptions { quality: true, improve: true, ..odd_request(true, true).options }, ..odd_request(true, true) };
    e.submit(review).unwrap();
    until("check and improve", || e.status().jobs.len() == 3 && done(&e)).await;
    let jobs = e.status().jobs;
    assert!(jobs.iter().any(|j| j.kind == JobKind::Review && j.flagged == 1), "{jobs:?}");
    assert!(jobs.iter().any(|j| j.kind == JobKind::Improve && j.total == 1), "the check queued the improvement");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_voices_are_not_checked_twice_at_once() {
    let (_fake, base) = serve().await;
    let dir = std::env::temp_dir().join(format!("ttv-engine-test-{}", rand::random::<u64>()));
    // no worker: jobs stay queued
    let e = Engine::new(dir, Arc::new(|| None));
    e.set_vcp(Some(ttv_vcp::Client::new(&base, None)));
    let d = def(0, "m", "Hard-boiled detective.");
    let key = d.def.key("en");
    e.adopt(vec![ttv_link::LibraryEntry { key: key.clone(), def_hash: d.def.hash(), lang: "en".into(), voice_id: "v".into(), gain: 1.0, ..Default::default() }]);
    let req = JobRequest { kind: JobKind::Review, defs: vec![d], only: vec![key], ..request(0) };
    assert_eq!(e.submit(req.clone()).unwrap().total, 1);
    assert!(e.submit(req).unwrap_err().contains("already"));
}

#[tokio::test(flavor = "multi_thread")]
async fn after_chat_only_speaking_goes_on() {
    // chat 20 s ago: past the 6 s gap, within the 90 s gap for model switches
    let (fake, base) = serve().await;
    let dir = std::env::temp_dir().join(format!("ttv-engine-test-{}", rand::random::<u64>()));
    let chat = std::time::Instant::now() - Duration::from_secs(20);
    let e = Engine::new(dir, Arc::new(move || Some(chat)));
    e.set_vcp(Some(ttv_vcp::Client::new(&base, None)));
    tokio::spawn(e.worker());
    fake.voices.lock().insert("voice_old".into(), FakeVoice { name: "TTS-en 01 Jack (Detektiv)".into(), hz: 115.0 });
    let d = def(0, "m", "Hard-boiled detective.");
    let key = d.def.key("en");
    e.adopt(vec![ttv_link::LibraryEntry { key: key.clone(), def_hash: d.def.hash(), lang: "en".into(), voice_id: "voice_old".into(), gain: 1.0, ..Default::default() }]);
    // a check speaks with the voice (speaking checkpoint, like chat) ...
    e.submit(JobRequest { kind: JobKind::Review, defs: vec![d], ..request(0) }).unwrap();
    until("the check speaks", || fake.clones.load(Ordering::SeqCst) >= 1).await;
    // ... but listens (speech recognition) only after a longer quiet
    until("the check waits", || e.status().jobs.iter().any(|j| j.kind == JobKind::Review && j.state == JobState::Waiting && j.phase == "review")).await;
    // designing waits for a quieter chat, too
    e.submit(request(1)).unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(fake.designs.load(Ordering::SeqCst), 0);
    assert!(e.status().jobs.iter().all(|j| j.state != JobState::Done), "{:?}", e.status().jobs);
}

#[tokio::test(flavor = "multi_thread")]
async fn let_me_pick_keeps_the_takes_until_one_is_chosen() {
    let (fake, base) = serve().await;
    let e = engine(&base);
    fake.voices.lock().insert("voice_old".into(), FakeVoice { name: "TTV old".into(), hz: 115.0 });
    let d = def(0, "m", "Hard-boiled detective.");
    let key = d.def.key("en");
    e.adopt(vec![ttv_link::LibraryEntry { key: key.clone(), def_hash: d.def.hash(), lang: "en".into(), voice_id: "voice_old".into(), gain: 1.0, ..Default::default() }]);
    let options = JobOptions { speech_check: true, takes: 3, backup: false, quality: false, improve: false, pick: true, archive: false };
    let req = JobRequest { kind: JobKind::Improve, defs: vec![d.clone()], only: vec![key.clone()], rounds: 1, options, ..request(0) };
    e.submit(req.clone()).unwrap();
    until("the takes", || done(&e)).await;
    // three takes wait, one recommended; nothing new in VCP, the old voice stays
    let choices = e.takes();
    assert_eq!(choices.len(), 1);
    let c = &choices[0];
    assert_eq!((c.key.as_str(), c.takes.len()), (key.as_str(), 3));
    assert_eq!(c.takes.iter().filter(|t| t.recommended).count(), 1);
    assert!(e.take_wav(&key, 2).is_some_and(|w| w.starts_with(b"RIFF")));
    assert_eq!(e.library().entries[0].voice_id, "voice_old");
    assert_eq!(fake.voices.lock().len(), 1);
    // choosing makes it the voice; the old one is deleted in VCP
    let chosen = e.choose_take(&key, 1).await.unwrap();
    assert_ne!(chosen.voice_id, "voice_old");
    assert_eq!(e.library().entries.iter().filter(|x| x.key == key).map(|x| x.voice_id.clone()).collect::<Vec<_>>(), std::slice::from_ref(&chosen.voice_id));
    assert!(!fake.voices.lock().contains_key("voice_old"));
    assert!(e.takes().is_empty() && e.take_wav(&key, 1).is_none());
    // again, but keep the voice: nothing changes
    e.submit(req).unwrap();
    until("the second takes", || e.takes().len() == 1).await;
    assert!(e.discard_takes(&key));
    assert!(e.takes().is_empty());
    assert_eq!(e.library().entries[0].voice_id, chosen.voice_id);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_voice_that_only_fails_the_speech_check_keeps_its_best_take_at_last() {
    let (fake, base) = serve().await;
    let e = engine(&base);
    let mut req = request(0);
    req.defs = vec![def(0, "f", "A breathy WHISPER, barely audible.")];
    e.submit(req).unwrap();
    until("the job", || done(&e)).await;
    let job = &e.status().jobs[0];
    // two attempts misheard, the third keeps the best take instead of failing
    assert_eq!((job.done, job.failed), (1, 0), "{job:?}");
    assert_eq!(fake.designs.load(Ordering::SeqCst), 3);
    let x = &e.library().entries[0];
    assert!(x.wer.is_some_and(|w| w > 0.5), "{x:?}");
    assert_eq!(fake.voices.lock().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn archive_jobs_move_finished_voices_out_of_vcp() {
    let (fake, base) = serve().await;
    // the odd voice's first design comes out odd: it's improved first
    fake.weird.store(5, Ordering::SeqCst);
    let e = engine(&base);
    let mut req = odd_request(true, true);
    req.options.backup = true;
    req.options.archive = true;
    e.submit(req.clone()).unwrap();
    until("compute, improve and archive", || e.status().jobs.len() == 2 && done(&e) && e.status().archived == 2).await;

    // VCP and the library are empty; the archive has both, improved
    assert!(fake.voices.lock().is_empty());
    assert!(e.library().entries.is_empty());
    let arch = e.archived().entries;
    assert!(arch.iter().all(|x| x.backup && x.ref_text.as_deref() == Some(ttv_engine::langs::get("en").unwrap().text)), "{arch:?}");
    assert!(arch.iter().all(|x| x.quality.as_ref().is_some_and(|q| q.score >= FLAG)), "{arch:?}");
    // archived voices count as computed
    assert_eq!(e.submit(req.clone()).unwrap().total, 0);
    // and export without VCP
    e.set_vcp(None);
    let voices: Vec<ttv_link::ExportVoice> = req
        .defs
        .iter()
        .map(|d| ttv_link::ExportVoice { key: d.def.key("en"), preset_id: "test".into(), index: d.index, name: d.def.name.clone(), description: d.def.description.clone() })
        .collect();
    let preset = ttv_link::PackPreset { id: "test".into(), name: "Test".into(), builtin: false, text: "#ttv-tts preset v1\nname: Test\n---\n".into() };
    let path = e.export(ttv_link::ExportRequest { title: "Archived".into(), presets: vec![preset], voices }, "test").await.unwrap();
    assert_eq!(ttv_engine::pack::read_manifest(&path).unwrap().voices.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_voices_can_be_archived_by_hand() {
    let (fake, base) = serve().await;
    let e = engine(&base);
    let mut req = request(2);
    req.options.backup = true;
    e.submit(req).unwrap();
    until("the job", || done(&e)).await;
    let keys: Vec<String> = e.library().entries.iter().map(|x| x.key.clone()).collect();
    assert_eq!(e.archive(&keys[..1]).await.unwrap(), 1);
    assert_eq!((e.library().entries.len(), e.archived().entries.len(), fake.voices.lock().len()), (1, 1, 1));
    assert_eq!(e.status().archived, 1);
}

/// A voice its job saved just before the proxy stopped (restart, update,
/// crash – the job's own count wasn't written yet) counts for the job when it
/// goes on, and gets the job's quality check, instead of being skipped as
/// "already there" and never checked (seen on the VCP PC: Cinematic ru 179/180).
#[tokio::test(flavor = "multi_thread")]
async fn a_voice_saved_just_before_a_restart_counts_for_its_job() {
    let (fake, base) = serve().await;
    let dir = std::env::temp_dir().join(format!("ttv-engine-test-{}", rand::random::<u64>()));
    // no worker yet: the job waits until the "restart"
    let e = Engine::new(dir, Arc::new(|| None));
    e.set_vcp(Some(ttv_vcp::Client::new(&base, None)));
    let mut req = request(3);
    req.options.quality = true;
    let entry = |i: usize, created: i64| ttv_link::LibraryEntry {
        key: req.defs[i].def.key("en"),
        def_hash: req.defs[i].def.hash(),
        lang: "en".into(),
        preset_id: "test".into(),
        index: i as u32,
        voice_id: format!("voice_saved_{i}"),
        gain: 1.0,
        created,
        ..Default::default()
    };
    // voice 2: made long before this job (another job's)
    e.adopt(vec![entry(2, chrono::Utc::now().timestamp() - 3600)]);
    let job = e.submit(req.clone()).unwrap();
    assert_eq!(job.total, 2);
    // voice 1: saved by this job, then the proxy stopped
    e.adopt(vec![entry(1, chrono::Utc::now().timestamp())]);
    tokio::spawn(e.worker());
    until("the job and its check", || done(&e)).await;
    let jobs = e.status().jobs;
    let j = jobs.iter().find(|j| j.id == job.id).unwrap();
    assert_eq!((j.done, j.failed), (2, 0), "voice 0 computed, voice 1 counted: {jobs:?}");
    assert_eq!(fake.voices.lock().len(), 1, "only voice 0 was saved in VCP");
    let lib = e.library().entries;
    let quality = |i: usize| lib.iter().find(|x| x.key == req.defs[i].def.key("en")).unwrap().quality.clone();
    assert!(quality(1).is_some(), "the voice saved before the restart got checked");
    assert!(quality(2).is_none(), "another job's voice stays as it was");
}

fn job(e: &Engine, id: &str) -> ttv_link::JobView {
    e.status().jobs.into_iter().find(|j| j.id == id).unwrap()
}

fn archived(mut req: JobRequest) -> JobRequest {
    req.options.archive = true;
    req.options.backup = true;
    req
}

/// A queue of ready-made packs runs for weeks: what the streamer starts now
/// must not wait behind it.
#[tokio::test(flavor = "multi_thread")]
async fn long_archive_jobs_go_last() {
    let (_fake, base) = serve().await;
    let e = engine(&base);
    e.set_vcp(None);
    let packs = e.submit(archived(request(3))).unwrap().id;
    let mine = e.submit(odd_request(false, false)).unwrap().id;
    e.set_vcp(Some(ttv_vcp::Client::new(&base, None)));
    until("the streamer's job", || job(&e, &mine).state == JobState::Done).await;
    assert_eq!(job(&e, &packs).done, 0, "the archive job ran first");
    until("the archive job", || done(&e)).await;
}

/// Odd voices used to be improved after every compute job of the queue
/// (weeks later): a preset is finished before the next one starts.
#[tokio::test(flavor = "multi_thread")]
async fn odd_voices_are_improved_before_the_next_archive_job() {
    let (fake, base) = serve().await;
    fake.weird.store(5, Ordering::SeqCst);
    let e = engine(&base);
    e.set_vcp(None);
    e.submit(archived(odd_request(true, true))).unwrap();
    let next = e.submit(archived(request(2))).unwrap().id;
    e.set_vcp(Some(ttv_vcp::Client::new(&base, None)));
    until("the improve job", || e.status().jobs.iter().any(|j| j.kind == JobKind::Improve && j.state == JobState::Done)).await;
    assert_eq!(job(&e, &next).done, 0, "the next job ran before the improve job");
    until("the rest", || done(&e)).await;
}

fn plan_preset(id: &str, defs: Vec<JobDef>) -> ttv_link::PlanPreset {
    ttv_link::PlanPreset { id: id.into(), name: id.to_uppercase(), defs }
}

fn plan_options() -> JobOptions {
    JobOptions { speech_check: true, takes: 2, backup: true, quality: true, improve: true, archive: true, ..Default::default() }
}

fn plan_state(e: &Engine) -> ttv_link::PlanState {
    e.status().plan.unwrap().state
}

/// Everything from one request: all voices of all presets in all languages,
/// odd ones designed again, finished ones out of VCP, nobody helping.
#[tokio::test(flavor = "multi_thread")]
async fn the_plan_finishes_everything_by_itself() {
    let (fake, base) = serve().await;
    // both takes of the odd voice
    fake.weird.store(5, Ordering::SeqCst);
    let e = engine(&base);
    let odd = odd_request(false, false).defs;
    let req = ttv_link::PlanRequest {
        presets: vec![plan_preset("a", odd), plan_preset("b", vec![def(7, "f", "A bright and cheerful announcer.")])],
        languages: vec!["en".into(), "de".into()],
        options: plan_options(),
    };
    assert_eq!(e.plan_start(req.clone()).unwrap().voices_total, 6);
    until("the plan", || plan_state(&e) == ttv_link::PlanState::Done).await;

    let st = e.status();
    let plan = st.plan.clone().unwrap();
    assert_eq!((plan.voices_done, plan.voices_odd, plan.gave_up), (6, 0, 0), "{plan:?}");
    assert!(plan.finished.is_some() && plan.eta_secs.is_none(), "{plan:?}");
    assert_eq!(plan.rows.len(), 2);
    assert_eq!(plan.rows[0].cells.iter().map(|c| c.done).collect::<Vec<_>>(), [2, 2]);
    // language by language: both presets, then what needed improving
    let order: Vec<(JobKind, String)> = st.jobs.iter().map(|j| (j.kind, j.languages.join(","))).collect();
    assert_eq!(&order[..3], [(JobKind::Compute, "en".to_string()), (JobKind::Compute, "en".to_string()), (JobKind::Improve, "en".to_string())], "{order:?}");
    assert!(order[3..].iter().all(|(_, l)| l == "de"), "{order:?}");
    // all in the archive, none left in VCP
    until("the archive", || e.status().archived == 6).await;
    assert!(fake.voices.lock().is_empty());
    assert!(e.library().entries.is_empty());
    // a plan that's done stays done, and starting again finds nothing to do
    assert!(!e.working());
    e.plan_start(req).unwrap();
    until("the second plan", || plan_state(&e) == ttv_link::PlanState::Done).await;
    assert_eq!(e.status().jobs.len(), st.jobs.len(), "nothing was computed twice");
}

/// A voice that fails gets more tries, with more takes, until it works.
#[tokio::test(flavor = "multi_thread")]
async fn the_plan_tries_failed_voices_again() {
    let (fake, base) = serve().await;
    fake.clone_broken.store(true, Ordering::SeqCst);
    let e = engine(&base);
    let req = ttv_link::PlanRequest { presets: vec![plan_preset("a", request(1).defs)], languages: vec!["en".into()], options: JobOptions { quality: false, improve: false, ..plan_options() } };
    e.plan_start(req).unwrap();
    until("the first job to fail", || e.status().jobs.iter().any(|j| j.state == JobState::Done && j.failed == 1)).await;
    fake.clone_broken.store(false, Ordering::SeqCst);
    until("the plan", || plan_state(&e) == ttv_link::PlanState::Done).await;
    let plan = e.status().plan.unwrap();
    assert_eq!((plan.voices_done, plan.gave_up), (1, 0), "{plan:?}");
}

/// Some voices just don't work: the plan ends anyway and says so.
#[tokio::test(flavor = "multi_thread")]
async fn the_plan_gives_up_on_a_voice_after_three_jobs() {
    let (fake, base) = serve().await;
    fake.clone_broken.store(true, Ordering::SeqCst);
    let e = engine(&base);
    let req = ttv_link::PlanRequest { presets: vec![plan_preset("a", request(1).defs)], languages: vec!["en".into()], options: JobOptions { quality: false, improve: false, ..plan_options() } };
    e.plan_start(req).unwrap();
    until("the plan", || plan_state(&e) == ttv_link::PlanState::Done).await;
    let st = e.status();
    assert_eq!(st.plan.unwrap().gave_up, 1);
    assert_eq!(st.jobs.len(), 3, "{:?}", st.jobs);
}

/// A queue from before the plan goes; what started stays.
#[tokio::test(flavor = "multi_thread")]
async fn the_plan_replaces_jobs_that_have_not_started() {
    let (_fake, base) = serve().await;
    let e = engine(&base);
    e.set_vcp(None);
    let queued = e.submit(archived(request(2))).unwrap().id;
    let mine = e.submit(odd_request(false, false)).unwrap().id;
    let paused = e.submit(archived(odd_request(false, false))).unwrap().id;
    e.control(&paused, "pause").unwrap();
    e.plan_start(ttv_link::PlanRequest { presets: vec![plan_preset("a", request(1).defs)], languages: vec!["en".into()], options: JobOptions { quality: false, improve: false, ..plan_options() } }).unwrap();
    let ids: Vec<String> = e.status().jobs.into_iter().map(|j| j.id).collect();
    assert!(!ids.contains(&queued) && !ids.contains(&paused), "{ids:?}");
    assert!(ids.contains(&mine), "the streamer's own job stays");
}

/// Voices that left VCP odd come back from their samples and get improved.
#[tokio::test(flavor = "multi_thread")]
async fn the_plan_improves_odd_voices_from_the_archive() {
    let (fake, base) = serve().await;
    fake.weird.store(5, Ordering::SeqCst);
    let e = engine(&base);
    // computed and archived without improving
    e.submit(archived(odd_request(true, false))).unwrap();
    until("the first job", || done(&e) && e.status().archived == 2).await;
    assert!(e.archived().entries.iter().any(|v| v.quality.as_ref().is_some_and(|q| q.score < FLAG)));

    let req = ttv_link::PlanRequest { presets: vec![plan_preset("a", odd_request(false, false).defs)], languages: vec!["en".into()], options: plan_options() };
    e.plan_start(req).unwrap();
    until("the plan", || plan_state(&e) == ttv_link::PlanState::Done).await;
    until("the archive", || e.status().archived == 2).await;
    let st = e.status();
    assert!(st.jobs.iter().any(|j| j.kind == JobKind::Restore), "{:?}", st.jobs);
    assert!(e.archived().entries.iter().all(|v| v.quality.as_ref().is_some_and(|q| q.score >= FLAG)), "{:?}", e.archived().entries);
    assert!(fake.voices.lock().is_empty());
    assert_eq!(st.plan.unwrap().voices_odd, 0);
}

/// VCP going away in the middle of a long batch costs no finished designs.
#[tokio::test(flavor = "multi_thread")]
async fn a_batch_keeps_its_designs_when_vcp_goes_away() {
    let (fake, base) = serve().await;
    // twice: the speech check tries once more by itself
    fake.asr_down.store(2, Ordering::SeqCst);
    let e = engine(&base);
    let mut req = archived(request(3));
    req.options.takes = 2;
    e.submit(req).unwrap();
    until("the job", || done(&e)).await;
    assert_eq!(fake.designs.load(Ordering::SeqCst), 6, "designed again");
    assert_eq!(e.status().jobs[0].done, 3);
}

/// A saved voice for the benchmark to speak with.
async fn with_a_voice(base: &str) -> Engine {
    let e = engine(base);
    e.submit(request(1)).unwrap();
    until("the voice", || done(&e)).await;
    e
}

fn counting_restart(n: Arc<AtomicU32>) -> ttv_engine::Restart {
    Arc::new(move || {
        n.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok("restarted".to_string()) })
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn the_benchmark_measures_everything_and_sets_the_batch_size() {
    let (fake, base) = serve().await;
    let e = with_a_voice(&base).await;
    let restarts = Arc::new(AtomicU32::new(0));
    let vram: ttv_engine::Vram = Arc::new(|| Box::pin(async { Some((5000, 8192)) }));
    let tools = ttv_engine::BenchTools { restart: Some(counting_restart(restarts.clone())), vram: Some(vram) };
    e.bench_start(ttv_link::BenchRequest { level: ttv_link::BenchLevel::Quick, restart: true }, tools.clone()).unwrap();
    assert!(e.bench_start(Default::default(), tools).is_err(), "only one at a time");
    assert!(e.working());
    // a job waits until the benchmark is over
    let waiting = e.submit(odd_request(false, false)).unwrap().id;
    assert_eq!(job(&e, &waiting).state, JobState::Queued);
    until("the benchmark", || e.status().bench.is_some_and(|b| !b.running)).await;
    let b = e.status().bench.unwrap();
    assert_eq!((b.error.as_deref(), b.step), (None, b.steps), "{b:?}");

    let h = e.bench_history();
    let r = &h.reports[0];
    assert_eq!(restarts.load(Ordering::SeqCst), 1);
    assert!(r.restarted && r.restart_secs.is_some());
    let kinds = |v: &[ttv_link::BenchSeries]| v.iter().map(|s| s.kind.clone()).collect::<Vec<_>>();
    assert_eq!(kinds(&r.series), ["design", "speak", "listen"], "{r:?}");
    assert_eq!(r.before.len(), 3);
    assert_eq!(r.switches.len(), 6);
    assert_eq!(r.drift.len(), 3);
    assert!(r.series.iter().all(|s| s.runs > 0 && s.failed == 0 && s.first_secs.is_some() && s.median_secs > 0.0 && s.vram_mb == Some(5000)), "{r:?}");
    assert!(r.series[0].rtf > 0.0 && r.series[1].rtf > 0.0);
    assert_eq!((r.vram_total_mb, r.vram_free_mb), (Some(8192), Some(3192)));
    assert!(r.advice.batch_voices >= 48 && r.advice.voice_secs > 0.0 && r.advice.design_limit > 0.0, "{:?}", r.advice);
    assert!(!ttv_vcp::exclusive() || !fake.low_vram.load(Ordering::SeqCst));
    // now the job runs, in batches of the measured size
    until("the waiting job", || job(&e, &waiting).state == JobState::Done).await;
    assert_eq!(fake.voices.lock().len(), 3);
}

/// Thorough: low VRAM on against off, each from a fresh start; the setting
/// only ever changes right before a restart, and ends up where it's better.
#[tokio::test(flavor = "multi_thread")]
async fn the_thorough_benchmark_compares_low_vram() {
    let (fake, base) = serve().await;
    fake.low_vram.store(false, Ordering::SeqCst);
    let e = with_a_voice(&base).await;
    let restarts = Arc::new(AtomicU32::new(0));
    let tools = ttv_engine::BenchTools { restart: Some(counting_restart(restarts.clone())), vram: None };
    e.bench_start(ttv_link::BenchRequest { level: ttv_link::BenchLevel::Thorough, restart: true }, tools).unwrap();
    until("the benchmark", || e.status().bench.is_some_and(|b| !b.running)).await;
    assert_eq!(e.status().bench.unwrap().error, None);
    let h = e.bench_history();
    let r = &h.reports[0];
    assert_eq!(r.variants.len(), 2, "{r:?}");
    assert!(r.variants[0].low_vram && r.variants[0].error.is_none() && !r.variants[0].series.is_empty(), "VCP left to itself: {:?}", r.variants);
    assert_eq!(r.variants[1].asr_model.as_deref(), Some("standard"), "the lighter model of the same family");
    assert_eq!(restarts.load(Ordering::SeqCst), 4, "before, for each variant, and back");
    assert_eq!(e.status().bench.unwrap().step, e.status().bench.unwrap().steps, "progress reaches its end");
    // the setting stays what it was
    assert!(!fake.low_vram.load(Ordering::SeqCst));
    assert_eq!(r.report_asr(), ("higher_quality", "higher_quality"));
    assert!(!fake.light_asr.load(Ordering::SeqCst));
}

trait AsrResult {
    fn report_asr(&self) -> (&str, &str);
}

impl AsrResult for ttv_link::BenchReport {
    fn report_asr(&self) -> (&str, &str) {
        (self.asr_model.as_deref().unwrap_or(""), self.advice.asr_model.as_deref().unwrap_or(""))
    }
}

/// Crowded video memory, and the lighter model hears as well: it stays.
#[tokio::test(flavor = "multi_thread")]
async fn a_lighter_speech_recognition_stays_when_the_models_do_not_fit_otherwise() {
    let (fake, base) = serve().await;
    let e = with_a_voice(&base).await;
    // voices with lossless samples to listen to
    let mut req = request(3);
    req.options.backup = true;
    req.options.quality = true;
    req.defs.iter_mut().for_each(|d| d.def.gender = "f".into());
    e.submit(req).unwrap();
    until("the voices", || done(&e)).await;
    // full with the big model, room to spare with the light one
    let f = fake.clone();
    let vram: ttv_engine::Vram = Arc::new(move || {
        let light = f.light_asr.load(Ordering::SeqCst);
        Box::pin(async move { Some((if light { 6300 } else { 8192 }, 8192)) })
    });
    let tools = ttv_engine::BenchTools { restart: Some(counting_restart(Default::default())), vram: Some(vram) };
    e.bench_start(ttv_link::BenchRequest { level: ttv_link::BenchLevel::Thorough, restart: true }, tools).unwrap();
    until("the benchmark", || e.status().bench.is_some_and(|b| !b.running)).await;
    assert_eq!(e.status().bench.unwrap().error, None);
    let h = e.bench_history();
    assert_eq!(h.reports[0].report_asr(), ("higher_quality", "standard"), "{:?}", h.reports[0].advice);
    assert!(fake.light_asr.load(Ordering::SeqCst));
}

/// However tight the memory: a model that mishears isn't worth it.
#[tokio::test(flavor = "multi_thread")]
async fn a_speech_recognition_that_mishears_does_not_stay() {
    let (fake, base) = serve().await;
    fake.light_asr_errors.store(1, Ordering::SeqCst);
    let e = with_a_voice(&base).await;
    let mut req = request(3);
    req.options.backup = true;
    req.options.quality = true;
    req.defs.iter_mut().for_each(|d| d.def.gender = "f".into());
    e.submit(req).unwrap();
    until("the voices", || done(&e)).await;
    let f = fake.clone();
    let vram: ttv_engine::Vram = Arc::new(move || {
        let light = f.light_asr.load(Ordering::SeqCst);
        Box::pin(async move { Some((if light { 6300 } else { 8192 }, 8192)) })
    });
    let tools = ttv_engine::BenchTools { restart: Some(counting_restart(Default::default())), vram: Some(vram) };
    e.bench_start(ttv_link::BenchRequest { level: ttv_link::BenchLevel::Thorough, restart: true }, tools).unwrap();
    until("the benchmark", || e.status().bench.is_some_and(|b| !b.running)).await;
    let h = e.bench_history();
    assert_eq!(h.reports[0].report_asr(), ("higher_quality", "higher_quality"), "{:?}", h.reports[0].variants);
    assert!(!fake.light_asr.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_benchmark_can_be_stopped_and_needs_no_restart() {
    let (_fake, base) = serve().await;
    let e = with_a_voice(&base).await;
    e.bench_start(ttv_link::BenchRequest { level: ttv_link::BenchLevel::Thorough, restart: true }, ttv_engine::BenchTools::default()).unwrap();
    e.bench_stop();
    until("the benchmark", || e.status().bench.is_some_and(|b| !b.running)).await;
    assert_eq!(e.status().bench.unwrap().error.as_deref(), Some("stopped"));
    assert!(e.bench_history().reports.is_empty());
    assert!(!e.working());
}

/// A take that shows a defect is replaced while the design model is loaded:
/// no second job, no model switches for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_bad_take_is_replaced_while_designing() {
    let (fake, base) = serve().await;
    fake.weird.store(2, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(odd_request(true, true)).unwrap();
    until("the job", || done(&e)).await;
    // the normal voice once, the odd one three times
    assert_eq!(fake.designs.load(Ordering::SeqCst), 4);
    let st = e.status();
    assert_eq!(st.jobs.len(), 1, "no improve job: {:?}", st.jobs);
    assert_eq!(st.jobs[0].flagged, 0);
}

/// It gives up on a hopeless voice after a few takes.
#[tokio::test(flavor = "multi_thread")]
async fn a_voice_gets_a_limited_number_of_takes() {
    let (fake, base) = serve().await;
    fake.weird.store(-1, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(odd_request(true, false)).unwrap();
    until("the job", || done(&e)).await;
    assert_eq!(fake.designs.load(Ordering::SeqCst), 1 + 5);
    assert_eq!(e.status().jobs[0].flagged, 1);
}

/// Once a take is clean, the other takes need no listening.
#[tokio::test(flavor = "multi_thread")]
async fn the_most_promising_take_is_checked_first() {
    let (fake, base) = serve().await;
    let e = engine(&base);
    let mut req = request(4);
    req.options.takes = 2;
    e.submit(req).unwrap();
    until("the job", || done(&e)).await;
    assert_eq!(fake.designs.load(Ordering::SeqCst), 8);
    assert_eq!(fake.transcriptions.load(Ordering::SeqCst), 4);
    assert_eq!(e.status().jobs[0].done, 4);
}

/// The streamer picks: every take must be checked.
#[tokio::test(flavor = "multi_thread")]
async fn takes_for_the_streamer_are_all_checked() {
    let (fake, base) = serve().await;
    let e = engine(&base);
    let mut req = request(2);
    req.options.takes = 2;
    req.options.pick = true;
    e.submit(req).unwrap();
    until("the job", || done(&e)).await;
    assert_eq!(fake.transcriptions.load(Ordering::SeqCst), 4);
}

/// Listed only for the one test sentence: the voice says what its sample
/// says, and if that's fine nothing is computed again.
#[tokio::test(flavor = "multi_thread")]
async fn a_fluke_with_the_test_sentence_gets_a_second_opinion() {
    let (fake, base) = serve().await;
    fake.flaky_new.store(true, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(odd_request(true, true)).unwrap();
    until("the job", || done(&e)).await;
    let st = e.status();
    assert_eq!(st.jobs.len(), 1, "nothing is computed again: {:?}", st.jobs);
    assert_eq!(st.jobs[0].flagged, 0);
    assert!(e.library().entries.iter().all(|v| v.quality.as_ref().is_some_and(|q| q.score >= FLAG)), "{:?}", e.library().entries);
    // one sentence each, and a second one each
    assert_eq!(fake.clones.load(Ordering::SeqCst), 4);
}

/// A voice that really is off stays listed.
#[tokio::test(flavor = "multi_thread")]
async fn a_real_problem_survives_the_second_opinion() {
    let (fake, base) = serve().await;
    fake.off_pitch.store(true, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(odd_request(true, false)).unwrap();
    until("the job", || done(&e)).await;
    assert_eq!(e.status().jobs[0].flagged, 2);
    assert!(e.library().entries.iter().all(|v| v.quality.as_ref().is_some_and(|q| q.issues.iter().any(|i| i.code == "new_pitch"))), "{:?}", e.library().entries);
}

/// The whole point of the order: a voice model and speech recognition are
/// never in VCP's memory together, whatever the job does.
#[tokio::test(flavor = "multi_thread")]
async fn only_one_model_is_in_memory_at_a_time() {
    let (fake, base) = serve().await;
    fake.weird.store(5, Ordering::SeqCst);
    fake.flaky_new.store(false, Ordering::SeqCst);
    let e = engine(&base);
    let mut req = odd_request(true, true);
    req.options.takes = 2;
    e.submit(req).unwrap();
    until("compute and improve", || e.status().jobs.len() == 2 && done(&e)).await;
    assert_eq!(fake.collisions.load(Ordering::SeqCst), 0);
    assert!(fake.designs.load(Ordering::SeqCst) > 4 && fake.transcriptions.load(Ordering::SeqCst) >= 4);
    // and the stored setting is what it was
    assert!(!fake.low_vram.load(Ordering::SeqCst));
    assert!(!fake.asr_loaded.load(Ordering::SeqCst), "nothing stays behind");
}

/// A proxy that stopped in the middle of a phase left its model behind.
#[tokio::test(flavor = "multi_thread")]
async fn what_was_left_in_memory_is_cleared_first() {
    let (fake, base) = serve().await;
    fake.asr_loaded.store(true, Ordering::SeqCst);
    fake.voice_loaded.store(true, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(request(2)).unwrap();
    until("the job", || done(&e)).await;
    assert_eq!(fake.collisions.load(Ordering::SeqCst), 0);
}

/// A second opinion and a check job keep the order too.
#[tokio::test(flavor = "multi_thread")]
async fn second_opinions_and_checks_keep_the_order() {
    let (fake, base) = serve().await;
    fake.flaky_new.store(true, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(odd_request(true, false)).unwrap();
    until("the job", || done(&e)).await;
    assert_eq!(fake.collisions.load(Ordering::SeqCst), 0);
    assert!(!fake.asr_loaded.load(Ordering::SeqCst));
}

/// With low VRAM on (VCP's usual setting): off for speech recognition, so it
/// stays loaded between clips, and on again afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn low_vram_is_only_off_while_speech_recognition_works() {
    let (fake, base) = serve().await;
    fake.low_vram.store(true, Ordering::SeqCst);
    let e = engine(&base);
    let mut req = odd_request(true, false);
    req.options.takes = 2;
    e.submit(req).unwrap();
    until("the job", || done(&e)).await;
    assert!(fake.low_vram.load(Ordering::SeqCst), "back on");
    assert_eq!(fake.collisions.load(Ordering::SeqCst), 0);
    assert!(!fake.asr_loaded.load(Ordering::SeqCst));
    // clips were heard with it off: one reload per phase, not one per clip
    assert!(fake.transcriptions.load(Ordering::SeqCst) >= 4);
}

/// A proxy that stopped during speech recognition left the setting off.
#[tokio::test(flavor = "multi_thread")]
async fn a_crash_during_speech_recognition_leaves_nothing_behind() {
    let (fake, base) = serve().await;
    let dir = std::env::temp_dir().join(format!("ttv-engine-crash-{}", rand::random::<u64>()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("stats.json"), r#"{"low_vram_was":true}"#).unwrap();
    fake.low_vram.store(false, Ordering::SeqCst);
    fake.asr_loaded.store(true, Ordering::SeqCst);
    let e = Engine::new(dir, Arc::new(|| None));
    e.set_vcp(Some(ttv_vcp::Client::new(&base, None)));
    tokio::spawn(e.worker());
    e.submit(request(1)).unwrap();
    until("the job", || done(&e)).await;
    assert!(fake.low_vram.load(Ordering::SeqCst), "the setting from before the crash");
    assert_eq!(fake.collisions.load(Ordering::SeqCst), 0);
}

/// A paused job doesn't leave speech recognition in memory.
#[tokio::test(flavor = "multi_thread")]
async fn a_batch_that_stops_while_listening_releases_speech_recognition() {
    let (fake, base) = serve().await;
    fake.asr_down.store(2, Ordering::SeqCst);
    let e = engine(&base);
    e.submit(request(2)).unwrap();
    until("the job", || done(&e)).await;
    assert!(!fake.asr_loaded.load(Ordering::SeqCst));
    assert_eq!(fake.collisions.load(Ordering::SeqCst), 0);
}
