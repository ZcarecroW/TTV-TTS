//! One model in VCP's memory at a time.
//!
//! A voice model (4.6 GB) and the speech recognition (4 GB) don't fit on an
//! 8 GB card together. When both are loaded they spill into system memory and
//! everything gets ten times slower until the next restart (design 27–34 s
//! instead of 3.3 s, a chat sentence up to a minute instead of 1.6 s; measured
//! 2026-11-05). VCP's low-VRAM mode prevents that by dropping speech
//! recognition after every clip, which costs 5 s per clip instead of 0.5 s.
//! Without it, nothing unloads speech recognition reliably, and VCP also
//! loads it by itself in the middle of a speaking request (voices with a long
//! sample): it'd stay there during a stream.
//!
//! Low VRAM makes no other difference (benchmark: design 3.10 s on, 3.27 s
//! off, speaking 2.73 s on, 2.72 s off). So it's off exactly while speech
//! recognition does all of its work in one go, and back on before anything
//! else runs. The setting does the unloading too, because VCP has no command
//! for that: switching it unloads the voice model, and with it on, a
//! transcription releases speech recognition afterwards.
//!
//! A PC set to low VRAM off (a bigger card) gets that setting back after each
//! phase, with speech recognition released all the same.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::Engine as _;
use ttv_vcp::Method;

use crate::{Inner, langs};

const SHORT: Duration = Duration::from_secs(30);
const CLIP: Duration = Duration::from_secs(180);

pub(crate) async fn set_low_vram(vcp: &ttv_vcp::Client, on: bool) -> bool {
    vcp.request(Method::PUT, &format!("/api/v1/settings/low-vram?enabled={on}")).timeout(SHORT).send().await.is_ok_and(|r| r.status().is_success())
}

/// VCP's setting; None if it can't tell.
pub(crate) async fn low_vram(vcp: &ttv_vcp::Client) -> Option<bool> {
    let v: serde_json::Value = vcp.get("/api/v1/settings/low-vram", SHORT).await.ok()?;
    v["enabled"].as_bool()
}

/// Speech recognition is next: the voice model goes, and it may stay loaded
/// between clips. The caller holds the turn.
pub(crate) async fn enter(vcp: &ttv_vcp::Client, found: bool) {
    // only a real change unloads the voice model
    if !found {
        set_low_vram(vcp, true).await;
    }
    set_low_vram(vcp, false).await;
}

/// Speech recognition goes, and the setting is what it was. The caller holds
/// the turn.
pub(crate) async fn leave(vcp: &ttv_vcp::Client, clip: &[u8], lang: &langs::DesignLang, back_to: bool) {
    set_low_vram(vcp, true).await;
    let body = serde_json::json!({ "audio_base64": base64::engine::general_purpose::STANDARD.encode(clip), "language": lang.vcp });
    let r = vcp.request(Method::POST, "/api/v1/transcription/start?sync=true").json(&body).timeout(CLIP).send().await;
    if let Ok(r) = r {
        let _ = r.bytes().await;
    }
    if !back_to {
        set_low_vram(vcp, false).await;
    }
}

/// A clip for releasing speech recognition when no real one is at hand.
fn tone() -> Vec<u8> {
    const RATE: u32 = 16_000;
    let samples: Vec<f32> = (0..RATE * 3 / 2).map(|i| (i as f32 / RATE as f32 * 220.0 * std::f32::consts::TAU).sin() * 0.1).collect();
    crate::audio::wav_pcm16(&samples, RATE)
}

/// Before the first work after a start: a proxy that stopped in the middle of
/// a phase (a crash, an update) left speech recognition and the setting behind.
pub(crate) async fn settle(inner: &Inner, vcp: &ttv_vcp::Client) {
    let was = inner.stats.lock().low_vram_was;
    let Some(found) = low_vram(vcp).await else { return };
    // it's on and nothing was going on: all is well
    if found && was.is_none() {
        return;
    }
    let _turn = ttv_vcp::background_turn("/ttv/models").await;
    enter(vcp, found).await;
    leave(vcp, &tone(), &langs::LANGS[0], was.unwrap_or(found)).await;
    remember(inner, None);
}

/// What the setting was while a phase runs: survives a crash.
fn remember(inner: &Inner, was: Option<bool>) {
    inner.stats.lock().low_vram_was = was;
    inner.save_stats();
}

/// Speech recognition at work: from the first clip until it's released.
pub(crate) struct Listening {
    vcp: ttv_vcp::Client,
    /// A clip to release it with, and the setting to go back to. None: VCP
    /// can't tell its setting, it's left alone.
    plan: Option<(Arc<Vec<u8>>, &'static langs::DesignLang, bool)>,
    /// Chat came in and released it: it loads again for the rest of the phase.
    released: Arc<AtomicBool>,
}

impl Listening {
    /// Before the first clip of a phase. `clip` is any short one of the phase.
    pub(crate) async fn begin(inner: &Inner, vcp: &ttv_vcp::Client, clip: Option<&[u8]>, lang: &'static langs::DesignLang) -> Self {
        let found = if clip.is_some() { low_vram(vcp).await } else { None };
        let plan = clip.zip(found).map(|(c, f)| (Arc::new(c.to_vec()), lang, f));
        let this = Self { vcp: vcp.clone(), plan, released: Arc::new(AtomicBool::new(false)) };
        if let Some(found) = found {
            remember(inner, Some(found));
            let _turn = ttv_vcp::background_turn("/ttv/models").await;
            enter(vcp, found).await;
        }
        this.arm();
        this
    }

