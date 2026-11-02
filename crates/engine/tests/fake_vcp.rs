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

use axum::extract::{Path, State};
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
}

type S = State<Arc<Fake>>;

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
async fn design(State(f): S, Json(b): Json<Value>) -> Json<Value> {
    f.designs.fetch_add(1, Ordering::SeqCst);
    let instruct = b["instruct_text"].as_str().unwrap_or_default();
    let weird = instruct.contains("WEIRD") && f.weird.try_update(Ordering::SeqCst, Ordering::SeqCst, |w| if w == 0 { None } else { Some(if w > 0 { w - 1 } else { w }) }).is_ok();
    let hz = if weird {
        400.0
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

async fn transcribe(State(f): S, Json(b): Json<Value>) -> Json<Value> {
    if f.asr_broken.load(Ordering::SeqCst) {
        return Json(json!({ "success": false, "text": null, "error": "ASR model failed to load" }));
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(b["audio_base64"].as_str().unwrap_or_default()).unwrap_or_default();
    let secs = bytes.len().saturating_sub(44) as f32 / 48_000.0;
    let l = lang_of(&b);
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
    if f.clone_broken.load(Ordering::SeqCst) {
        return Json(json!({ "success": false, "status": "failed", "error_detail": "Generation failed" }));
    }
    let hz = f.voices.lock().get(b["voice_id"].as_str().unwrap_or_default()).map_or(150.0, |v| v.hz);
    Json(json!({ "success": true, "status": "completed", "audio_base64": wav(2.5, hz) }))
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
        options: JobOptions { speech_check: true, takes: 1, backup: false, quality, improve, pick: false },
        ..request(0)
    }
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(30), "timed out waiting for {what}");
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
    // The worker backs off (10 s) instead of designing the batch again at once.
    let designs = fake.designs.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(fake.designs.load(Ordering::SeqCst), designs);
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
    fake.weird.store(1, Ordering::SeqCst);
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
    let options = JobOptions { speech_check: true, takes: 3, backup: false, quality: false, improve: false, pick: true };
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
