//! Turns text into sound: splits it into sentence-sized pieces, has VCP
//! generate each piece (sync clone with a saved voice) and queues the audio.
//! Piece n+1 is generated while piece n plays, so playback is gapless as long
//! as VCP is faster than real time (measured ~0.5× on an RTX 3070, while VCP's
//! own streaming was 3–4× slower than real time on the same PC).

use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use ttv_vcp::{GenerationResult, wav};


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

/// Development builds: make every generation fail (to test the fallback voice).
pub static DEBUG_FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Generates one piece with VCP (sync clone of a saved voice).
pub async fn generate(vcp: &ttv_vcp::Client, u: &Utterance, text: &str) -> Result<(Vec<f32>, u32), String> {
    if cfg!(debug_assertions) && DEBUG_FAIL.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("VCP failed (simulated)".into());
    }
    touch();
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
    Ok((samples, f.sample_rate))
}

/// A streaming generation that failed; `sent` = some of its audio was
/// already handed to the player (so it can't be retried another way).
pub struct StreamError {
    pub message: String,
    pub sent: bool,
}

/// Generates one piece with VCP's streaming session: the progressive WAV is
/// decoded while it arrives and passed on in blocks of ~0.3 s. Returns the
/// seconds of audio.
pub async fn generate_streaming(vcp: &ttv_vcp::Client, u: &Utterance, text: &str, tx: &tokio::sync::mpsc::Sender<Result<(Vec<f32>, u32), String>>) -> Result<f32, StreamError> {
    use futures_util::StreamExt;
    let fail = |message: String, sent: bool| StreamError { message, sent };
    touch();
    let mut body = serde_json::json!({ "voice_id": u.voice, "target_text": text, "language": u.language });
    if let Some(e) = u.emotion.as_deref().filter(|e| *e != "neutral") {
        body["emotion"] = e.into();
        body["intensity"] = u.intensity.clone().unwrap_or_else(|| "normal".into()).into();
    }
    let start: serde_json::Value = vcp.post("/api/v1/generation/clone/stream", &body, Duration::from_secs(60)).await.map_err(|e| fail(e.to_string(), false))?;
    let sid = start["session_id"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| fail(format!("VCP started no stream: {start}"), false))?.to_string();
    let path = format!("/api/v1/streaming/{sid}/audio?format=wav");
    // VCP direct: this app's other work waits (the proxy does that itself)
    let _turn = ttv_vcp::turn(&path).await;
    let r = vcp.request(ttv_vcp::Method::GET, &path).send().await.map_err(|e| fail(e.to_string(), false))?;
    if !r.status().is_success() {
        return Err(fail(format!("VCP answered HTTP {} for the stream", r.status()), false));
    }
    let mut stream = r.bytes_stream();
    let mut dec = wav::StreamDecoder::new();
    let (mut buf, mut total, mut sent) = (Vec::new(), 0usize, false);
    let started = Instant::now();
    let flush = |buf: &mut Vec<f32>, ch: usize| -> Vec<f32> {
        let whole = buf.len() / ch.max(1) * ch.max(1);
        let frames: Vec<f32> = buf.drain(..whole).collect();
        if ch > 1 { frames.chunks(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect() } else { frames }
    };
    loop {
        let next = tokio::time::timeout(Duration::from_secs(3), stream.next()).await;
        match next {
            Ok(Some(Ok(chunk))) => dec.push(&chunk, &mut buf).map_err(|e| fail(e.to_string(), sent))?,
            Ok(Some(Err(e))) => return Err(fail(e.to_string(), sent)),
            Ok(None) => break,
            Err(_) => {
                // no data for a while: done, failed or just slow?
                let st: serde_json::Value = vcp.get(&format!("/api/v1/streaming/{sid}"), Duration::from_secs(10)).await.unwrap_or_default();
                if let Some(e) = st["error"].as_str().filter(|e| !e.is_empty()) {
                    return Err(fail(format!("VCP: {e}"), sent));
                }
                if st["is_complete"].as_bool() == Some(true) || started.elapsed() > Duration::from_secs(300) {
                    break;
                }
            }
        }
        if let Some(f) = dec.format()
            && buf.len() >= f.sample_rate as usize * f.channels as usize * 3 / 10
        {
            let block = flush(&mut buf, f.channels as usize);
            total += block.len();
            sent = true;
            if tx.send(Ok((block, f.sample_rate))).await.is_err() {
                // skipped: nobody listens any more
                break;
            }
        }
    }
    touch();
    let Some(f) = dec.format() else { return Err(fail("VCP's stream had no audio".into(), sent)) };
    if !buf.is_empty() {
        let block = flush(&mut buf, f.channels as usize);
        total += block.len();
        let _ = tx.send(Ok((block, f.sample_rate))).await;
    }
    Ok(total as f32 / f.sample_rate.max(1) as f32)
}

/// Translates chat text with VCP (`POST /api/v1/translate`, one result per
/// text). `target` and `source` are language codes ("de", "en").
pub async fn translate(vcp: &ttv_vcp::Client, texts: &[String], target: &str, source: Option<&str>) -> Result<Vec<String>, String> {
    touch();
    let mut body = serde_json::json!({ "texts": texts, "target_lang": target, "candidates": 1 });
    if let Some(s) = source {
        body["source_lang"] = s.into();
    }
    let v: serde_json::Value = vcp.post("/api/v1/translate", &body, Duration::from_secs(60)).await.map_err(|e| e.to_string())?;
    touch();
    let out = translations(&v).ok_or_else(|| format!("unexpected answer from VCP's translator: {}", v.to_string().chars().take(200).collect::<String>()))?;
    if out.len() != texts.len() {
        return Err(format!("VCP translated {} of {} texts", out.len(), texts.len()));
    }
    Ok(out)
}

/// The translated texts in VCP's answer: a list of strings, or of objects
/// with the text, possibly under a key like "translations" or "results".
fn translations(v: &serde_json::Value) -> Option<Vec<String>> {
    let text_of = |x: &serde_json::Value| -> Option<String> {
        match x {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(o) => ["text", "translation", "translated", "target", "output", "result"].iter().find_map(|k| o.get(*k).and_then(|t| t.as_str()).map(str::to_string)),
            _ => None,
        }
    };
    match v {
        serde_json::Value::Array(a) => a.iter().map(text_of).collect(),
        serde_json::Value::Object(o) => ["translations", "results", "texts", "items", "data", "outputs"].iter().find_map(|k| o.get(*k).and_then(|x| x.as_array()).and_then(|a| a.iter().map(text_of).collect())),
        _ => None,
    }
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

