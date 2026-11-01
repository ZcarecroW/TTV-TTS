//! Turns text into sound: splits it into sentence-sized pieces, has VCP
//! generate each piece (sync clone with a saved voice) and queues the audio.
//! Piece n+1 is generated while piece n plays, so playback is gapless as long
//! as VCP is faster than real time (measured ~0.5× on an RTX 3070, while VCP's
//! own streaming was 3–4× slower than real time on the same PC).

use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use ttv_vcp::{GenerationResult, wav};

use crate::audio::Audio;

/// One piece of text to generate.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece(pub String);

/// Splits text into pieces for generation. The first piece is kept short so
/// the first sound comes quickly; later pieces may be longer.
pub fn split(text: &str) -> Vec<Piece> {
    const MIN: usize = 18;
    const FIRST_MAX: usize = 110;
    const MAX: usize = 220;
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Vec::new();
    }
    // 1. sentences
    let mut sentences: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        cur.push(c);
        let end = matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？');
        let next_space = chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        let cjk = matches!(c, '。' | '！' | '？');
        if end && (next_space || cjk) && !chars.get(i + 1).is_some_and(|n| matches!(n, '.' | '!' | '?')) {
            sentences.push(cur.trim().to_string());
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        sentences.push(cur.trim().to_string());
    }
    // 2. merge very short sentences into the following one
    let mut merged: Vec<String> = Vec::new();
    for s in sentences {
        match merged.last_mut() {
            Some(last) if last.chars().count() < MIN => {
                last.push(' ');
                last.push_str(&s);
            }
            _ => merged.push(s),
        }
    }
    // 3. cut long pieces at a comma / semicolon / space
    let mut out: Vec<Piece> = Vec::new();
    for s in merged {
        let mut rest = s;
        loop {
            let max = if out.is_empty() { FIRST_MAX } else { MAX };
            if rest.chars().count() <= max {
                out.push(Piece(rest));
                break;
            }
            let cut = cut_point(&rest, max);
            let (a, b) = rest.split_at(cut);
            out.push(Piece(a.trim().to_string()));
            rest = b.trim().to_string();
            if rest.is_empty() {
                break;
            }
        }
    }
    out.retain(|p| p.0.chars().any(char::is_alphanumeric));
    out
}

/// Byte index to cut `s` at, at most `max` characters in: the last comma or
/// semicolon in the second half, else the last space, else hard.
fn cut_point(s: &str, max: usize) -> usize {
    let limit = s.char_indices().nth(max).map(|(i, _)| i).unwrap_or(s.len());
    let head = &s[..limit];
    let half = limit / 2;
    if let Some(i) = head.rfind([',', ';', ':']).filter(|&i| i >= half) {
        return i + 1;
    }
    if let Some(i) = head.rfind(' ').filter(|&i| i >= half / 2) {
        return i + 1;
    }
    limit
}

/// What to say and how.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utterance {
    pub text: String,
    /// VCP voice id.
    pub voice: String,
    /// VCP language name ("German", "English", … or "Auto").
    pub language: String,
    pub emotion: Option<String>,
    pub intensity: Option<String>,
    /// Loudness correction for this voice (1.0 = none).
    #[serde(default = "one")]
    pub gain: f32,
    /// Voice effect ("radio", …).
    #[serde(default)]
    pub fx: Option<String>,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SayReport {
    pub pieces: usize,
    /// Time until the first sound started (ms).
    pub first_audio_ms: u64,
    /// Total time until everything was played (ms).
    pub total_ms: u64,
    /// Seconds of audio generated.
    pub audio_secs: f32,
    /// Generation time / audio duration (lower is faster; < 1 = faster than real time).
    pub rtf: f32,
}

/// When this app last asked VCP for speech (unix ms; 0 = never). The local
/// engine (VCP on this PC) waits until chat has been quiet for a moment.
static LAST_VCP_USE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn touch() {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    LAST_VCP_USE.store(now, std::sync::atomic::Ordering::Relaxed);
}

