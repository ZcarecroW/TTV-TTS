//! Speech in the app: the shared generation code (`ttv_reader::speech`) plus
//! saying something right now on the audio output (tests, previews).

use std::sync::Arc;
use std::time::{Duration, Instant};

pub use ttv_reader::speech::*;

/// Says `u` now: generates piece by piece and plays as soon as the first one
/// is ready. Returns when the last piece finished playing (or was skipped).
pub async fn say(vcp: ttv_vcp::Client, state: &Arc<crate::state::AppState>, u: Utterance) -> Result<SayReport, String> {
    let audio = state.audio.clone();
    let stage = crate::plugin_cmds::Stage::new(state).await;
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
        let out = match fx {
            Some(f) => chain.get_or_insert_with(|| crate::fx::Chain::new(f, rate)).process(samples, u.gain),
            None => {
                let mut s = samples;
                if u.gain != 1.0 {
                    s.iter_mut().for_each(|x| *x *= u.gain);
                }
                s
            }
        };
        let (out, out_rate) = stage.run(state, out, rate).await;
        last_id = audio.play(out, out_rate, 1.0);
        if report.first_audio_ms == 0 {
            report.first_audio_ms = t0.elapsed().as_millis() as u64;
        }
    }
    let _ = producer.await;
    if let Some(c) = chain {
        let rate = c.rate;
        let tail = c.finish();
        if !tail.is_empty() {
            let (tail, r) = stage.run(state, tail, rate).await;
            last_id = audio.play(tail, r, 1.0);
        }
    }
    if let Some((t, r)) = stage.tail(state).await {
        last_id = audio.play(t, r, 1.0);
    }
    if last_id != 0 {
        audio.wait(last_id).await;
    }
    report.total_ms = t0.elapsed().as_millis() as u64;
    report.rtf = if report.audio_secs > 0.0 { gen_time.as_secs_f32() / report.audio_secs } else { 0.0 };
    Ok(report)
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

        let audio = Arc::new(crate::audio::Audio::start(None, 0.25));
        let u = Utterance { text: "Hello from TTV-TTS. This sentence was generated on the other PC.".into(), voice, language: "English".into(), emotion: None, intensity: None, gain: 1.0, fx: None };
        let t0 = Instant::now();
        let (samples, rate) = generate(&vcp, &u, &u.text).await.expect("generate");
        println!("generated {:.1} s in {} ms", samples.len() as f32 / rate as f32, t0.elapsed().as_millis());
        audio.wait(audio.play(samples.clone(), rate, 1.0)).await;
        assert!(samples.len() as f32 / rate as f32 > 1.0);
    }
}
