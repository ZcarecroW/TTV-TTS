//! OBS links on the voice host PC: profiles that the app hands over ("play
//! in OBS through the voice host PC") are read here, each by its own chat
//! reader (the same code as in the app), and played to OBS browser sources:
//! over plain HTTP in the network, over HTTPS from the internet (OBS doesn't
//! trust the proxy's own certificate, so these links use a Let's Encrypt
//! certificate for the DuckDNS name; see `ttv_net`).
//!
//! A hosted profile reads its chat only while a browser source listens (and
//! a minute after the last one left), so a friend's link costs nothing while
//! they're offline. Each one keeps its viewers' voices in its own folder.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use ttv_link::{Event, LibraryEntry, VoiceDef};
use ttv_reader::obs::{self, Links, ObsSink, Refusal};
use ttv_reader::output::Output;
use ttv_reader::profile::{OutputMode, Profile};
use ttv_reader::reader::{Host, ItemView, Reader, ReaderStatus, Settings, ViewerView};
use ttv_reader::translate::Translator;

use crate::state::AppState;

/// After the last browser source left, keep reading this long (OBS reloads,
/// scene switches with "shut down source when not visible").
const LINGER: Duration = Duration::from_secs(60);

/// The voices of the profile's preset, as the app knows them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HostedPreset {
    pub id: String,
    pub defs: Vec<VoiceDef>,
}

/// The app's settings that matter for reading.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HostedSettings {
    pub gap_ms: u32,
    pub streaming: bool,
    pub translate_email: String,
}

impl Default for HostedSettings {
    fn default() -> Self {
        Self { gap_ms: 250, streaming: false, translate_email: String::new() }
    }
}

/// `PUT /ttv/v1/hosted/{id}`: a profile to read here.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HostedRequest {
    pub profile: Profile,
    pub preset: Option<HostedPreset>,
    pub settings: HostedSettings,
    /// The viewers' voices so far (user id -> role), so nobody's voice
    /// changes when a profile moves here; kept only for viewers new here.
    pub roles: HashMap<String, u32>,
    /// Viewers' own effects (user id -> effect).
    pub fx: HashMap<String, String>,
    /// Viewers' plugin effects.
    pub plugins: HashMap<String, ttv_reader::reader::PluginPick>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    preset: Option<HostedPreset>,
    settings: HostedSettings,
}

/// What the app and the window see of one hosted profile.
#[derive(Debug, Clone, Serialize)]
pub struct HostedView {
    pub id: String,
    pub name: String,
    pub channels: Vec<String>,
    pub online: bool,
    /// Browser sources connected right now.
    pub listeners: usize,
    /// Reading chat right now (somebody listens, or did a minute ago).
    pub reading: bool,
    pub status: ReaderStatus,
    /// The link in this network (one per address of this PC).
    pub lan: Vec<String>,
    /// The link from the internet (DuckDNS name or public IP + forwarded port).
    pub internet: Option<String>,
    /// That link uses HTTPS.
    pub internet_secure: bool,
    /// Shared on the internet, but the HTTPS certificate is still being fetched.
    pub securing: bool,
}

/// Shared by a hosted profile's reader host and its entry.
struct Shared {
    id: String,
    sink: Arc<ObsSink>,
    saved: RwLock<Saved>,
}

pub struct ProxyHost {
    state: Weak<AppState>,
    shared: Arc<Shared>,
    translator: Translator,
}

impl Host for ProxyHost {
    fn vcp(&self) -> Option<ttv_vcp::Client> {
        let st = self.state.upgrade()?;
        // chat goes first: computing jobs wait, the proxy doesn't update now
        crate::forward::touch_work();
        crate::updater::touch();
        Some(st.vcp())
    }

    fn vcp_quiet(&self) -> Option<ttv_vcp::Client> {
        Some(self.state.upgrade()?.vcp())
    }

    fn busy(&self) -> bool {
        ttv_vcp::exclusive()
    }

    fn connected(&self) -> bool {
        self.state.upgrade().is_some_and(|s| s.vcp_state.read().reachable)
    }

    fn library_entry(&self, key: &str) -> Option<LibraryEntry> {
        self.state.upgrade()?.engine.entry(key)
    }

    fn with_preset<R>(&self, id: &str, f: impl FnOnce(&[VoiceDef]) -> R) -> Option<R> {
        let saved = self.shared.saved.read();
        let p = saved.preset.as_ref().filter(|p| p.id == id)?;
        Some(f(&p.defs))
    }

    fn output(&self, _profile: &Profile) -> Arc<dyn Output> {
        self.shared.sink.clone()
    }

