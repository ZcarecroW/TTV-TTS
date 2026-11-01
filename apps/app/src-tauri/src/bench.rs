//! Which way VCP speaks faster on this setup: whole sentences (sync, the
//! next generated while the current plays) or VCP's streaming sessions.
//! "Automatic" measures both once per voice host and model and keeps the
//! faster one (on the target PC streaming was 7–8× slower: real-time factor
//! 3.3–4.1 against 0.51). Also: warming VCP up, so the first chat message
//! doesn't wait for the model to load.

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::config::{GenerationMode, HostConfig};
use crate::speech::{self, Utterance};
use crate::state::AppState;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Measured {
    /// Voice host and model the numbers belong to.
    pub host: String,
    pub model: String,
    /// Unix seconds.
    pub at: i64,
    pub sentences_first_ms: u64,
    pub sentences_rtf: f32,
    pub streaming_first_ms: Option<u64>,
    pub streaming_rtf: Option<f32>,
    /// The faster way: "sentences" or "streaming".
    pub pick: String,
}

fn host_key(state: &AppState) -> String {
    match &state.cfg.read().host {
        HostConfig::Proxy { id, .. } => format!("proxy:{id}"),
        HostConfig::Direct { url, .. } => format!("direct:{url}"),
        HostConfig::None => String::new(),
    }
}

fn path(state: &AppState) -> std::path::PathBuf {
    state.data_dir.join("generation-benchmark.json")
}

/// The last measurement for the current host, if any.
pub fn current(state: &AppState) -> Option<Measured> {
    let m: Measured = crate::storage::read_json(&path(state))?;
    (m.host == host_key(state)).then_some(m)
}

/// How to generate now.
pub fn mode(state: &AppState) -> GenerationMode {
    match state.cfg.read().generation {
        GenerationMode::Auto => match current(state) {
            Some(m) if m.pick == "streaming" => GenerationMode::Streaming,
            _ => GenerationMode::Sentences,
        },
        m => m,
    }
}

/// A voice to measure with: the VCP default voice, else the first one.
async fn voice(vcp: &ttv_vcp::Client) -> Result<(String, String), String> {
    let voices = vcp.voices().await.map_err(|e| e.to_string())?;
    let default = vcp.agent_settings().await.ok().and_then(|a| a.default_voice);
    let v = default
        .and_then(|d| voices.iter().find(|v| v.id == d || v.name == d).cloned())
        .or_else(|| voices.iter().find(|v| v.has_voice_clone_prompt).cloned())
        .ok_or("VCP has no saved voice to measure with")?;
    let lang = if v.name.starts_with("TTS-de") { "German" } else { "English" };
    Ok((v.id, lang.into()))
}

/// Measures both ways with the same sentence (~4 s of speech).
pub async fn run(state: &Arc<AppState>) -> Result<Measured, String> {
    let vcp = state.vcp().ok_or("No voice host set up yet.")?;
    let (voice, language) = voice(&vcp).await?;
    let text = if language == "German" { "Das ist ein kurzer Test, wie schnell diese Stimme hier spricht." } else { "This is a short test of how fast this voice speaks right here." };
    let u = Utterance { text: text.into(), voice, language, emotion: None, intensity: None, gain: 1.0, fx: None };
    // the first call may load the model: not measured
    speech::generate(&vcp, &u, text).await?;
    let t0 = Instant::now();
    let (s, rate) = speech::generate(&vcp, &u, text).await?;
    let sync_ms = t0.elapsed().as_millis() as u64;
    let audio = s.len() as f32 / rate.max(1) as f32;
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let t1 = Instant::now();
    let streamed = {
        let u = u.clone();
        let vcp = vcp.clone();
        tokio::spawn(async move { speech::generate_streaming(&vcp, &u, text, &tx).await })
    };
    let mut first = None;
    let mut got = 0f32;
    while let Some(c) = rx.recv().await {
        if let Ok((s, rate)) = c {
            first.get_or_insert(t1.elapsed().as_millis() as u64);
            got += s.len() as f32 / rate.max(1) as f32;
        }
    }
    let stream_ok = streamed.await.map_err(|e| e.to_string())?.is_ok() && got > 0.5;
    let stream_ms = t1.elapsed().as_millis() as u64;
    let model = state.host.read().vcp.as_ref().and_then(|v| v.model_name.clone()).unwrap_or_default();
    let (streaming_first_ms, streaming_rtf) = if stream_ok { (first, Some(stream_ms as f32 / 1000.0 / got)) } else { (None, None) };
    let sentences_rtf = sync_ms as f32 / 1000.0 / audio.max(0.1);
    // streaming only if it keeps up with playback and starts clearly sooner
    let pick = match (streaming_first_ms, streaming_rtf) {
        (Some(f), Some(r)) if r < 0.9 && (f as f32) < sync_ms as f32 * 0.8 => "streaming",
        _ => "sentences",
    };
    let m = Measured {
        host: host_key(state),
        model,
        at: chrono::Utc::now().timestamp(),
        sentences_first_ms: sync_ms,
        sentences_rtf,
        streaming_first_ms,
        streaming_rtf,
        pick: pick.into(),
    };
    log::info!("generation benchmark: sentences {} ms (rtf {:.2}), streaming {:?} ms (rtf {:?}) -> {pick}", m.sentences_first_ms, m.sentences_rtf, m.streaming_first_ms, m.streaming_rtf);
    let _ = crate::storage::write_json(&path(state), &m);
    Ok(m)
}

/// One short sentence so VCP loads its speaking model now, not with the
/// first chat message (nothing is played).
pub async fn warm_up(state: &Arc<AppState>) {
    let Some(vcp) = state.vcp() else { return };
    let Ok((voice, language)) = voice(&vcp).await else { return };
    let u = Utterance { text: "Hello.".into(), voice, language, emotion: None, intensity: None, gain: 1.0, fx: None };
    let t0 = Instant::now();
    match speech::generate(&vcp, &u, "Hello.").await {
        Ok(_) => log::info!("voice host warmed up in {} ms", t0.elapsed().as_millis()),
        Err(e) => log::info!("warming up: {e}"),
    }
}
