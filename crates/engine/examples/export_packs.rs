//! Builds voice packs from a copy of an engine's folder, without VCP:
//!   cargo run --release -p ttv-engine --example export_packs -- <engine dir> <requests.json> <out dir> <app version>
//! requests.json: [{ "file": "name.ttvpack", "request": <ExportRequest> }, …]
//! Used by scripts/build-voice-packs.mjs (TTV_FROM).
use std::sync::Arc;

use serde::Deserialize;

#[derive(Deserialize)]
struct Wanted {
    file: String,
    request: ttv_link::ExportRequest,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, dir, requests, out, app] = &args[..] else { panic!("usage: export_packs <engine dir> <requests.json> <out dir> <app version>") };
    let wanted: Vec<Wanted> = serde_json::from_slice(&std::fs::read(requests).expect("requests.json")).expect("requests.json");
    let engine = ttv_engine::Engine::new(dir.into(), Arc::new(|| None));
    let t = std::time::Instant::now();
    for (i, w) in wanted.iter().enumerate() {
        let path = engine.export(w.request.clone(), app).await.unwrap_or_else(|e| panic!("{}: {e}", w.file));
        let target = std::path::Path::new(out).join(&w.file);
        std::fs::rename(&path, &target).or_else(|_| std::fs::copy(&path, &target).map(|_| ())).expect("moving the pack");
        if (i + 1) % 50 == 0 {
            println!("{} of {} packs, {:.0} s", i + 1, wanted.len(), t.elapsed().as_secs_f32());
        }
    }
    println!("{} packs in {:.0} s", wanted.len(), t.elapsed().as_secs_f32());
}
