//! Audio plugins (VST3, CLAP) on the voice host PC: the app sends its chains
//! (the chain on every voice and the viewers' plugin effects, with their
//! settings); this PC runs them on the speech it makes, so the gaming PC
//! only plays. Plugins must be installed here too – matched by their id, so
//! another folder or version of the same plugin works.
//!
//! - Readers don't share instances: speech for the app ("speak") and each
//!   profile read here for OBS ("h:<id>") get their own, made on first use
//!   and unloaded after 10 idle minutes. A reverb's tail never leaks into
//!   another stream.
//! - Everything runs at 48 kHz (plugins expect normal rates); the speech is
//!   resampled once before.
//! - The plugin thread starts only when a plugin is used, and plugin files
//!   are scanned only when asked (scanning loads every plugin once).
//! - Plugins run inside the proxy: a faulty plugin can take it down.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use ttv_plugins::wire::{Chains, HostView};
use ttv_plugins::{PluginInfo, Rack, Resamplers, SlotSpec};

use crate::state::AppState;

/// Plugins run at this rate.
pub const RATE: u32 = 48_000;
/// Chains of a reader that spoke nothing for this long are unloaded.
const IDLE: Duration = Duration::from_secs(600);

/// Settings a plugin saved: (chain, slot id, data).
type Change = (String, String, Vec<u8>);

pub struct ProxyPlugins {
    dir: PathBuf,
    rack: OnceLock<Rack>,
    installed: RwLock<Option<Vec<PluginInfo>>>,
    scanning: AtomicBool,
    chains: RwLock<Chains>,
    /// Loaded chains (rack key) -> last use.
    loaded: Mutex<HashMap<String, Instant>>,
    errors: Mutex<HashMap<String, String>>,
    resamplers: Resamplers,
    /// Settings that changed here (an editor on this PC): (chain, slot id, data).
    changed: tokio::sync::mpsc::UnboundedSender<Change>,
    changes: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Change>>>,
}

