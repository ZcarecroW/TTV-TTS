//! How fast VCP speaks, measured on real work: seconds of generating per
//! second of audio (the real-time factor; lower is faster). A fresh VCP on
//! the target PC (RTX 3070, 1.7B model) speaks at about 0.5. VCP gets slower
//! the longer it runs, and a restart brings it back, so the proxy compares
//! the recent speed with the best speed seen with the same model and
//! restarts VCP at a quiet moment when it has become clearly slower.
//!
//! Only chat and previews count, and only when VCP spoke right before (no
//! switch of models in between, which costs ~15 s); designing, recognition
//! and the first sentence of a new voice do other or more work.

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

use base64::Engine as _;

/// Recent clips that decide the current speed (median).
pub const WINDOW: usize = 10;
/// Clips shorter than this mostly measure overhead.
const MIN_AUDIO_SECS: f32 = 1.5;
/// Slow: the recent median is this many times the usual speed …
pub const SLOW_FACTOR: f32 = 1.8;
/// … and slower than this (a fast GPU that got a bit slower is still fine).
pub const SLOW_FLOOR: f32 = 0.7;
/// Clearly broken whatever the usual speed: VCP in the state where it loads
/// its model again for every sentence measured 4–6 (a restart: 0.5).
pub const BROKEN: f32 = 2.5;

#[derive(Default)]
struct State {
    model: String,
    recent: VecDeque<f32>,
    /// model -> best recent median seen (the usual speed)
    best: HashMap<String, f32>,
    samples: u64,
}

static PERF: LazyLock<Mutex<State>> = LazyLock::new(Default::default);

fn lock() -> std::sync::MutexGuard<'static, State> {
    PERF.lock().unwrap_or_else(|e| e.into_inner())
}

fn median(v: &VecDeque<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    let mut s: Vec<f32> = v.iter().copied().collect();
    s.sort_by(f32::total_cmp);
    Some(s[s.len() / 2])
}

/// One clip: `secs` of work for `audio_secs` of speech.
pub fn record(secs: f32, audio_secs: f32) {
    if audio_secs < MIN_AUDIO_SECS || !secs.is_finite() || secs <= 0.0 {
        return;
    }
    let mut s = lock();
    s.recent.push_back(secs / audio_secs);
    while s.recent.len() > WINDOW {
        s.recent.pop_front();
    }
    s.samples += 1;
    if s.recent.len() == WINDOW
        && let Some(m) = median(&s.recent)
    {
        let model = s.model.clone();
        let best = s.best.entry(model).or_insert(m);
        *best = best.min(m);
    }
}

/// The loaded model changed: its speed is measured anew.
pub fn set_model(name: &str) {
    let mut s = lock();
    if s.model != name {
        s.model = name.to_string();
        s.recent.clear();
    }
}

fn is_design(kind: Option<&str>, name: &str) -> bool {
    kind.is_some_and(|k| k.contains("design")) || name.to_ascii_lowercase().starts_with("design")
}

/// What VCP has loaded (its model status). Only a speaking model decides
/// whose speed is measured: during jobs VCP loads its design model for each
/// design and the speaking model again afterwards, and that mustn't start
/// the measurement over every time.
pub fn set_loaded(kind: Option<&str>, name: &str) {
    if !is_design(kind, name) {
        set_model(name);
    }
}

/// After a restart the old measurements no longer apply.
pub fn clear_recent() {
    lock().recent.clear();
}

/// Usual speeds per model, to keep them across restarts of the proxy.
pub fn usual() -> HashMap<String, f32> {
    lock().best.clone()
}

pub fn set_usual(map: HashMap<String, f32>) {
    let mut s = lock();
    for (k, v) in map {
        // (older versions also kept the design model's)
        if v.is_finite() && v > 0.0 && !is_design(None, &k) {
            let e = s.best.entry(k).or_insert(v);
            *e = e.min(v);
        }
    }
}