/// For the engine: when speech was last generated.
pub fn last_vcp_use() -> Option<Instant> {
    let ms = LAST_VCP_USE.load(std::sync::atomic::Ordering::Relaxed);
    if ms == 0 {
        return None;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(ms);
    Instant::now().checked_sub(Duration::from_millis(now.saturating_sub(ms)))
}

/// Generates one piece with VCP (sync clone of a saved voice).
pub async fn generate(vcp: &ttv_vcp::Client, u: &Utterance, text: &str) -> Result<(Vec<f32>, u32), String> {
    touch();
    let t0 = Instant::now();
    let mut body = serde_json::json!({ "voice_id": u.voice, "target_text": text, "language": u.language });
    if let Some(e) = u.emotion.as_deref().filter(|e| *e != "neutral") {
        body["emotion"] = e.into();
        body["intensity"] = u.intensity.clone().unwrap_or_else(|| "normal".into()).into();
    }
    let r: Result<GenerationResult, String> = vcp
        .post("/api/v1/generation/clone?sync=true", &body, Duration::from_secs(300))
        .await
        .map_err(|e| e.to_string());
    // Long generations: the quiet gap counts from the end.
    touch();
    let r = r?;
    if !r.success {
        return Err(format!("VCP: {}", r.error_detail.or(r.error_code).unwrap_or(r.status)));
    }
    let b64 = r.audio_base64.ok_or("VCP returned no audio")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()).map_err(|e| e.to_string())?;
    let mut dec = wav::StreamDecoder::new();
    let mut samples = Vec::new();
    dec.push(&bytes, &mut samples).map_err(|e| e.to_string())?;
    let f = dec.format().ok_or("VCP returned an empty WAV")?;
    // Mono for playback.
    if f.channels > 1 {
        let ch = f.channels as usize;
        samples = samples.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect();
    }
    ttv_vcp::perf::record(t0.elapsed().as_secs_f32(), samples.len() as f32 / f.sample_rate.max(1) as f32);
    Ok((samples, f.sample_rate))
}

/// Says `u` now: generates piece by piece and plays as soon as the first one
/// is ready. Returns when the last piece finished playing (or was skipped).
pub async fn say(vcp: ttv_vcp::Client, audio: Arc<Audio>, u: Utterance) -> Result<SayReport, String> {
    let pieces = split(&u.text);
    if pieces.is_empty() {
        return Ok(SayReport::default());
    }
    let t0 = Instant::now();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<(Vec<f32>, u32, Duration), String>>(2);
    let n = pieces.len();
    let gen_u = u.clone();
    let producer = tokio::spawn(async move {
        for p in pieces {
            let t = Instant::now();
            let r = generate(&vcp, &gen_u, &p.0).await.map(|(s, rate)| (s, rate, t.elapsed()));
            let stop = r.is_err();
            if tx.send(r).await.is_err() || stop {
                break;
            }
        }
    });
    let mut report = SayReport { pieces: n, ..Default::default() };
    let mut gen_time = Duration::ZERO;
    let mut last_id = 0;
    let fx = u.fx.as_deref().and_then(crate::fx::Fx::parse);
    let mut chain: Option<crate::fx::Chain> = None;
    while let Some(r) = rx.recv().await {
        let (samples, rate, took) = r?;
        gen_time += took;
        report.audio_secs += samples.len() as f32 / rate.max(1) as f32;
        last_id = match fx {
            Some(f) => {
                let c = chain.get_or_insert_with(|| crate::fx::Chain::new(f, rate));
                audio.play(c.process(samples, u.gain), rate, 1.0)
            }
            None => audio.play(samples, rate, u.gain),
        };
        if report.first_audio_ms == 0 {
            report.first_audio_ms = t0.elapsed().as_millis() as u64;
        }
    }
    let _ = producer.await;
    if let Some(c) = chain {
        let rate = c.rate;
        let tail = c.finish();
        if !tail.is_empty() {
            last_id = audio.play(tail, rate, 1.0);
        }
    }
    if last_id != 0 {
        audio.wait(last_id).await;
    }
    report.total_ms = t0.elapsed().as_millis() as u64;
    report.rtf = if report.audio_secs > 0.0 { gen_time.as_secs_f32() / report.audio_secs } else { 0.0 };
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(v: Vec<Piece>) -> Vec<String> {
        v.into_iter().map(|p| p.0).collect()
    }

    #[test]
    fn splits_sentences_and_merges_short_ones() {
        assert_eq!(texts(split("Hi! How are you doing today? I am fine.")), vec!["Hi! How are you doing today?", "I am fine."]);
        assert_eq!(texts(split("gg")), vec!["gg"]);
        assert_eq!(texts(split("Wait... what?! No way.")), vec!["Wait... what?! No way."]);
        assert!(split("   ").is_empty());
        assert!(split("!!! ...").is_empty());
    }

    #[test]
    fn first_piece_is_short_long_ones_are_cut_at_commas() {
        let long = "This is a rather long first sentence, which keeps going with more and more words, because chat messages can be long, and it never seems to end at all.";
        let p = texts(split(long));
        assert!(p.len() >= 2);
        assert!(p[0].chars().count() <= 110, "{}", p[0]);
        assert!(p[0].ends_with(','), "{}", p[0]);
        assert_eq!(p.join(" ").replace("  ", " "), long);
    }

    #[test]
    fn cjk_sentences() {
        assert_eq!(split("你好。今天怎么样？很好！").len(), 1, "short CJK sentences are merged");
    }
}

