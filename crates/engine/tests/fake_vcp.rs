//! The engine against a fake Voice Creator Pro: a whole job, broken speech
//! recognition, a voice VCP can't speak with, and checking the library.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use parking_lot::Mutex;
use serde_json::{Value, json};
use ttv_engine::Engine;
use ttv_link::{JobDef, JobOptions, JobRequest, JobState, VoiceDef};

#[derive(Default)]
struct Fake {
    /// id -> name
    voices: Mutex<BTreeMap<String, String>>,
    next: AtomicU32,
    asr_broken: AtomicBool,
    clone_broken: AtomicBool,
    clones: AtomicU32,
    designs: AtomicU32,
}

type S = State<Arc<Fake>>;

/// Mono PCM16 sine at −20 dBFS RMS (gain 1.0).
fn wav(secs: f32) -> String {
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
        let s = (i as f32 * 150.0 * std::f32::consts::TAU / rate as f32).sin() * 0.1414;
        b.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(b)
}

// Handlers read the body like VCP does (an unread upload resets the connection).
async fn design(State(f): S, Json(_b): Json<Value>) -> Json<Value> {
    f.designs.fetch_add(1, Ordering::SeqCst);
    Json(json!({ "success": true, "status": "completed", "audio_base64": wav(7.0) }))
}

async fn transcribe(State(f): S, Json(_b): Json<Value>) -> Json<Value> {
    if f.asr_broken.load(Ordering::SeqCst) {
        return Json(json!({ "success": false, "text": null, "error": "ASR model failed to load" }));
    }
    Json(json!({ "success": true, "text": ttv_engine::langs::get("en").unwrap().text }))
}

async fn save(State(f): S, Json(b): Json<Value>) -> Json<Value> {
    let id = format!("voice_{:08x}", f.next.fetch_add(1, Ordering::SeqCst));
    let name = b["name"].as_str().unwrap_or_default().to_string();
    f.voices.lock().insert(id.clone(), name.clone());
    Json(json!({ "success": true, "voice": { "id": id, "name": name }, "message": "saved" }))
}

async fn clone(State(f): S, Json(_b): Json<Value>) -> Json<Value> {
    f.clones.fetch_add(1, Ordering::SeqCst);
    if f.clone_broken.load(Ordering::SeqCst) {
        return Json(json!({ "success": false, "status": "failed", "error_detail": "Generation failed" }));
    }
    Json(json!({ "success": true, "status": "completed", "audio_base64": wav(2.5) }))
}

async fn list(State(f): S) -> Json<Value> {
    let voices: Vec<Value> = f.voices.lock().iter().map(|(id, name)| json!({ "id": id, "name": name })).collect();
    Json(json!({ "voices": voices }))
}

async fn one(Path(id): Path<String>) -> Json<Value> {
    // Every fake reference sample is at −20 dBFS.
    Json(json!({ "id": id, "audio_base64": wav(7.0) }))
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

fn request(voices: u32) -> JobRequest {
    JobRequest {
        preset_id: "test".into(),
        preset_name: "Test".into(),
        defs: (0..voices)
            .map(|i| JobDef { index: i, def: VoiceDef { name: format!("Voice {i}"), gender: "f".into(), age: 30, description: format!("A clear, friendly test voice number {i}."), fx: None } })
            .collect(),
        languages: vec!["en".into()],
        options: JobOptions { speech_check: true, takes: 1, backup: false },
    }
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(30), "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
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
    fake.voices.lock().insert("voice_old".into(), "TTS-de 01 Anna".into());
    let adopted = ttv_link::LibraryEntry { key: "vabc:de".into(), lang: "de".into(), voice_id: "voice_old".into(), gain: 1.0, ..Default::default() };
    assert_eq!(e.adopt(vec![adopted]), 1);

    let r = e.verify().await.unwrap();
    assert_eq!((r.removed, r.measured), (0, 1));
    let x = &e.library().entries[0];
    assert!(x.level.is_some_and(|db| (db + 20.0).abs() < 0.5), "{x:?}");
    assert!((x.gain - 1.0).abs() < 0.06);
    assert_eq!(e.verify().await.unwrap().measured, 0);
}