impl ProxyPlugins {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let installed = crate::storage::read_json::<Vec<PluginInfo>>(&data_dir.join("plugins-installed.json"));
        let chains = crate::storage::read_json::<Chains>(&data_dir.join("plugin-chains.json")).unwrap_or_default();
        let (changed, changes) = tokio::sync::mpsc::unbounded_channel();
        Self {
            dir: data_dir.to_path_buf(),
            rack: OnceLock::new(),
            installed: RwLock::new(installed),
            scanning: AtomicBool::new(false),
            chains: RwLock::new(chains),
            loaded: Mutex::new(HashMap::new()),
            errors: Mutex::new(HashMap::new()),
            resamplers: Resamplers::default(),
            changed,
            changes: Mutex::new(Some(changes)),
        }
    }

    fn rack(&self) -> &Rack {
        self.rack.get_or_init(|| {
            let tx = self.changed.clone();
            Rack::start_keyed(move |chain, id, data| {
                let _ = tx.send((chain.to_string(), id, data));
            })
        })
    }

    /// What this PC has and what the app's chains need.
    pub fn view(&self) -> HostView {
        let chains = self.chains.read().clone();
        let installed = self.installed.read().clone();
        let missing = match &installed {
            Some(list) => chains.main.iter().map(|s| (&s.id, &s.plugin)).chain(chains.variants.iter().map(|v| (&v.id, &v.plugin))).filter(|(_, p)| resolve(list, p).is_none()).map(|(id, _)| id.clone()).collect(),
            None => chains.main.iter().map(|s| s.id.clone()).chain(chains.variants.iter().map(|v| v.id.clone())).collect(),
        };
        HostView { installed, scanning: self.scanning.load(Ordering::Relaxed), missing, errors: self.errors.lock().clone(), chains: chains.without_states(), editors: ttv_plugins::editors_supported() }
    }

    /// Scans the standard plugin folders (in the background; loads each plugin once).
    pub fn scan(self: &Arc<Self>) {
        if self.scanning.swap(true, Ordering::Relaxed) {
            return;
        }
        let me = self.clone();
        tokio::spawn(async move {
            let t0 = Instant::now();
            let list = tokio::task::spawn_blocking(ttv_plugins::scan).await.unwrap_or_default();
            log::info!("plugins: {} effects found in {} ms", list.len(), t0.elapsed().as_millis());
            let _ = crate::storage::write_json(&me.dir.join("plugins-installed.json"), &list);
            *me.installed.write() = Some(list);
            me.scanning.store(false, Ordering::Relaxed);
            // chains loaded before the scan may find their plugins now
            me.reload().await;
        });
    }

    /// The app's new chains: kept, and loaded chains follow them.
    pub async fn set_chains(&self, chains: Chains) {
        let _ = crate::storage::write_json(&self.dir.join("plugin-chains.json"), &chains);
        *self.chains.write() = chains;
        self.reload().await;
    }

    async fn reload(&self) {
        let keys: Vec<String> = self.loaded.lock().keys().cloned().collect();
        for key in keys {
            match self.specs_for(&key) {
                Some(specs) if !specs.is_empty() => self.load(&key, specs).await,
                _ => {
                    self.loaded.lock().remove(&key);
                    if let Some(r) = self.rack.get() {
                        r.drop_chain(&key);
                    }
                }
            }
        }
    }

    /// The slots of a rack key ("<lane>/main" or "<lane>/v:<id>"), with this
    /// PC's plugin files.
    fn specs_for(&self, key: &str) -> Option<Vec<SlotSpec>> {
        let (_, what) = key.split_once('/')?;
        let chains = self.chains.read();
        let installed = self.installed.read();
        let local = |p: &PluginInfo| match installed.as_deref() {
            Some(list) => resolve(list, p),
            // not scanned: the same file as on the gaming PC, if it's here
            None => std::path::Path::new(&p.path).exists().then(|| p.clone()),
        };
        if what == "main" {
            return Some(chains.main.iter().filter_map(|s| Some(SlotSpec::from_wire(&s.id, &local(&s.plugin)?, s.enabled, s.state.as_deref()))).collect());
        }
        let id = what.strip_prefix("v:")?;
        let v = chains.variants.iter().find(|v| v.id == id && v.enabled)?;
        Some(local(&v.plugin).map(|p| vec![SlotSpec::from_wire(&v.id, &p, true, v.state.as_deref())]).unwrap_or_default())
    }

    async fn load(&self, key: &str, specs: Vec<SlotSpec>) {
        let results = self.rack().set_chain(key, RATE, specs).await;
        let mut errors = self.errors.lock();
        for (id, r) in results {
            match r {
                Ok(()) => {
                    errors.remove(&id);
                }
                Err(e) => {
                    errors.insert(id, e);
                }
            }
        }
        self.loaded.lock().insert(key.to_string(), Instant::now());
    }

    /// The rack keys a message of `lane` runs through (loaded on first use).
    async fn keys(&self, lane: &str, variant: Option<&str>, main: bool) -> Vec<String> {
        let mut keys = Vec::new();
        if let Some(v) = variant {
            keys.push(format!("{lane}/v:{v}"));
        }
        if main {
            keys.push(format!("{lane}/main"));
        }
        let mut out = Vec::new();
        for key in keys {
            let known = self.loaded.lock().get_mut(&key).map(|t| *t = Instant::now()).is_some();
            if !known {
                match self.specs_for(&key) {
                    Some(specs) if !specs.is_empty() => self.load(&key, specs).await,
                    _ => continue,
                }
            }
            if !self.rack().chain_idle(&key) {
                out.push(key);
            }
        }
        out
    }

    /// Whether anything would run for these (to skip resampling).
    pub fn has_work(&self, variant: Option<&str>, main: bool) -> bool {
        let c = self.chains.read();
        (main && c.main.iter().any(|s| s.enabled)) || variant.is_some_and(|v| c.variants.iter().any(|x| x.id == v && x.enabled))
    }

    /// Runs speech of `lane` through the viewer's effect and the chain on
    /// every voice; returns the audio at the plugins' rate.
    pub async fn run(&self, lane: &str, variant: Option<&str>, main: bool, samples: Vec<f32>, rate: u32) -> (Vec<f32>, u32) {
        if !self.has_work(variant, main) {
            return (samples, rate);
        }
        let keys = self.keys(lane, variant, main).await;
        if keys.is_empty() {
            return (samples, rate);
        }
        let x = self.resamplers.resample(samples, rate, RATE);
        (self.rack().process_chains(&keys, x).await, RATE)
    }

    /// What still rings out after a message (at the plugins' rate).
    pub async fn tail(&self, lane: &str, variant: Option<&str>, main: bool) -> Vec<f32> {
        if !self.has_work(variant, main) {
            return Vec::new();
        }
        let keys = self.keys(lane, variant, main).await;
        self.rack().tail_chains(&keys).await
    }

    /// Opens a plugin's editor on this PC's screen.
    pub async fn edit(&self, slot: &str) -> Result<(), String> {
        let key = {
            let c = self.chains.read();
            if c.main.iter().any(|s| s.id == slot) {
                "edit/main".to_string()
            } else if c.variants.iter().any(|v| v.id == slot) {
                format!("edit/v:{slot}")
            } else {
                return Err("this plugin isn't in the app's chains".into());
            }
        };
        let specs = self.specs_for(&key).unwrap_or_default();
        if !specs.iter().any(|s| s.id == slot) {
            return Err("this plugin isn't installed on the voice host PC".into());
        }
        // the editor's instance is the only one that may change settings
        let specs = specs.into_iter().map(|mut s| {
            s.enabled = s.id == slot;
            s
        });
        self.load(&key, specs.collect()).await;
        self.rack().open_editor(slot).await
    }

    /// Settings changed on this PC: kept here and handed to the app.
    fn keep(&self, chain: &str, slot: &str, data: &[u8]) -> bool {
        if !chain.starts_with("edit/") {
            return false;
        }
        let b64 = ttv_plugins::state_to_text(data);
        let mut changed = false;
        {
            let mut c = self.chains.write();
            for s in c.main.iter_mut().filter(|s| s.id == slot) {
                changed |= s.state.as_deref() != Some(b64.as_str());
                s.state = Some(b64.clone());
            }
            for v in c.variants.iter_mut().filter(|v| v.id == slot) {
                changed |= v.state.as_deref() != Some(b64.as_str());
                v.state = Some(b64.clone());
            }
        }
        if changed {
            let _ = crate::storage::write_json(&self.dir.join("plugin-chains.json"), &*self.chains.read());
        }
        changed
    }

    /// Unloads the chains of readers that went quiet.
    fn evict(&self) {
        let Some(rack) = self.rack.get() else { return };
        let mut loaded = self.loaded.lock();
        loaded.retain(|key, used| {
            // an editor's chain stays while it may be open
            let keep = key.starts_with("edit/") || used.elapsed() < IDLE;
            if !keep {
                rack.drop_chain(key);
            }
            keep
        });
    }
}