    fn status_changed(&self, status: &ReaderStatus) {
        if let Some(st) = self.state.upgrade() {
            let _ = st.events.send(Event::Hosted { id: self.shared.id.clone(), status: serde_json::to_value(status).ok(), item: None });
        }
    }

    fn item_changed(&self, item: &ItemView) {
        if let Some(st) = self.state.upgrade() {
            let _ = st.events.send(Event::Hosted { id: self.shared.id.clone(), status: None, item: serde_json::to_value(item).ok() });
        }
    }

    fn settings(&self) -> Settings {
        let s = &self.shared.saved.read().settings;
        Settings { gap_ms: s.gap_ms, streaming: s.streaming, translate_email: s.translate_email.clone() }
    }

    fn deepl_key(&self) -> Option<String> {
        // stays on the streamer's PC
        None
    }

    fn set_volume(&self, _volume: f32) {
        // OBS has its own fader
    }

    fn translator(&self) -> &Translator {
        &self.translator
    }

    async fn os_speak(&self, _text: &str, _lang: &str) -> Result<(Vec<f32>, u32), String> {
        Err("the voice host PC has no fallback voice".into())
    }

    fn plugin_pool(&self) -> Vec<(String, u32)> {
        self.state.upgrade().map(|s| s.plugins.view().chains.variants.iter().filter(|v| v.enabled).map(|v| (v.id.clone(), v.weight)).collect()).unwrap_or_default()
    }

    /// The app's plugins run here, in this profile's own instances.
    type Stage = Option<(Arc<AppState>, Option<String>)>;

    async fn stage(&self, variant: Option<&str>, _on_host: bool) -> Self::Stage {
        let s = self.state.upgrade()?;
        s.plugins.has_work(variant, true).then(|| (s, variant.map(str::to_string)))
    }

    async fn stage_run(&self, stage: &Self::Stage, samples: Vec<f32>, rate: u32) -> (Vec<f32>, u32) {
        match stage {
            Some((s, v)) => s.plugins.run(&format!("h:{}", self.shared.id), v.as_deref(), true, samples, rate).await,
            None => (samples, rate),
        }
    }

    async fn stage_tail(&self, stage: &Self::Stage) -> Option<(Vec<f32>, u32)> {
        let (s, v) = stage.as_ref()?;
        let t = s.plugins.tail(&format!("h:{}", self.shared.id), v.as_deref(), true).await;
        (!t.is_empty()).then_some((t, crate::plugins::RATE))
    }
}

struct Entry {
    reader: Arc<Reader<ProxyHost>>,
    shared: Arc<Shared>,
    dir: PathBuf,
    /// Since when nobody listens (None while somebody does).
    idle: Mutex<Option<Instant>>,
}

impl Entry {
    fn profile(&self) -> Profile {
        self.reader.store.read().profiles.first().cloned().unwrap_or_default()
    }

    fn reading(&self) -> bool {
        !self.reader.store.read().active.is_empty()
    }

    fn set_reading(&self, on: bool) {
        {
            let mut s = self.reader.store.write();
            s.active = if on { s.profiles.iter().map(|p| p.id.clone()).take(1).collect() } else { Vec::new() };
        }
        self.reader.apply_channels();
        log::info!("OBS link {}: {} reading chat", self.shared.id, if on { "started" } else { "stopped" });
    }

    fn save(&self) {
        let saved = self.shared.saved.read().clone();
        let _ = ttv_reader::storage::write_json(&self.dir.join("hosted.json"), &saved);
        self.reader.save_store();
    }
}

/// All hosted profiles.
pub struct Hosted {
    dir: PathBuf,
    entries: RwLock<HashMap<String, Arc<Entry>>>,
}

fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl Hosted {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self { dir: data_dir.join("hosted"), entries: RwLock::new(HashMap::new()) }
    }

    /// Loads the hosted profiles kept on disk (call once at start).
    pub fn load(&self, state: &Arc<AppState>) {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return };
        for d in rd.flatten() {
            let id = d.file_name().to_string_lossy().into_owned();
            if !safe_id(&id) || !d.path().join("profiles.json").exists() {
                continue;
            }
            let saved: Saved = ttv_reader::storage::read_json(&d.path().join("hosted.json")).unwrap_or_default();
            let e = self.make(state, &id, saved);
            self.entries.write().insert(id, e);
        }
        let n = self.entries.read().len();
        if n > 0 {
            log::info!("OBS links: {n} profiles to read");
        }
    }

    fn make(&self, state: &Arc<AppState>, id: &str, saved: Saved) -> Arc<Entry> {
        let dir = self.dir.join(id);
        let shared = Arc::new(Shared { id: id.to_string(), sink: ObsSink::new(), saved: RwLock::new(saved) });
        let http = reqwest::Client::builder().user_agent(concat!("TTV-TTS-Proxy/", env!("CARGO_PKG_VERSION"))).build().unwrap_or_default();
        let host = ProxyHost { state: Arc::downgrade(state), shared: shared.clone(), translator: Translator::new(http) };
        let reader = Arc::new(Reader::new(host, &dir));
        // reads only while somebody listens
        reader.store.write().active.clear();
        reader.clone().start();
        Arc::new(Entry { reader, shared, dir, idle: Mutex::new(Some(Instant::now())) })
    }

    fn get(&self, id: &str) -> Option<Arc<Entry>> {
        self.entries.read().get(id).cloned()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }

    /// Adds or updates a hosted profile.
    pub fn put(&self, state: &Arc<AppState>, id: &str, req: HostedRequest) -> Result<HostedView, String> {
        if !safe_id(id) {
            return Err("bad profile id".into());
        }
        let mut profile = req.profile;
        profile.id = id.to_string();
        profile.output.mode = OutputMode::HostObs;
        profile.normalize();
        let saved = Saved { preset: req.preset, settings: req.settings };
        let e = match self.get(id) {
            Some(e) => {
                *e.shared.saved.write() = saved;
                e
            }
            None => {
                let e = self.make(state, id, saved);
                self.entries.write().insert(id.to_string(), e.clone());
                e
            }
        };
        let reading = e.reading();
        {
            let mut s = e.reader.store.write();
            s.profiles = vec![profile];
            s.active = if reading { vec![id.to_string()] } else { Vec::new() };
        }
        e.reader.adopt_viewers(id, &req.roles, &req.fx, &req.plugins);
        e.reader.apply_channels();
        e.save();
        Ok(self.view(state, &e))
    }

    pub fn remove(&self, id: &str) -> bool {
        let Some(e) = self.entries.write().remove(id) else { return false };
        e.set_reading(false);
        e.shared.sink.skip_all();
        e.reader.shutdown();
        let _ = std::fs::remove_dir_all(&e.dir);
        log::info!("OBS link {id} removed");
        true
    }

    pub fn list(&self, state: &AppState) -> Vec<HostedView> {
        let mut v: Vec<HostedView> = self.entries.read().values().map(|e| self.view(state, e)).collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn one(&self, state: &AppState, id: &str) -> Option<HostedView> {
        self.get(id).map(|e| self.view(state, &e))
    }

    fn view(&self, state: &AppState, e: &Entry) -> HostedView {
        let p = e.profile();
        let c = state.cfg.read();
        let path = format!("/o/{}", p.output.token);
        let lan = state.addresses.read().iter().map(|a| format!("http://{}:{}{path}", a.ip, c.obs_port)).collect();
        drop(c);
        let base = p.output.online.then(|| crate::internet::obs_internet_base(state)).flatten();
        let securing = p.output.online && base.is_none() && crate::internet::duckdns_login(state).is_some();
        let internet_secure = base.as_ref().is_some_and(|b| b.1);
        let internet = base.map(|(b, _)| format!("{b}{path}"));
        HostedView {
            id: p.id.clone(),
            name: p.name.clone(),
            channels: p.channels.clone(),
            online: p.output.online,
            listeners: e.shared.sink.listeners(),
            reading: e.reading(),
            status: e.reader.status(),
            lan,
            internet,
            internet_secure,
            securing,
        }
    }

    /// Some OBS link is playing right now (a browser source is connected).
    pub fn listening(&self) -> bool {
        self.entries.read().values().any(|e| e.shared.sink.listening())
    }

    /// Some hosted profile's link is shared on the internet.
    pub fn any_online(&self) -> bool {
        self.entries.read().values().any(|e| e.profile().output.online)
    }

    pub fn history(&self, id: &str) -> Option<Vec<ItemView>> {
        Some(self.get(id)?.reader.history())
    }

    /// skip, clear, pause, resume, replay
    pub fn control(&self, id: &str, action: &str) -> Result<(), String> {
        let e = self.get(id).ok_or("unknown OBS link")?;
        let r = &e.reader;
        match action {
            "skip" => r.skip(),
            "clear" => r.clear("app"),
            "pause" => r.set_paused(true),
            "resume" => r.set_paused(false),
            "replay" => r.replay(None)?,
            _ => return Err("unknown action".into()),
        }
        Ok(())
    }

    /// A test message through the whole pipeline (needs a listener).
    pub fn test(&self, id: &str, user: &str, text: &str) -> Result<(), String> {
        let e = self.get(id).ok_or("unknown OBS link")?;
        let channel = e.profile().channels.first().cloned().ok_or("This profile has no channel.")?;
        if !e.reading() {
            return Err("Nobody listens: open the link in OBS first.".into());
        }
        e.reader.test_message(&channel, user, text);
        Ok(())
    }

    pub fn viewers(&self, id: &str, query: &str, limit: usize) -> Option<(Vec<ViewerView>, usize)> {
        Some(self.get(id)?.reader.viewers(id, query, limit))
    }

    pub fn set_viewer(&self, id: &str, user_id: &str, change: ViewerChange) -> Result<(), String> {
        let e = self.get(id).ok_or("unknown OBS link")?;
        let r = &e.reader;
        match change {
            ViewerChange::Role(role) => r.set_role(id, user_id, role),
            ViewerChange::Fx(fx) => r.set_fx(id, user_id, fx),
            ViewerChange::Plugin(p) => r.set_plugin(id, user_id, p),
            ViewerChange::Forget => r.forget_viewer(id, user_id),
            ViewerChange::Mute(login, on) => r.set_muted(id, &login, on),
        }
        r.save_assignments();
        Ok(())
    }

    /// Starts and stops reading with the listeners; saves viewers now and then.
    pub async fn supervise(state: Arc<AppState>) {
        let mut tick = tokio::time::interval(Duration::from_secs(3));
        let mut round = 0u32;
        loop {
            tick.tick().await;
            round += 1;
            let entries: Vec<Arc<Entry>> = state.hosted.entries.read().values().cloned().collect();
            for e in entries {
                let listening = e.shared.sink.listening();
                let reading = e.reading();
                let mut idle = e.idle.lock();
                if listening {
                    *idle = None;
                    if !reading {
                        drop(idle);
                        e.set_reading(true);
                    }
                } else {
                    let since = *idle.get_or_insert_with(Instant::now);
                    if reading && since.elapsed() >= LINGER {
                        drop(idle);
                        e.set_reading(false);
                    }
                }
                if round.is_multiple_of(20) {
                    e.reader.save_assignments();
                }
            }
        }
    }

    /// Saves pending viewer data (call on exit).
    pub fn flush(&self) {
        for e in self.entries.read().values() {
            e.reader.flush();
        }
    }
}

