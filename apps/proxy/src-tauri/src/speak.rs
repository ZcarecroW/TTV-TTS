//! `POST /ttv/v1/speak`: one piece of speech with the voice's effect and
//! loudness correction applied here, so the gaming PC only plays it. Pieces
//! of one message share the effect's state (echo tails run on into the next
//! piece); the last one brings the tail.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use ttv_reader::fx::{Chain, Fx};
use ttv_reader::speech::{self, SpeakRequest, Utterance};

use crate::server::api_error;
use crate::state::AppState;

/// Effects of messages in progress: stream id -> (chain, last use).
static CHAINS: Mutex<Option<HashMap<String, (Chain, Instant)>>> = Mutex::new(None);
/// A message's chain is dropped this long after its last piece (cancelled messages).
const FORGET: Duration = Duration::from_secs(120);

pub async fn speak(State(state): State<Arc<AppState>>, Json(req): Json<SpeakRequest>) -> Response {
    // chat goes first: computing jobs wait, the proxy doesn't update now
    crate::forward::touch_work();
    crate::updater::touch();
    let vcp = state.vcp();
    let u = Utterance { text: req.text.clone(), voice: req.voice.clone(), language: req.language.clone(), emotion: req.emotion.clone(), intensity: req.intensity.clone(), gain: req.gain, fx: None };
    let (samples, rate) = match speech::generate(&vcp, &u, &req.text).await {
        Ok(x) => x,
        Err(e) => return api_error(StatusCode::BAD_GATEWAY, "vcp_failed", e),
    };
    crate::forward::touch_work();
    let gain = if req.gain > 0.0 { req.gain } else { 1.0 };
    let out = match req.fx.as_deref().and_then(Fx::parse) {
        Some(fx) => process(fx, &req.stream, req.last, samples, rate, gain),
        None => {
            let mut s = samples;
            if gain != 1.0 {
                s.iter_mut().for_each(|x| *x *= gain);
            }
            s
        }
    };
    // then the viewer's plugin effect and the chain on every voice
    let (out, rate) = match &req.plugins {
        Some(pl) => {
            let (mut out, r) = state.plugins.run("speak", pl.variant.as_deref(), pl.main, out, rate).await;
            if req.last || req.stream.is_empty() {
                out.extend(state.plugins.tail("speak", pl.variant.as_deref(), pl.main).await);
            }
            (out, r)
        }
        None => (out, rate),
    };
    ([(header::CONTENT_TYPE, "audio/wav"), (header::CACHE_CONTROL, "no-store")], speech::encode_wav16(&out, rate)).into_response()
}

/// Runs a piece through its message's effect chain (made for the first
/// piece); the last piece also gets the chain's tail.
fn process(fx: Fx, stream: &str, last: bool, samples: Vec<f32>, rate: u32, gain: f32) -> Vec<f32> {
    let mut chain = {
        let mut g = CHAINS.lock();
        let map = g.get_or_insert_with(HashMap::new);
        map.retain(|_, (_, t)| t.elapsed() < FORGET);
        map.remove(stream).map(|(c, _)| c).filter(|c| c.fx == fx && c.rate == rate).unwrap_or_else(|| Chain::new(fx, rate))
    };
    let mut out = chain.process(samples, gain);
    if last || stream.is_empty() {
        out.extend(chain.finish());
    } else {
        CHAINS.lock().get_or_insert_with(HashMap::new).insert(stream.to_string(), (chain, Instant::now()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_keeps_its_effect_across_pieces_and_ends_with_the_tail() {
        let piece = vec![0.2f32; 2400];
        let a = process(Fx::Cave, "m1", false, piece.clone(), 24_000, 1.0);
        assert!(CHAINS.lock().as_ref().unwrap().contains_key("m1"), "kept for the next piece");
        let b = process(Fx::Cave, "m1", true, piece.clone(), 24_000, 1.0);
        assert!(!CHAINS.lock().as_ref().unwrap().contains_key("m1"), "gone after the last piece");
        assert_eq!(a.len(), 2400);
        assert!(b.len() > 2400, "the last piece brings the echo's tail");
        // a single piece without a stream id: processed and finished at once
        assert!(process(Fx::Radio, "", false, piece, 24_000, 1.0).len() >= 2400);
    }
}
