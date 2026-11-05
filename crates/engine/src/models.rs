//! One model in VCP's memory at a time.
//!
//! A voice model (4.6 GB) and the speech recognition (4 GB) don't fit on an
//! 8 GB card together. VCP doesn't unload one reliably before it loads the
//! other: both spill into system memory and everything gets ten times slower
//! until the next restart (design 27–34 s instead of 3.3 s, chat 16 s per
//! sentence instead of 1.6 s; measured 2026-11-05 with low VRAM off, within
//! minutes of mixed work). Its low-VRAM mode avoids that by dropping speech
//! recognition after every clip, which costs 5 s per clip instead of 0.6 s.
//!
//! So each model does all of its work in one go and is really gone before the
//! next one loads. VCP has no command for that, but its low-VRAM setting does
//! both jobs when it's touched at the right moment:
//!   - switching it unloads the voice model,
//!   - with it on, a transcription releases speech recognition afterwards.
//!
//! The stored setting stays off. With low VRAM on (somebody's choice, or a
//! smaller card) VCP does this by itself and nothing here happens.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use ttv_vcp::Method;

use crate::langs;

const SHORT: Duration = Duration::from_secs(30);
const CLIP: Duration = Duration::from_secs(180);

async fn set_low_vram(vcp: &ttv_vcp::Client, on: bool) -> bool {
    vcp.request(Method::PUT, &format!("/api/v1/settings/low-vram?enabled={on}")).timeout(SHORT).send().await.is_ok_and(|r| r.status().is_success())
}

/// VCP's setting; None if it can't tell.
pub(crate) async fn low_vram(vcp: &ttv_vcp::Client) -> Option<bool> {
    let v: serde_json::Value = vcp.get("/api/v1/settings/low-vram", SHORT).await.ok()?;
    v["enabled"].as_bool()
}

/// Unloads the voice model, so speech recognition has the memory to itself.
/// The caller holds the turn.
pub(crate) async fn clear(vcp: &ttv_vcp::Client) {
    set_low_vram(vcp, true).await;
    set_low_vram(vcp, false).await;
}

/// Releases speech recognition. The caller holds the turn.
pub(crate) async fn release(vcp: &ttv_vcp::Client, clip: &[u8], lang: &langs::DesignLang) {
    set_low_vram(vcp, true).await;
    let body = serde_json::json!({ "audio_base64": base64::engine::general_purpose::STANDARD.encode(clip), "language": lang.vcp });
    let r = vcp.request(Method::POST, "/api/v1/transcription/start?sync=true").json(&body).timeout(CLIP).send().await;
    if let Ok(r) = r {
        let _ = r.bytes().await;
    }
    set_low_vram(vcp, false).await;
}

/// Speech recognition at work: from the first clip until it's released.
pub(crate) struct Listening {
    vcp: ttv_vcp::Client,
    /// A clip to release it with; None: VCP looks after it by itself.
    clip: Option<(Arc<Vec<u8>>, &'static langs::DesignLang)>,
}

impl Listening {
    /// Before the first clip of a phase. `clip` is any short one of the phase.
    pub(crate) async fn begin(vcp: &ttv_vcp::Client, clip: Option<&[u8]>, lang: &'static langs::DesignLang) -> Self {
        let managed = clip.is_some() && low_vram(vcp).await == Some(false);
        let this = Self { vcp: vcp.clone(), clip: clip.filter(|_| managed).map(|c| (Arc::new(c.to_vec()), lang)) };
        if managed {
            let _turn = ttv_vcp::background_turn("/ttv/models").await;
            clear(vcp).await;
        }
        this.keep_ready();
        this
    }

    /// Chat may come in now: it'd need the memory at once, so the next chat
    /// request releases speech recognition first. Again before every clip:
    /// after chat, speech recognition comes back for the rest of the phase.
    pub(crate) fn keep_ready(&self) {
        if let Some((clip, lang)) = &self.clip {
            let (vcp, clip, lang) = (self.vcp.clone(), clip.clone(), *lang);
            ttv_vcp::before_next_chat(Some(Box::pin(async move { release(&vcp, &clip, lang).await })));
        }
    }

    /// The phase is over, however it ended.
    pub(crate) async fn end(self) {
        ttv_vcp::before_next_chat(None);
        if let Some((clip, lang)) = &self.clip {
            let _turn = ttv_vcp::background_turn("/ttv/models").await;
            release(&self.vcp, clip, lang).await;
        }
    }
}