    /// Chat needs the memory at once: the next chat request releases speech
    /// recognition first.
    fn arm(&self) {
        if let Some((clip, lang, found)) = &self.plan {
            let (vcp, clip, lang, found, released) = (self.vcp.clone(), clip.clone(), *lang, *found, self.released.clone());
            ttv_vcp::before_next_chat(Some(Box::pin(async move {
                leave(&vcp, &clip, lang, found).await;
                released.store(true, Ordering::SeqCst);
            })));
        }
    }

    /// Before every clip: after chat, the phase goes on where it was.
    pub(crate) async fn keep_ready(&self) {
        if self.released.swap(false, Ordering::SeqCst)
            && let Some((_, _, found)) = &self.plan
        {
            let _turn = ttv_vcp::background_turn("/ttv/models").await;
            enter(&self.vcp, *found).await;
        }
        self.arm();
    }

    /// The phase is over, however it ended.
    pub(crate) async fn end(self, inner: &Inner) {
        ttv_vcp::before_next_chat(None);
        if let Some((clip, lang, found)) = &self.plan {
            if !self.released.load(Ordering::SeqCst) {
                let _turn = ttv_vcp::background_turn("/ttv/models").await;
                leave(&self.vcp, clip, lang, *found).await;
            }
            remember(inner, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::Router;
    use axum::extract::{Query, State};
    use axum::routing::{get, post};

    use super::*;

    #[derive(Default)]
    struct Vcp {
        low: AtomicBool,
        calls: Mutex<Vec<String>>,
    }

    type S = State<Arc<Vcp>>;

    async fn get_low(State(v): S) -> axum::Json<serde_json::Value> {
        axum::Json(serde_json::json!({ "enabled": v.low.load(Ordering::SeqCst) }))
    }

    async fn put_low(State(v): S, Query(q): Query<std::collections::HashMap<String, String>>) -> axum::Json<serde_json::Value> {
        let on = q.get("enabled").is_some_and(|x| x == "true");
        v.low.store(on, Ordering::SeqCst);
        v.calls.lock().unwrap().push(if on { "on" } else { "off" }.into());
        axum::Json(serde_json::json!({ "success": true }))
    }

    async fn listen(State(v): S, _body: String) -> axum::Json<serde_json::Value> {
        v.calls.lock().unwrap().push("clip".into());
        axum::Json(serde_json::json!({ "success": true, "text": "" }))
    }

    /// The whole dance around chat, in one test: the slot for chat is one per
    /// process, so nothing else may use it meanwhile.
    #[tokio::test(flavor = "multi_thread")]
    async fn chat_in_the_middle_of_speech_recognition_gets_the_memory_first() {
        let v = Arc::new(Vcp { low: AtomicBool::new(true), ..Default::default() });
        let app = Router::new().route("/api/v1/settings/low-vram", get(get_low).put(put_low)).route("/api/v1/transcription/start", post(listen)).with_state(v.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let vcp = ttv_vcp::Client::new(&base, None);
        let dir = std::env::temp_dir().join(format!("ttv-engine-models-{}", rand::random::<u64>()));
        let e = crate::Engine::new(dir, Arc::new(|| None));
        let calls = || std::mem::take(&mut *v.calls.lock().unwrap()).join(" ");
        let lang = &langs::LANGS[0];

        let l = Listening::begin(&e.inner, &vcp, Some(b"clip"), lang).await;
        assert_eq!(calls(), "off", "stays loaded between clips");
        assert_eq!(e.inner.stats.lock().low_vram_was, Some(true), "known after a crash");
        l.keep_ready().await;
        assert_eq!(calls(), "", "nothing to do between clips");

        // chat: speech recognition goes first, it waits for nothing else
        drop(ttv_vcp::turn("/api/v1/generation/clone").await);
        assert_eq!(calls(), "on clip");
        assert!(v.low.load(Ordering::SeqCst));

        // the phase goes on where it was
        l.keep_ready().await;
        assert_eq!(calls(), "off");
        drop(ttv_vcp::turn("/api/v1/generation/clone").await);
        assert_eq!(calls(), "on clip", "and chat comes first again");
        l.keep_ready().await;
        assert_eq!(calls(), "off");

        // the end: released, and the setting is back
        l.end(&e.inner).await;
        assert_eq!(calls(), "on clip");
        assert!(v.low.load(Ordering::SeqCst));
        assert_eq!(e.inner.stats.lock().low_vram_was, None);

        // nothing is left for the next chat message
        drop(ttv_vcp::turn("/api/v1/generation/clone").await);
        assert_eq!(calls(), "");

        // a PC set to low VRAM off keeps that
        v.low.store(false, Ordering::SeqCst);
        let l = Listening::begin(&e.inner, &vcp, Some(b"clip"), lang).await;
        assert_eq!(calls(), "on off", "the voice model goes");
        l.end(&e.inner).await;
        assert_eq!(calls(), "on clip off", "speech recognition goes, the setting stays");
    }
}