/// The benchmark measured a fresh VCP: this is the usual speed now.
pub fn set_usual_now(rtf: f32) {
    let mut s = lock();
    if !s.model.is_empty() && rtf.is_finite() && rtf > 0.0 {
        let model = s.model.clone();
        s.best.insert(model, rtf);
    }
    s.recent.clear();
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Speed {
    /// Median of the recent clips (None until there are any).
    pub now: Option<f32>,
    /// Best speed seen with this model.
    pub usual: Option<f32>,
    /// Clips in the recent window.
    pub recent: usize,
}

impl Speed {
    /// Clearly slower than usual (or broken), measured on a full window.
    pub fn slow(&self) -> bool {
        let Some(n) = self.now.filter(|_| self.recent >= WINDOW) else { return false };
        n > BROKEN || self.usual.is_some_and(|u| n > u * SLOW_FACTOR && n > SLOW_FLOOR)
    }
}

pub fn speed() -> Speed {
    let s = lock();
    Speed { now: median(&s.recent), usual: s.best.get(&s.model).copied(), recent: s.recent.len() }
}

/// Seconds of audio in a VCP generation answer (`{"audio_base64":"…"}`),
/// from the WAV header at the start of the base64 text and the text's
/// length: `head` holds the first bytes of the answer, `total` its length,
/// `tail` its last bytes. No need to decode or even keep the audio.
pub fn audio_secs(head: &[u8], tail: &[u8], total: usize) -> Option<f32> {
    const KEY: &[u8] = b"\"audio_base64\":\"";
    let at = head.windows(KEY.len()).position(|w| w == KEY)? + KEY.len();
    let b64_head = head.get(at..at + 64)?;
    let wav = base64::engine::general_purpose::STANDARD.decode(b64_head).ok()?;
    if wav.get(0..4)? != b"RIFF" || wav.get(8..12)? != b"WAVE" {
        return None;
    }
    let channels = u16::from_le_bytes([wav[22], wav[23]]) as f32;
    let rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]) as f32;
    let bits = u16::from_le_bytes([wav[34], wav[35]]) as f32;
    if rate < 1000.0 || channels < 1.0 || bits < 8.0 {
        return None;
    }
    // where the base64 text ends: the closing quote, searched in the tail
    let tail_start = total.checked_sub(tail.len())?;
    let end = tail_start + tail.iter().position(|&b| b == b'"').unwrap_or(tail.len());
    let b64_len = end.checked_sub(at)?;
    let bytes = (b64_len as f32 * 0.75 - 44.0).max(0.0);
    Some(bytes / (rate * channels * bits / 8.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(secs: f32, rate: u32) -> Vec<u8> {
        let n = (secs * rate as f32) as u32;
        let mut b = Vec::new();
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
        b.resize(44 + n as usize * 2, 0);
        b
    }

    #[test]
    fn reads_the_length_of_a_generation_answer() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(wav(3.25, 24_000));
        let json = format!("{{\"success\":true,\"status\":\"completed\",\"audio_base64\":\"{b64}\",\"error_code\":null,\"error_detail\":null}}");
        let b = json.as_bytes();
        let secs = audio_secs(&b[..1024], &b[b.len() - 512..], b.len()).unwrap();
        assert!((secs - 3.25).abs() < 0.01, "{secs}");
        assert_eq!(audio_secs(b"{\"error\":\"x\"}", b"", 13), None);
    }

    #[test]
    fn notices_when_vcp_gets_slower() {
        set_model("test-model");
        clear_recent();
        for _ in 0..WINDOW {
            record(2.0, 4.0); // 0.5
        }
        assert_eq!(speed().usual, Some(0.5));
        assert!(!speed().slow());
        // one switch of the checkpoint (17 s) doesn't count
        record(17.0, 4.0);
        assert!(!speed().slow());
        for _ in 0..WINDOW {
            record(4.4, 4.0); // 1.1
        }
        let s = speed();
        assert!(s.slow(), "{s:?}");
        clear_recent();
        assert!(!speed().slow(), "a restart starts over");
        // broken even without a usual speed (a new model)
        set_model("another-model");
        for _ in 0..WINDOW {
            record(18.0, 4.0); // 4.5: the model loads for every sentence
        }
        assert!(speed().slow(), "{:?}", speed());
        set_model("test-model");
        // short clips and nonsense are ignored
        record(5.0, 0.5);
        record(f32::NAN, 3.0);
        assert_eq!(speed().recent, 0);
        // a design between the sentences (as in jobs) doesn't start over
        for _ in 0..5 {
            record(2.0, 4.0);
        }
        set_loaded(Some("voice_design"), "design Higher Quality (1.7B)");
        set_loaded(None, "design Higher Quality (1.7B)");
        assert_eq!(speed().recent, 5);
        assert_eq!(speed().usual, Some(0.5));
        set_loaded(Some("base"), "test-model");
        assert_eq!(speed().recent, 5);
        // design speeds kept by older versions are dropped
        set_usual(HashMap::from([("design Higher Quality (1.7B)".to_string(), 3.75)]));
        assert!(!usual().keys().any(|k| k.starts_with("design")));
    }
}