/// This PC's file of a plugin: same format and id (another folder or
/// version of the same plugin is fine).
fn resolve(installed: &[PluginInfo], p: &PluginInfo) -> Option<PluginInfo> {
    installed.iter().find(|i| i.format == p.format && i.id == p.id).cloned()
}

/// Hands settings changed in an editor here to the app, and unloads idle chains.
pub async fn run(state: Arc<AppState>) {
    let Some(mut rx) = state.plugins.changes.lock().take() else { return };
    let weak: Weak<AppState> = Arc::downgrade(&state);
    drop(state);
    let mut tick = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            m = rx.recv() => {
                let Some((chain, slot, data)) = m else { return };
                let Some(st) = weak.upgrade() else { return };
                if st.plugins.keep(&chain, &slot, &data) {
                    log::info!("plugin settings changed here: {slot}");
                    let _ = st.events.send(ttv_link::Event::PluginState { slot, state: ttv_plugins::state_to_text(&data) });
                    // readers take the new settings
                    st.plugins.reload().await;
                }
            }
            _ = tick.tick() => {
                if let Some(st) = weak.upgrade() {
                    st.plugins.evict();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ttv_plugins::Format;

    fn info(id: &str, path: &str) -> PluginInfo {
        PluginInfo { format: Format::Clap, path: path.into(), id: id.into(), name: id.into(), vendor: "x".into() }
    }

    #[test]
    fn plugins_are_found_by_id_not_path() {
        let installed = vec![info("com.a.reverb", "D:/CLAP/Reverb.clap")];
        assert_eq!(resolve(&installed, &info("com.a.reverb", "C:/Program Files/Common Files/CLAP/Reverb.clap")).unwrap().path, "D:/CLAP/Reverb.clap");
        assert!(resolve(&installed, &info("com.b.delay", "C:/x.clap")).is_none());
        let mut vst = info("com.a.reverb", "x");
        vst.format = Format::Vst3;
        assert!(resolve(&installed, &vst).is_none(), "the format must match too");
    }

    #[test]
    fn missing_plugins_are_reported() {
        let dir = std::env::temp_dir().join(format!("ttv-proxy-plugins-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = ProxyPlugins::new(&dir);
        *p.chains.write() = Chains {
            main: vec![ttv_plugins::wire::Slot { id: "m1".into(), plugin: info("com.a.reverb", "C:/r.clap"), enabled: true, state: Some("AAAA".into()) }],
            variants: vec![ttv_plugins::wire::Variant { id: "v1".into(), name: "Chipmunk".into(), plugin: info("com.c.pitch", "C:/p.clap"), enabled: true, weight: 1, state: None }],
        };
        assert_eq!(p.view().missing, vec!["m1", "v1"], "not scanned: everything counts as missing");
        *p.installed.write() = Some(vec![info("com.a.reverb", "D:/r.clap")]);
        let v = p.view();
        assert_eq!(v.missing, vec!["v1"]);
        assert!(v.chains.main[0].state.is_none(), "views carry no settings");
        assert!(p.has_work(None, true) && p.has_work(Some("v1"), false) && !p.has_work(Some("v2"), false));
        let main = p.specs_for("speak/main").unwrap();
        assert_eq!(main[0].plugin.path, "D:/r.clap");
        assert_eq!(main[0].state.as_deref(), Some(&[0u8, 0, 0][..]));
        assert_eq!(p.specs_for("speak/v:v1").unwrap().len(), 0, "not installed here");
        // settings from an editor here are kept for the app
        assert!(!p.keep("speak/main", "m1", &[1, 2, 3]), "only the editor's instance counts");
        assert!(p.keep("edit/main", "m1", &[1, 2, 3]));
        assert_eq!(p.chains.read().main[0].state.as_deref(), Some("AQID"));
        assert!(!p.keep("edit/main", "m1", &[1, 2, 3]), "unchanged");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The test CLAP plugin built by `cargo test -p ttv-plugins` (a gain kept
    /// in its settings), as a file with the .clap extension.
    fn test_clap() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?.parent()?.join("examples");
        let lib = if cfg!(windows) { "test_clap.dll" } else if cfg!(target_os = "macos") { "libtest_clap.dylib" } else { "libtest_clap.so" };
        let to = std::env::temp_dir().join(format!("ttv-proxy-test-{}-{}.clap", std::process::id(), rand::random::<u32>()));
        std::fs::copy(dir.join(lib), &to).ok()?;
        Some(to)
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().skip(x.len() / 4).take(x.len() / 2).fold(0f32, |m, s| m.max(s.abs()))
    }

    /// Real plugins: the viewer's effect, then the chain on every voice, at
    /// 48 kHz, in separate instances per reader; new settings from the app
    /// reach loaded chains.
    #[tokio::test(flavor = "multi_thread")]
    async fn runs_the_apps_chains_on_speech() {
        let Some(path) = test_clap() else {
            eprintln!("test plugin not built (run cargo test -p ttv-plugins first) - skipped");
            return;
        };
        let info = ttv_plugins::scan_file(&path).unwrap().remove(0);
        let gain = |g: f32| Some(ttv_plugins::state_to_text(&g.to_bits().to_le_bytes()));
        let dir = std::env::temp_dir().join(format!("ttv-proxy-plugins-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = ProxyPlugins::new(&dir);
        *p.installed.write() = Some(vec![info.clone()]);
        p.set_chains(Chains {
            main: vec![ttv_plugins::wire::Slot { id: "m1".into(), plugin: info.clone(), enabled: true, state: gain(0.5) }],
            variants: vec![ttv_plugins::wire::Variant { id: "v1".into(), name: "Quiet".into(), plugin: info.clone(), enabled: true, weight: 1, state: gain(0.25) }],
        })
        .await;
        let x: Vec<f32> = (0..24_000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin() * 0.8).collect();
        let t0 = Instant::now();
        let (out, rate) = p.run("speak", None, true, x.clone(), 24_000).await;
        println!("first piece (loads the plugin): {} ms", t0.elapsed().as_millis());
        assert_eq!(rate, RATE);
        assert!((out.len() as i64 - 48_000).abs() < 64, "{}", out.len());
        assert!((peak(&out) - 0.4).abs() < 0.02, "main 0.5: {}", peak(&out));
        let t1 = Instant::now();
        let (out, _) = p.run("speak", Some("v1"), true, x.clone(), 24_000).await;
        println!("with a viewer effect: {} ms for 1 s of speech", t1.elapsed().as_millis());
        assert!((peak(&out) - 0.1).abs() < 0.01, "viewer 0.25 then main 0.5: {}", peak(&out));
        // another reader has its own instances
        let (out, _) = p.run("h:friend", Some("v1"), false, x.clone(), 24_000).await;
        assert!((peak(&out) - 0.2).abs() < 0.01, "{}", peak(&out));
        assert!(p.loaded.lock().len() >= 3, "{:?}", p.loaded.lock().keys().collect::<Vec<_>>());
        assert!(p.tail("speak", Some("v1"), true).await.is_empty(), "a gain doesn't ring out");
        // the app changes a setting: loaded chains take it
        let mut c = p.chains.read().clone();
        c.main[0].state = gain(1.0);
        p.set_chains(c).await;
        let (out, _) = p.run("speak", None, true, x.clone(), 24_000).await;
        assert!((peak(&out) - 0.8).abs() < 0.02, "new setting: {}", peak(&out));
        // switched off in the app: speech passes untouched
        let mut c = p.chains.read().clone();
        c.main[0].enabled = false;
        p.set_chains(c).await;
        let (out, r) = p.run("speak", None, true, x.clone(), 24_000).await;
        assert_eq!((out, r), (x, 24_000));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
