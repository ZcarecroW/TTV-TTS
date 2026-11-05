//! The app's side of the chat reader (`ttv_reader::reader`): VCP through the
//! proxy or on this PC, the computed voices and presets, the audio device
//! and plugins, and the window that shows what happens.

use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use tauri::Emitter;
pub use ttv_reader::reader::{ItemView, ProfileStore, ReaderStatus, Settings, ViewerView};
use ttv_reader::output::Output;
use ttv_reader::profile::{OutputMode, Profile};
use ttv_reader::reader::Host;
use ttv_reader::translate::Translator;

use crate::state::AppState;

pub type Reader = ttv_reader::reader::Reader<AppHost>;

/// The app as the reader's host. Knows the app state once it exists (the
/// reader is part of it).
pub struct AppHost {
    state: OnceLock<Weak<AppState>>,
    translator: Translator,
    /// The voice host turned out not to apply effects (an older proxy).
    no_host_fx: std::sync::atomic::AtomicBool,
}

impl AppHost {
    pub fn new(http: reqwest::Client) -> Self {
        Self { state: OnceLock::new(), translator: Translator::new(http), no_host_fx: Default::default() }
    }

    pub fn attach(&self, state: &Arc<AppState>) {
        let _ = self.state.set(Arc::downgrade(state));
    }

    fn state(&self) -> Option<Arc<AppState>> {
        self.state.get()?.upgrade()
    }
}

impl Host for AppHost {
    fn vcp(&self) -> Option<ttv_vcp::Client> {
        self.state()?.vcp()
    }

    fn busy(&self) -> bool {
        ttv_vcp::exclusive() || self.state().is_some_and(|s| s.host.read().vcp.as_ref().is_some_and(|v| v.benchmark))
    }

    fn connected(&self) -> bool {
        self.state().is_some_and(|s| s.host.read().connected)
    }

    fn library_entry(&self, key: &str) -> Option<ttv_link::LibraryEntry> {
        self.state()?.library.get(key)
    }

    fn with_preset<R>(&self, id: &str, f: impl FnOnce(&[ttv_link::VoiceDef]) -> R) -> Option<R> {
        self.state()?.presets.with(id, |p| f(&p.defs))
    }

    fn output(&self, profile: &Profile) -> Arc<dyn Output> {
        match (self.state(), profile.output.mode) {
            (Some(s), OutputMode::Obs) => s.obs.get(&profile.id),
            (Some(s), _) => s.audio.clone(),
            (None, _) => Arc::new(Silent),
        }
    }

