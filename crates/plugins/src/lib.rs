//! Audio plugins (VST3 and CLAP effects) for the speech of TTV-TTS.
//!
//! All plugins live on one thread of their own (the plugin thread): plugins
//! expect their "main thread" calls (loading, settings, editor) and their
//! audio calls in a strict order, and editor windows need a message loop.
//! Speech is processed offline, clip by clip, in blocks: no real-time
//! deadline, so a heavy plugin only delays the next sentence a little.
//! A chain keeps its instances across clips, so reverbs and compressors
//! carry on between the sentences of a message; [`Rack::tail`] gives the
//! ring-out at its end.
//!
//! Plugins run inside the app: a faulty plugin can take it down.

mod clap;
mod resample;
mod vst3;
pub mod wire;
#[cfg(windows)]
mod win32;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use resample::Resamplers;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Vst3,
    Clap,
}

/// One effect a plugin file offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub format: Format,
    pub path: String,
    /// CLAP plugin id or VST3 class id.
    pub id: String,
    pub name: String,
    pub vendor: String,
}

/// One effect in the chain, as the app stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotSpec {
    pub id: String,
    pub plugin: PluginInfo,
    pub enabled: bool,
    /// The plugin's settings, as it saved them.
    pub state: Option<Vec<u8>>,
}

/// What the rack needs from a loaded plugin.
pub(crate) trait Effect {
    /// Mono in place, any length.
    fn process(&mut self, x: &mut [f32]);
    fn save_state(&mut self) -> Option<Vec<u8>>;
    fn load_state(&mut self, data: &[u8]) -> Result<(), String>;
    fn has_editor(&mut self) -> bool;
    fn open_editor(&mut self, title: &str) -> Result<(), String>;
    fn close_editor(&mut self);
    fn editor_open(&self) -> bool;
    /// Plugin housekeeping between clips; true when the user just closed
    /// the editor.
    fn idle(&mut self) -> bool;
    /// How long it may ring out after the input ends (seconds).
    fn tail(&self) -> f32;
    /// The editor window (to check the plugin really drew into it).
    // only Windows looks into the editor window (tests, closing)
    #[cfg_attr(not(windows), allow(dead_code))]
    fn editor_window(&self) -> Option<&Editor> {
        None
    }
}

#[cfg(windows)]
pub(crate) type Editor = win32::EditorWindow;

#[cfg(not(windows))]
pub(crate) struct Editor;

#[cfg(not(windows))]
#[allow(dead_code)] // a stub: not every platform calls every method
impl Editor {
    fn new(_title: &str, _w: i32, _h: i32) -> Result<Self, String> {
        Err("plugin editors in a host window need Windows".into())
    }
    fn show(&self) {}
    fn close_requested(&self) -> bool {
        false
    }
    fn set_client_size(&self, _w: i32, _h: i32) {}
    fn raw(&self) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
}

