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
mod vst3;
#[cfg(windows)]
mod win32;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
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

struct Loaded {
    spec: SlotSpec,
    effect: Box<dyn Effect>,
}

type SetReply = oneshot::Sender<Vec<(String, Result<(), String>)>>;

enum Cmd {
    /// The whole chain (instances of unchanged slots are kept).
    Set(u32, Vec<SlotSpec>, SetReply),
    Process(Vec<f32>, oneshot::Sender<Vec<f32>>),
    Tail(oneshot::Sender<Vec<f32>>),
    Open(String, oneshot::Sender<Result<(), String>>),
    Close(String),
    Save(oneshot::Sender<Vec<(String, Vec<u8>)>>),
    Children(String, oneshot::Sender<usize>),
}

/// The effect chain on its own thread.
pub struct Rack {
    tx: std::sync::mpsc::Sender<Cmd>,
    /// Enabled effects in the chain (0 = nothing to do).
    active: Arc<AtomicU32>,
    rate: Arc<AtomicU32>,
}

impl Rack {
    /// Starts the plugin thread. `on_state` gets a plugin's settings when
    /// the user closes its editor or it leaves the chain (to keep them).
    pub fn start(on_state: impl Fn(String, Vec<u8>) + Send + 'static) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Cmd>();
        let active = Arc::new(AtomicU32::new(0));
        let rate = Arc::new(AtomicU32::new(48_000));
        let a = active.clone();
        std::thread::Builder::new().name("ttv-plugins".into()).spawn(move || run(rx, a, on_state)).expect("plugin thread");
        Self { tx, active, rate }
    }

    /// Nothing in the chain is switched on.
    pub fn is_idle(&self) -> bool {
        self.active.load(Ordering::Relaxed) == 0
    }

    /// The rate the chain runs at (clips must be resampled to it).
    pub fn rate(&self) -> u32 {
        self.rate.load(Ordering::Relaxed)
    }

    /// Loads, reorders and removes effects to match `slots`, at `rate`.
    /// Returns per slot whether it is ready.
    pub async fn set(&self, rate: u32, slots: Vec<SlotSpec>) -> Vec<(String, Result<(), String>)> {
        self.rate.store(rate, Ordering::Relaxed);
        let (t, r) = oneshot::channel();
        let _ = self.tx.send(Cmd::Set(rate, slots, t));
        r.await.unwrap_or_default()
    }

    /// Runs mono samples through the switched-on effects.
    pub async fn process(&self, samples: Vec<f32>) -> Vec<f32> {
        if self.is_idle() {
            return samples;
        }
        let (t, r) = oneshot::channel();
        if self.tx.send(Cmd::Process(samples, t)).is_err() {
            return Vec::new();
        }
        r.await.unwrap_or_default()
    }

    /// The ring-out of the chain after a message.
    pub async fn tail(&self) -> Vec<f32> {
        if self.is_idle() {
            return Vec::new();
        }
        let (t, r) = oneshot::channel();
        let _ = self.tx.send(Cmd::Tail(t));
        r.await.unwrap_or_default()
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

    /// The current settings of every effect (to store them).
    pub async fn save_states(&self) -> Vec<(String, Vec<u8>)> {
        let (t, r) = oneshot::channel();
        let _ = self.tx.send(Cmd::Save(t));
        r.await.unwrap_or_default()
    }
}

fn run(rx: std::sync::mpsc::Receiver<Cmd>, active: Arc<AtomicU32>, on_state: impl Fn(String, Vec<u8>)) {
    let mut chain: Vec<Loaded> = Vec::new();
    let mut rate = 48_000u32;
    loop {
        let wait = if chain.iter().any(|l| l.effect.editor_open()) { Duration::from_millis(10) } else { Duration::from_millis(250) };
        match rx.recv_timeout(wait) {
            Ok(Cmd::Set(r, slots, reply)) => {
                let rate_changed = r != rate;
                rate = r;
                let mut old: Vec<Loaded> = std::mem::take(&mut chain);
                let mut results = Vec::new();
                for spec in slots {
                    // keep a running instance of the same plugin (its sound and editor)
                    let kept = (!rate_changed).then(|| old.iter().position(|l| l.spec.id == spec.id && l.spec.plugin == spec.plugin)).flatten().map(|i| old.remove(i));
                    let loaded = match kept {
                        Some(mut l) => {
                            l.spec.enabled = spec.enabled;
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
                            chain.push(l);
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
                        on_state(l.spec.id.clone(), s);
                    }
                }
                active.store(chain.iter().filter(|l| l.spec.enabled).count() as u32, Ordering::Relaxed);
                let _ = reply.send(results);
            }
            Ok(Cmd::Process(mut x, reply)) => {
                for l in chain.iter_mut().filter(|l| l.spec.enabled) {
                    l.effect.process(&mut x);
                }
                for s in x.iter_mut() {
                    // a broken plugin must not blow up the speakers
                    *s = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
                }
                let _ = reply.send(x);
            }
            Ok(Cmd::Tail(reply)) => {
                let max = chain.iter().filter(|l| l.spec.enabled).map(|l| l.effect.tail()).fold(0f32, f32::max).min(5.0);
                let block = ((rate as f32 * 0.05) as usize).max(1);
                // until 0.3 s stay quiet (a delay's next echo may follow a
                // short gap), without that trailing quiet
                let enough = (rate as f32 * 0.3) as usize;
                let mut out = Vec::new();
                let mut quiet_run = 0;
                while (out.len() as f32) < max * rate as f32 && quiet_run < enough {
                    let mut b = vec![0.0f32; block];
                    for l in chain.iter_mut().filter(|l| l.spec.enabled) {
                        l.effect.process(&mut b);
                    }
                    let quiet = b.iter().map(|s| s * s).sum::<f32>() / b.len() as f32 <= 1e-8;
                    quiet_run = if quiet { quiet_run + b.len() } else { 0 };
                    out.extend(b.iter().map(|s| if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 }));
                }
                out.truncate(out.len() - quiet_run);
                let fade = (rate as usize / 50).min(out.len());
                let n = out.len();
                for (i, s) in out[n - fade..].iter_mut().enumerate() {
                    *s *= 1.0 - (i + 1) as f32 / fade as f32;
                }
                let _ = reply.send(out);
            }
            Ok(Cmd::Open(id, reply)) => {
                let r = match chain.iter_mut().find(|l| l.spec.id == id) {
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
                if let Some(l) = chain.iter_mut().find(|l| l.spec.id == id) {
                    l.effect.close_editor();
                    if let Some(s) = l.effect.save_state() {
                        on_state(id, s);
                    }
                }
            }
            Ok(Cmd::Save(reply)) => {
                let states = chain.iter_mut().filter_map(|l| l.effect.save_state().map(|s| (l.spec.id.clone(), s))).collect();
                let _ = reply.send(states);
            }
            Ok(Cmd::Children(id, reply)) => {
                #[cfg(windows)]
                let n = chain.iter().find(|l| l.spec.id == id).and_then(|l| l.effect.editor_window()).map_or(0, |w| win32::child_count(w.hwnd));
                #[cfg(not(windows))]
                let n = { let _ = id; 0 };
                let _ = reply.send(n);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        #[cfg(windows)]
        win32::pump();
        for l in &mut chain {
            if l.effect.idle()
                && let Some(s) = l.effect.save_state()
            {
                on_state(l.spec.id.clone(), s);
            }
        }
    }
}

/// Whether editors can open in a host window on this system.
pub fn editors_supported() -> bool {
    cfg!(windows)
}