    fn effects_on_host(&self) -> bool {
        let Some(s) = self.state() else { return false };
        let c = s.cfg.read();
        // a friend's proxy (guest) offers no effects
        c.effects_on_host && matches!(c.host, crate::config::HostConfig::Proxy { guest: false, .. }) && !self.no_host_fx.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn effects_unsupported(&self) {
        self.no_host_fx.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn reads(&self, profile: &Profile) -> bool {
        // the voice host PC reads these itself
        profile.output.mode != OutputMode::HostObs
    }

    fn status_changed(&self, status: &ReaderStatus) {
        if let Some(app) = self.state().as_ref().and_then(|s| s.app().cloned()) {
            let _ = app.emit("app://reader", status);
        }
    }

    fn item_changed(&self, item: &ItemView) {
        if let Some(app) = self.state().as_ref().and_then(|s| s.app().cloned()) {
            let _ = app.emit("app://history", item);
        }
    }

    fn settings(&self) -> Settings {
        let Some(s) = self.state() else { return Settings::default() };
        let streaming = crate::bench::mode(&s) == crate::config::GenerationMode::Streaming;
        let c = s.cfg.read();
        Settings { gap_ms: c.audio.gap_ms, streaming, translate_email: c.translate_email.clone() }
    }

    fn deepl_key(&self) -> Option<String> {
        self.state()?.secrets.get(crate::secrets::DEEPL_KEY)
    }

    fn set_volume(&self, volume: f32) {
        if let Some(s) = self.state() {
            s.audio.set_volume(volume);
            s.cfg.write().audio.volume = volume;
            let _ = s.save_config();
        }
    }

    fn translator(&self) -> &Translator {
        &self.translator
    }

    async fn os_speak(&self, text: &str, lang: &str) -> Result<(Vec<f32>, u32), String> {
        let cfg = self.state().map(|s| s.cfg.read().os_voice.clone()).unwrap_or_default();
        crate::os_voice::speak(text, lang, &cfg).await
    }

    fn plugin_pool(&self) -> Vec<(String, u32)> {
        self.state().map(|s| s.cfg.read().viewer_plugins.iter().filter(|v| v.enabled).map(|v| (v.id.clone(), v.weight)).collect()).unwrap_or_default()
    }

    fn plugins_on_host(&self, variant: Option<&str>) -> bool {
        self.state().is_some_and(|s| crate::plugin_cmds::on_host(&s, variant))
    }

    type Stage = Option<crate::plugin_cmds::Stage>;

    async fn stage(&self, variant: Option<&str>, on_host: bool) -> Self::Stage {
        let s = self.state()?;
        Some(crate::plugin_cmds::Stage::new(&s, variant, on_host).await)
    }

    async fn stage_run(&self, stage: &Self::Stage, samples: Vec<f32>, rate: u32) -> (Vec<f32>, u32) {
        match (stage, self.state()) {
            (Some(st), Some(s)) => st.run(&s, samples, rate).await,
            _ => (samples, rate),
        }
    }

    async fn stage_tail(&self, stage: &Self::Stage) -> Option<(Vec<f32>, u32)> {
        let (st, s) = (stage.as_ref()?, self.state()?);
        st.tail(&s).await
    }
}

/// While the app shuts down.
struct Silent;

impl Output for Silent {
    fn play(&self, _samples: Vec<f32>, _rate: u32) -> u64 {
        0
    }
    fn is_finished(&self, _id: u64) -> bool {
        true
    }
    fn skip_all(&self) {}
}

/// The audio device as an output of the reader.
impl Output for crate::audio::Audio {
    fn play(&self, samples: Vec<f32>, rate: u32) -> u64 {
        crate::audio::Audio::play(self, samples, rate, 1.0)
    }
    fn is_finished(&self, id: u64) -> bool {
        crate::audio::Audio::is_finished(self, id)
    }
    fn skip_all(&self) {
        crate::audio::Audio::skip_all(self)
    }
}

/// Starts reading chat, the optional Twitch login (EventSub) and the
/// generation benchmark.
pub fn start(state: &Arc<AppState>) {
    state.reader.clone().start();
    crate::twitch_cmds::start(state);
    tokio::spawn(on_connect(state.clone()));
}

/// When the voice host (re)connects: warm VCP up and, on "Automatic"
/// generation, measure both ways once.
async fn on_connect(state: Arc<AppState>) {
    let mut tick = tokio::time::interval(Duration::from_secs(10));
    let mut was_connected = false;
    loop {
        tick.tick().await;
        let connected = state.host.read().connected;
        if connected && !was_connected {
            if !state.reader.store.read().active.is_empty() {
                let st = state.clone();
                tokio::spawn(async move { crate::bench::warm_up(&st).await });
            }
            if state.cfg.read().generation == crate::config::GenerationMode::Auto && crate::bench::current(&state).is_none() {
                tokio::spawn(measure_when_quiet(state.clone()));
            }
        }
        was_connected = connected;
    }
}

/// "Automatic" generation: measure both ways once chat has been quiet for
/// two minutes (it keeps VCP busy for ~30 s on a slow setup).
async fn measure_when_quiet(state: Arc<AppState>) {
    for _ in 0..120 {
        tokio::time::sleep(Duration::from_secs(30)).await;
        if crate::bench::current(&state).is_some() || !state.host.read().connected {
            return;
        }
        let quiet = crate::speech::last_vcp_use().is_none_or(|t| t.elapsed() > Duration::from_secs(120));
        if quiet && state.reader.queue_len() == 0 {
            if let Err(e) = crate::bench::run(&state).await {
                log::info!("generation benchmark: {e}");
            }
            return;
        }
    }
}

/// Live end-to-end test through a real proxy (plays two short messages):
///   TTV_LIVE_PROXY=192.168.178.25:8110 TTV_LIVE_TOKEN=<key or token>
///   cargo test -p ttv-tts live_reader -- --ignored --nocapture
#[cfg(test)]
mod live {
    use super::*;
    use std::time::Instant;
    use ttv_reader::reader::ItemState;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_reader_end_to_end() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let address = std::env::var("TTV_LIVE_PROXY").expect("TTV_LIVE_PROXY");
        let token = std::env::var("TTV_LIVE_TOKEN").expect("TTV_LIVE_TOKEN");
        let dir = std::env::temp_dir().join(format!("ttv-reader-live-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();

        let probe = crate::host::probe(&address).await.expect("probe");
        let mut cfg = crate::config::Config::default();
        cfg.normalize();
        cfg.audio.volume = 0.25;
        cfg.audio.gap_ms = 150;
        cfg.host = crate::config::HostConfig::Proxy { id: probe.info.id.clone(), name: probe.info.name.clone(), address: probe.address.clone(), fingerprint: probe.fingerprint.clone(), mac: None, guest: false };
        let state = crate::state::AppState::new(cfg, dir.join("config.json"), dir.clone());
        let token_key = crate::secrets::proxy_token(&probe.info.id);
        state.secrets.set(&token_key, Some(&token)).unwrap();
        state.host_updated();
        {
            let mut s = state.reader.store.write();
            s.profiles[0].channels = vec!["ttvtts_e2e_test".into()];
            s.profiles[0].language.default = "de".into();
            let id = s.profiles[0].id.clone();
            s.active = vec![id];
        }
        crate::reader::start(&state);
        let t0 = Instant::now();
        state.reader.test_message("ttvtts_e2e_test", "Klaus_Dieter123", "Hallo zusammen, gg! Das war ein richtig gutes Spiel 😂😂");
        state.reader.test_message("ttvtts_e2e_test", "Anya", "Привет всем, как дела у вас сегодня?");
        let done = |s: &ItemState| matches!(s, ItemState::Spoken | ItemState::Failed | ItemState::Skipped | ItemState::Removed);
        while t0.elapsed() < Duration::from_secs(150) {
            let h = state.reader.history();
            if h.len() >= 2 && h.iter().all(|i| done(&i.state)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let h = state.reader.history();
        for i in &h {
            println!("{:?} [{} · {:?} · first audio {:?} ms] {}", i.state, i.lang, i.voice, i.first_audio_ms, i.speech);
            if let Some(r) = &i.reason {
                println!("   reason: {r}");
            }
        }
        println!("total {} ms", t0.elapsed().as_millis());
        let _ = state.secrets.set(&token_key, None);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(h.len(), 2);
        assert!(h.iter().all(|i| i.state == ItemState::Spoken), "both messages spoken");
        assert_eq!(h[0].lang, "de");
        assert_eq!(h[1].lang, "ru");
        assert!(h[0].voice.as_deref().is_some_and(|v| v.starts_with("TTS-de")), "German pool voice");
        assert!(h[1].voice.as_deref().is_some_and(|v| v.starts_with("TTS-ru")), "Russian pool voice");
    }
}