/// Live test against a real proxy (plays sound on the default device):
///   TTV_LIVE_PROXY=192.168.178.25:8110 TTV_LIVE_TOKEN=<key or token> TTV_LIVE_VOICE=<voice id>
///   cargo test -p ttv-tts live_ -- --ignored --nocapture
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_discover_probe_and_say() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let address = std::env::var("TTV_LIVE_PROXY").expect("TTV_LIVE_PROXY");
        let token = std::env::var("TTV_LIVE_TOKEN").expect("TTV_LIVE_TOKEN");
        let voice = std::env::var("TTV_LIVE_VOICE").expect("TTV_LIVE_VOICE");

        let t = Instant::now();
        let found = crate::host::discover(Duration::from_millis(1200), &[]).await;
        println!("discovery: {} proxy/proxies in {} ms: {:?}", found.len(), t.elapsed().as_millis(), found.iter().map(|f| (&f.info.name, &f.address)).collect::<Vec<_>>());

        let probe = crate::host::probe(&address).await.expect("probe");
        println!("probe: {} v{} fingerprint {}", probe.info.name, probe.info.version, probe.fingerprint_short);

        let host = crate::config::HostConfig::Proxy { id: probe.info.id.clone(), name: probe.info.name.clone(), address: probe.address.clone(), fingerprint: probe.fingerprint.clone(), mac: None, guest: false };
        let vcp = crate::host::client(&host, |_| Some(token.clone())).expect("client");
        let voices = vcp.voices().await.expect("voices through the pinned connection");
        println!("voices via proxy: {}", voices.len());

        // A wrong pin must be refused.
        let wrong_pin = crate::config::HostConfig::Proxy { id: probe.info.id.clone(), name: String::new(), address: probe.address.clone(), fingerprint: "00".repeat(32), mac: None, guest: false };
        let bad = crate::host::client(&wrong_pin, |_| Some(token.clone())).unwrap();
        assert!(bad.voices().await.is_err(), "a wrong certificate pin must fail");

        let audio = Arc::new(Audio::start(None, 0.25));
        let u = Utterance { text: "Hello from TTV-TTS. This sentence was generated on the other PC.".into(), voice, language: "English".into(), emotion: None, intensity: None, gain: 1.0, fx: None };
        let r = say(vcp, audio, u).await.expect("say");
        println!("say: {r:?}");
        assert!(r.audio_secs > 1.0);
    }
}
