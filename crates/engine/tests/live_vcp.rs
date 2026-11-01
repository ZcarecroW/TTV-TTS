//! Voice packs against a real Voice Creator Pro (ignored; needs the VCP PC).
//!   TTV_LIVE_VCP=http://192.168.178.25:8101 TTV_LIVE_KEY=<key> TTV_LIVE_LIB=.work/diag/lib-q.json \
//!   cargo test -p ttv-engine --test live_vcp -- --ignored --nocapture
//! Exports two computed voices of the given library (read-only), imports them
//! as new VCP voices, checks them, and deletes exactly those again.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ttv_engine::Engine;
use ttv_link::{ExportRequest, ExportVoice, JobKind, JobRequest, JobState, LibrarySnapshot, PackPreset};

fn engine(vcp: &str, key: &str) -> Engine {
    let dir = std::env::temp_dir().join(format!("ttv-live-{}", rand::random::<u64>()));
    let e = Engine::new(dir, Arc::new(|| None));
    e.set_vcp(Some(ttv_vcp::Client::new(vcp, Some(key.to_string()))));
    tokio::spawn(e.worker());
    e
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn live_pack_roundtrip() {
    let (Ok(vcp), Ok(key), Ok(lib)) = (std::env::var("TTV_LIVE_VCP"), std::env::var("TTV_LIVE_KEY"), std::env::var("TTV_LIVE_LIB")) else {
        panic!("set TTV_LIVE_VCP, TTV_LIVE_KEY, TTV_LIVE_LIB");
    };
    let snap: LibrarySnapshot = serde_json::from_slice(&std::fs::read(lib).unwrap()).unwrap();
    // two Cinematic voices, one per language
    let picked: Vec<_> = ["de", "en"].iter().filter_map(|l| snap.entries.iter().find(|e| e.preset_id == "cinematic" && &e.lang == l)).cloned().collect();
    assert_eq!(picked.len(), 2);

    let a = engine(&vcp, &key);
    a.adopt(picked.clone());
    let t = Instant::now();
    let voices = picked.iter().map(|e| ExportVoice { key: e.key.clone(), preset_id: e.preset_id.clone(), index: e.index, name: e.name.clone(), description: "live test".into() }).collect();
    let preset = PackPreset { id: "cinematic".into(), name: "Cinematic".into(), builtin: true, text: String::new() };
    let path = a.export(ExportRequest { title: "live".into(), presets: vec![preset], voices }, "test").await.unwrap();
    let size = std::fs::metadata(&path).unwrap().len();
    println!("export: 2 voices, {size} bytes, {:.1} s", t.elapsed().as_secs_f32());
    let m = ttv_engine::pack::read_manifest(&path).unwrap();
    assert!(m.voices.iter().all(|v| !v.ref_text.is_empty()), "{:?}", m.voices);

    let b = engine(&vcp, &key);
    let t = Instant::now();
    let job = b.submit(JobRequest { kind: JobKind::Import, pack: path.to_string_lossy().into(), ..Default::default() }).unwrap();
    assert_eq!(job.total, 2);
    let deadline = Instant::now() + Duration::from_secs(300);
    while b.status().jobs[0].state != JobState::Done {
        assert!(Instant::now() < deadline, "import took too long: {:?}", b.status().jobs[0]);
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let job = &b.status().jobs[0];
    println!("import: {} done, {} failed, {:.1} s, error {:?}", job.done, job.failed, t.elapsed().as_secs_f32(), job.error);
    let imported = b.library().entries;
    assert_eq!(imported.len(), 2);

    // the new VCP voices exist and differ from the originals
    let client = ttv_vcp::Client::new(&vcp, Some(key.clone()));
    let all = client.voices().await.unwrap();
    for e in &imported {
        let v = all.iter().find(|v| v.id == e.voice_id).expect("imported voice in VCP");
        assert!(picked.iter().all(|p| p.voice_id != v.id));
        println!("  {} → {} ({:.1} s, gain {:.2})", e.key, v.name, v.duration_seconds, e.gain);
    }
    // clean up: only the two new voices
    let keys: Vec<String> = imported.iter().map(|e| e.key.clone()).collect();
    assert_eq!(b.delete(&keys).await.unwrap(), 2);
    let after = client.voices().await.unwrap();
    assert!(imported.iter().all(|e| after.iter().all(|v| v.id != e.voice_id)), "deleted again");
    assert!(picked.iter().all(|p| after.iter().any(|v| v.id == p.voice_id)), "originals untouched");
}