fn clap_dirs() -> Vec<std::path::PathBuf> {
    let mut v: Vec<std::path::PathBuf> = std::env::var_os("CLAP_PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    #[cfg(windows)]
    {
        if let Some(c) = std::env::var_os("COMMONPROGRAMFILES") {
            v.push(std::path::Path::new(&c).join("CLAP"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            v.push(std::path::Path::new(&l).join("Programs").join("Common").join("CLAP"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        v.push("/Library/Audio/Plug-Ins/CLAP".into());
        if let Some(h) = std::env::var_os("HOME") {
            v.push(std::path::Path::new(&h).join("Library/Audio/Plug-Ins/CLAP"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(h) = std::env::var_os("HOME") {
            v.push(std::path::Path::new(&h).join(".clap"));
        }
        v.push("/usr/lib/clap".into());
    }
    v
}

fn clap_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>, depth: u32) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("clap")) {
            out.push(p);
        } else if p.is_dir() && depth < 4 {
            clap_files(&p, out, depth + 1);
        }
    }
}

/// The effects of one plugin file (.vst3 or .clap), or why it can't be used.
pub fn scan_file(path: &std::path::Path) -> Result<Vec<PluginInfo>, String> {
    match path.extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("vst3") => vst3::scan_file(path),
        Some("clap") => clap::scan_file(path),
        _ => Err("not a .vst3 or .clap file".into()),
    }
}

/// Every effect in the standard plugin folders. Loads each plugin file once
/// (that's how plugins tell what they are), so it can take a while.
pub fn scan() -> Vec<PluginInfo> {
    let mut files = vst3::plugin_files();
    for d in clap_dirs() {
        clap_files(&d, &mut files, 0);
    }
    files.sort();
    files.dedup();
    let mut out: Vec<PluginInfo> = files
        .iter()
        .flat_map(|f| {
            scan_file(f).unwrap_or_else(|e| {
                log::info!("plugin {}: {e}", f.display());
                Vec::new()
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then((a.format == Format::Vst3).cmp(&(b.format == Format::Vst3))));
    out
}

fn load(p: &PluginInfo, rate: f64, block: usize) -> Result<Box<dyn Effect>, String> {
    Ok(match p.format {
        Format::Clap => Box::new(clap::ClapEffect::load(&p.path, &p.id, rate, block)?),
        Format::Vst3 => Box::new(vst3::Vst3Effect::load(&p.path, rate, block)?),
    })
}

const BLOCK: usize = 512;

/// The chain the app's single-chain calls use.
pub const MAIN: &str = "main";

struct Loaded {
    spec: SlotSpec,
    effect: Box<dyn Effect>,
}

/// One named chain: its own plugin instances at its own rate.
struct Chain {
    rate: u32,
    slots: Vec<Loaded>,
}

type SetReply = oneshot::Sender<Vec<(String, Result<(), String>)>>;

enum Cmd {
    /// A whole chain (instances of unchanged slots are kept).
    Set(String, u32, Vec<SlotSpec>, SetReply),
    Process(String, Vec<f32>, oneshot::Sender<Vec<f32>>),
    Tail(String, oneshot::Sender<Vec<f32>>),
    /// Removes a chain (its plugins' settings go to `on_state`).
    Drop(String),
    /// Slot ids are looked up in every chain (the first match).
    Open(String, oneshot::Sender<Result<(), String>>),
    Close(String),
    Save(oneshot::Sender<Vec<(String, Vec<u8>)>>),
    Children(String, oneshot::Sender<usize>),
}

/// Called with (chain, slot id, settings) when a plugin's settings should be kept.
type OnState = Box<dyn Fn(&str, String, Vec<u8>) + Send>;

/// Effect chains on their own thread. Each chain (key) has its own plugin
/// instances, so chains never hear each other's audio: the app's chain for
/// every voice ("main"), one per viewer effect, and on the proxy one set per
/// reader. The cost of a chain is its plugins' memory; processing speech
/// offline takes a few percent of real time for typical effects.
pub struct Rack {
    tx: std::sync::mpsc::Sender<Cmd>,
    /// Enabled effects per chain (missing or 0 = nothing to do).
    active: Arc<parking_lot::Mutex<HashMap<String, (u32, u32)>>>,
}

impl Rack {
    /// Starts the plugin thread. `on_state` gets a plugin's settings when
    /// the user closes its editor or it leaves its chain (to keep them).
    pub fn start(on_state: impl Fn(String, Vec<u8>) + Send + 'static) -> Self {
        Self::start_keyed(move |_, id, data| on_state(id, data))
    }

    /// Like [`Rack::start`], with the chain the settings come from.
    pub fn start_keyed(on_state: impl Fn(&str, String, Vec<u8>) + Send + 'static) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Cmd>();
        let active = Arc::new(parking_lot::Mutex::new(HashMap::new()));
        let a = active.clone();
        let on_state: OnState = Box::new(on_state);
        std::thread::Builder::new().name("ttv-plugins".into()).spawn(move || run(rx, a, on_state)).expect("plugin thread");
        Self { tx, active }
    }

    /// Nothing in the main chain is switched on.
    pub fn is_idle(&self) -> bool {
        self.chain_idle(MAIN)
    }

    /// Nothing in this chain is switched on (or it doesn't exist).
    pub fn chain_idle(&self, chain: &str) -> bool {
        self.active.lock().get(chain).is_none_or(|(n, _)| *n == 0)
    }

    /// Whether the chain exists (loaded with `set_chain`).
    pub fn has_chain(&self, chain: &str) -> bool {
        self.active.lock().contains_key(chain)
    }

    /// The rate the main chain runs at (clips must be resampled to it).
    pub fn rate(&self) -> u32 {
        self.chain_rate(MAIN)
    }

    pub fn chain_rate(&self, chain: &str) -> u32 {
        self.active.lock().get(chain).map_or(48_000, |(_, r)| *r)
    }

    /// Loads, reorders and removes the main chain's effects to match `slots`.
    pub async fn set(&self, rate: u32, slots: Vec<SlotSpec>) -> Vec<(String, Result<(), String>)> {
        self.set_chain(MAIN, rate, slots).await
    }

    /// Loads, reorders and removes a chain's effects to match `slots`, at
    /// `rate`. Returns per slot whether it is ready.
    pub async fn set_chain(&self, chain: &str, rate: u32, slots: Vec<SlotSpec>) -> Vec<(String, Result<(), String>)> {
        let (t, r) = oneshot::channel();
        let _ = self.tx.send(Cmd::Set(chain.to_string(), rate, slots, t));
        r.await.unwrap_or_default()
    }

    /// Runs mono samples through the main chain's switched-on effects.
    pub async fn process(&self, samples: Vec<f32>) -> Vec<f32> {
        self.process_chain(MAIN, samples).await
    }

    /// Runs mono samples (at the chain's rate) through a chain.
    pub async fn process_chain(&self, chain: &str, samples: Vec<f32>) -> Vec<f32> {
        if self.chain_idle(chain) {
            return samples;
        }
        let (t, r) = oneshot::channel();
        if self.tx.send(Cmd::Process(chain.to_string(), samples, t)).is_err() {
            return Vec::new();
        }
        r.await.unwrap_or_default()
    }

    /// The ring-out of the main chain after a message.
    pub async fn tail(&self) -> Vec<f32> {
        self.tail_chain(MAIN).await
    }

    /// The ring-out of a chain after a message.
    pub async fn tail_chain(&self, chain: &str) -> Vec<f32> {
        if self.chain_idle(chain) {
            return Vec::new();
        }
        let (t, r) = oneshot::channel();
        let _ = self.tx.send(Cmd::Tail(chain.to_string(), t));
        r.await.unwrap_or_default()
    }

    /// Removes a chain and unloads its plugins.
    pub fn drop_chain(&self, chain: &str) {
        self.active.lock().remove(chain);
        let _ = self.tx.send(Cmd::Drop(chain.to_string()));
    }

    pub async fn open_editor(&self, id: &str) -> Result<(), String> {
        let (t, r) = oneshot::channel();
        let _ = self.tx.send(Cmd::Open(id.to_string(), t));
        r.await.unwrap_or_else(|_| Err("plugin thread ended".into()))
    }

    pub fn close_editor(&self, id: &str) {
        let _ = self.tx.send(Cmd::Close(id.to_string()));
    }

    /// Child windows the plugin put into its open editor window (0 = none or
    /// no editor open); a check without looking at the screen.
    pub async fn editor_children(&self, id: &str) -> usize {
        let (t, r) = oneshot::channel();
        let _ = self.tx.send(Cmd::Children(id.to_string(), t));
        r.await.unwrap_or(0)
    }

    /// The current settings of every effect in every chain (slot id; a slot
    /// in several chains comes once, from the first).
    pub async fn save_states(&self) -> Vec<(String, Vec<u8>)> {
        let (t, r) = oneshot::channel();
        let _ = self.tx.send(Cmd::Save(t));
        r.await.unwrap_or_default()
    }

    /// Runs samples through several chains one after the other (a viewer's
    /// effect, then the chain on every voice).
    pub async fn process_chains(&self, chains: &[String], mut samples: Vec<f32>) -> Vec<f32> {
        for c in chains {
            samples = self.process_chain(c, samples).await;
        }
        samples
    }

    /// The ring-out of several chains in a row: each one's tail also passes
    /// through the chains after it.
    pub async fn tail_chains(&self, chains: &[String]) -> Vec<f32> {
        let mut out = Vec::new();
        for (i, c) in chains.iter().enumerate() {
            let t = self.tail_chain(c).await;
            if t.is_empty() {
                continue;
            }
            out.extend(self.process_chains(&chains[i + 1..], t).await);
        }
        out
    }
}

impl SlotSpec {
    /// A slot as the app stores it (settings in base64).
    pub fn from_wire(id: &str, plugin: &PluginInfo, enabled: bool, state: Option<&str>) -> Self {
        use base64::Engine as _;
        Self { id: id.to_string(), plugin: plugin.clone(), enabled, state: state.and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok()) }
    }
}

/// Plugin settings as text (base64) for configs and the wire.
pub fn state_to_text(data: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn clean(x: &mut [f32]) {
    for s in x.iter_mut() {
        // a broken plugin must not blow up the speakers
        *s = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
    }
}

/// Loads, keeps and reorders a chain's slots; returns per slot whether it's ready.
fn set_slots(chain: &mut Chain, key: &str, rate: u32, slots: Vec<SlotSpec>, on_state: &OnState) -> Vec<(String, Result<(), String>)> {
    let rate_changed = rate != chain.rate;
    chain.rate = rate;
    let mut old: Vec<Loaded> = std::mem::take(&mut chain.slots);
    let mut results = Vec::new();
    for spec in slots {
        // keep a running instance of the same plugin (its sound and editor)
        let kept = (!rate_changed).then(|| old.iter().position(|l| l.spec.id == spec.id && l.spec.plugin == spec.plugin)).flatten().map(|i| old.remove(i));
        let loaded = match kept {
            Some(mut l) => {
                l.spec.enabled = spec.enabled;
                // settings changed elsewhere (the app, for the voice host PC's
                // instances) go into the running instance – not while its
                // window is open (that would undo what the user just did)
                if let Some(s) = spec.state.as_ref().filter(|s| Some(*s) != l.spec.state.as_ref())
                    && !l.effect.editor_open()
                {
                    if let Err(e) = l.effect.load_state(s) {
                        log::info!("plugin {}: new settings not loaded: {e}", spec.plugin.name);
                    }
                    l.spec.state = Some(s.clone());
                }
                Ok(l)
            }
            None => load(&spec.plugin, rate as f64, BLOCK).map(|mut effect| {
                if let Some(s) = &spec.state
                    && let Err(e) = effect.load_state(s)
                {
                    log::info!("plugin {}: settings not loaded: {e}", spec.plugin.name);
                }
                Loaded { spec: spec.clone(), effect }
            }),
        };
        match loaded {
            Ok(l) => {
                results.push((spec.id.clone(), Ok(())));
                chain.slots.push(l);
            }
            Err(e) => {
                log::warn!("plugin {}: {e}", spec.plugin.name);
                results.push((spec.id.clone(), Err(e)));
            }
        }
    }
    // removed or reloaded ones: keep their last settings
    for mut l in old {
        if let Some(s) = l.effect.save_state() {
            on_state(key, l.spec.id.clone(), s);
        }
    }
    results
}

/// What a chain still plays after its input ended, up to the longest tail
/// its plugins report (at most 5 s), without trailing silence.
fn ring_out(chain: &mut Chain) -> Vec<f32> {
    let rate = chain.rate;
    let max = chain.slots.iter().filter(|l| l.spec.enabled).map(|l| l.effect.tail()).fold(0f32, f32::max).min(5.0);
    let block = ((rate as f32 * 0.05) as usize).max(1);
    // until 0.3 s stay quiet (a delay's next echo may follow a short gap),
    // without that trailing quiet
    let enough = (rate as f32 * 0.3) as usize;
    let mut out = Vec::new();
    let mut quiet_run = 0;
    while (out.len() as f32) < max * rate as f32 && quiet_run < enough {
        let mut b = vec![0.0f32; block];
        for l in chain.slots.iter_mut().filter(|l| l.spec.enabled) {
            l.effect.process(&mut b);
        }
        clean(&mut b);
        let quiet = b.iter().map(|s| s * s).sum::<f32>() / b.len() as f32 <= 1e-8;
        quiet_run = if quiet { quiet_run + b.len() } else { 0 };
        out.extend(b);
    }
    out.truncate(out.len() - quiet_run);
    let fade = (rate as usize / 50).min(out.len());
    let n = out.len();
    for (i, s) in out[n - fade..].iter_mut().enumerate() {
        *s *= 1.0 - (i + 1) as f32 / fade as f32;
    }
    out
}

fn run(rx: std::sync::mpsc::Receiver<Cmd>, active: Arc<parking_lot::Mutex<HashMap<String, (u32, u32)>>>, on_state: OnState) {
    let mut chains: HashMap<String, Chain> = HashMap::new();
    let publish = |key: &str, c: &Chain| {
        active.lock().insert(key.to_string(), (c.slots.iter().filter(|l| l.spec.enabled).count() as u32, c.rate));
    };
    loop {
        let editing = chains.values().flat_map(|c| &c.slots).any(|l| l.effect.editor_open());
        let wait = if editing { Duration::from_millis(10) } else { Duration::from_millis(250) };
        match rx.recv_timeout(wait) {
            Ok(Cmd::Set(key, rate, slots, reply)) => {
                let chain = chains.entry(key.clone()).or_insert_with(|| Chain { rate, slots: Vec::new() });
                let results = set_slots(chain, &key, rate, slots, &on_state);
                publish(&key, chain);
                let _ = reply.send(results);
            }
            Ok(Cmd::Process(key, mut x, reply)) => {
                if let Some(c) = chains.get_mut(&key) {
                    for l in c.slots.iter_mut().filter(|l| l.spec.enabled) {
                        l.effect.process(&mut x);
                    }
                    clean(&mut x);
                }
                let _ = reply.send(x);
            }
            Ok(Cmd::Tail(key, reply)) => {
                let out = chains.get_mut(&key).map(ring_out).unwrap_or_default();
                let _ = reply.send(out);
            }
            Ok(Cmd::Drop(key)) => {
                if let Some(c) = chains.remove(&key) {
                    for mut l in c.slots {
                        l.effect.close_editor();
                        if let Some(s) = l.effect.save_state() {
                            on_state(&key, l.spec.id.clone(), s);
                        }
                    }
                }
            }
            Ok(Cmd::Open(id, reply)) => {
                let r = match chains.values_mut().flat_map(|c| c.slots.iter_mut()).find(|l| l.spec.id == id) {
                    Some(l) => {
                        if l.effect.has_editor() {
                            let title = format!("{} – TTV-TTS", l.spec.plugin.name);
                            l.effect.open_editor(&title)
                        } else {
                            Err("this plugin has no editor".into())
                        }
                    }
                    None => Err("this plugin isn't loaded".into()),
                };
                let _ = reply.send(r);
            }
            Ok(Cmd::Close(id)) => {
                for (key, c) in chains.iter_mut() {
                    if let Some(l) = c.slots.iter_mut().find(|l| l.spec.id == id && l.effect.editor_open()) {
                        l.effect.close_editor();
                        if let Some(s) = l.effect.save_state() {
                            on_state(key, id.clone(), s);
                        }
                    }
                }
            }
            Ok(Cmd::Save(reply)) => {
                let mut states: Vec<(String, Vec<u8>)> = Vec::new();
                let mut keys: Vec<&String> = chains.keys().collect();
                keys.sort_by_key(|k| (k.as_str() != MAIN, k.as_str()));
                let keys: Vec<String> = keys.into_iter().cloned().collect();
                for k in keys {
                    for l in chains.get_mut(&k).into_iter().flat_map(|c| c.slots.iter_mut()) {
                        if !states.iter().any(|(id, _)| *id == l.spec.id)
                            && let Some(s) = l.effect.save_state()
                        {
                            states.push((l.spec.id.clone(), s));
                        }
                    }
                }
                let _ = reply.send(states);
            }
            Ok(Cmd::Children(id, reply)) => {
                #[cfg(windows)]
                let n = chains.values().flat_map(|c| &c.slots).find(|l| l.spec.id == id && l.effect.editor_window().is_some()).and_then(|l| l.effect.editor_window()).map_or(0, |w| win32::child_count(w.hwnd));
                #[cfg(not(windows))]
                let n = {
                    let _ = id;
                    0
                };
                let _ = reply.send(n);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        #[cfg(windows)]
        win32::pump();
        for (key, c) in chains.iter_mut() {
            for l in &mut c.slots {
                if l.effect.idle()
                    && let Some(s) = l.effect.save_state()
                {
                    on_state(key, l.spec.id.clone(), s);
                }
            }
        }
    }
}

/// Whether editors can open in a host window on this system.
pub fn editors_supported() -> bool {
    cfg!(windows)
}