pub enum ViewerChange {
    Role(Option<u32>),
    Fx(Option<String>),
    Plugin(Option<String>),
    Forget,
    Mute(String, bool),
}

/// Constant-time comparison of a token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

struct ProxyLinks(Weak<AppState>);

impl Links for ProxyLinks {
    fn sink(&self, token: &str, from: IpAddr, secure: bool) -> Result<Arc<ObsSink>, Refusal> {
        let st = self.0.upgrade().ok_or(Refusal::Unknown)?;
        let sink = {
            let entries = st.hosted.entries.read();
            let e = entries.values().find(|e| e.reader.store.read().profiles.first().is_some_and(|p| same(&p.output.token, token))).ok_or(Refusal::Unknown)?;
            if !obs::is_local(from) && !e.profile().output.online {
                return Err(Refusal::NotOnline);
            }
            e.shared.sink.clone()
        };
        // from the internet the link (its token) must not travel unencrypted
        if !obs::is_local(from) && !secure && crate::internet::needs_https(&st) {
            return Err(Refusal::NeedsHttps);
        }
        Ok(sink)
    }
}

/// The port of the OBS links (HTTP, and HTTPS when switched on); listens
/// while hosted profiles exist and moves when the port setting changes.
pub async fn serve(state: Arc<AppState>) {
    let mut running: Option<((u16, bool), tokio::sync::oneshot::Sender<()>)> = None;
    let tls = state.certs.server_config();
    loop {
        let want = (!state.hosted.is_empty()).then(|| {
            let c = state.cfg.read();
            (c.obs_port, c.obs_https)
        });
        if running.as_ref().map(|(p, _)| *p) != want {
            if let Some((_, stop)) = running.take() {
                let _ = stop.send(());
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            if let Some((port, https)) = want {
                match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
                    Ok(listener) => {
                        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
                        let app = obs::router(Arc::new(ProxyLinks(Arc::downgrade(&state))));
                        log::info!("OBS links on port {port}");
                        *state.obs_error.write() = None;
                        let tls = https.then(|| tls.clone());
                        tokio::spawn(ttv_net::serve::serve(listener, app, tls, async {
                            let _ = rx.await;
                        }));
                        running = Some(((port, https), tx));
                    }
                    Err(e) => {
                        let msg = format!("port {port} can't be used for OBS links: {e}");
                        if state.obs_error.read().as_deref() != Some(msg.as_str()) {
                            log::warn!("{msg}");
                        }
                        *state.obs_error.write() = Some(msg);
                    }
                }
            }
        }
        tokio::select! {
            _ = state.hosted_changed.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(10)) => {}
        }
    }
}
