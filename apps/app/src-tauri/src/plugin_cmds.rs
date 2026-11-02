//! Audio plugins (VST3, CLAP) on speech, set up in the settings:
//! - the chain on every voice ("main"),
//! - plugin effects viewers get at random ("v:<id>", one plugin each with
//!   its own settings; which viewers, per profile).
//!
//! They run on the plugin thread (crates/plugins) after the built-in voice
//! effects: the viewer's effect, then the chain on every voice. With TTV-TTS
//! Proxy and "effects on the voice host PC", they run there instead when it
//! has every plugin a message needs (the app sends its chains with their
//! settings); otherwise here, at the output device's rate. Each plugin's
//! settings are kept in the config as the plugin saves them.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use ttv_plugins::wire::{Chains, HostView};
use ttv_plugins::{PluginInfo, SlotSpec};

use crate::config::{PluginSlot, ViewerPlugin};
use crate::state::AppState;

type St<'a> = tauri::State<'a, Arc<AppState>>;

/// Rack key of a viewer effect.
pub fn variant_key(id: &str) -> String {
    format!("v:{id}")
}

fn specs(slots: &[PluginSlot]) -> Vec<SlotSpec> {
    slots.iter().map(|s| SlotSpec::from_wire(&s.id, &s.plugin, s.enabled, s.state.as_deref())).collect()
}

/// Loads the configured chains at the output device's rate; remembers why a
/// plugin couldn't be loaded. Viewer effects are loaded right away (the first
/// message of a viewer shouldn't wait for a plugin to load). Then tells the
/// voice host PC.
pub async fn apply(state: &Arc<AppState>) {
    let (slots, variants) = {
        let c = state.cfg.read();
        (c.plugins.clone(), c.viewer_plugins.clone())
    };
    let rate = state.audio.rate();
    let mut results = state.rack.set(rate, specs(&slots)).await;
    for v in &variants {
        let key = variant_key(&v.id);
        if v.enabled {
            results.extend(state.rack.set_chain(&key, rate, vec![SlotSpec::from_wire(&v.id, &v.plugin, true, v.state.as_deref())]).await);
        } else if state.rack.has_chain(&key) {
            state.rack.drop_chain(&key);
        }
    }
    {
        let mut errors = state.plugin_errors.lock();
        errors.clear();
        for (id, r) in results {
            if let Err(e) = r {
                errors.insert(id, e);
            }
        }
    }
    push_host(state).await;
}

/// The chains as the voice host PC gets them.
fn chains(state: &AppState) -> Chains {
    let c = state.cfg.read();
    Chains { main: c.plugins.clone(), variants: c.viewer_plugins.clone() }
}

/// Our own proxy (not a friend's, which runs no plugins for us).
fn own_proxy(state: &AppState) -> Option<ttv_vcp::Client> {
    matches!(state.cfg.read().host, crate::config::HostConfig::Proxy { guest: false, .. }).then(|| state.vcp()).flatten()
}

/// Sends the chains to the voice host PC (after a change, and when it
/// connects); keeps what it has.
pub async fn push_host(state: &Arc<AppState>) {
    let Some(c) = own_proxy(state) else {
        *state.host_plugins.write() = None;
        return;
    };
    let body = chains(state);
    let empty = body.main.is_empty() && body.variants.is_empty();
    // nothing to run and nothing sent yet: don't make the proxy scan its plugins
    if empty && state.host_plugins.read().is_none() {
        return;
    }
    match c.put::<_, HostView>("/ttv/v1/plugins/chains", &body, Duration::from_secs(20)).await {
        Ok(v) => *state.host_plugins.write() = Some((v, body)),
        Err(ttv_vcp::Error::Http { status: 404, .. }) => *state.host_plugins.write() = None,
        Err(e) => {
            log::info!("plugins on the voice host: {e}");
            *state.host_plugins.write() = None;
        }
    }
}

/// Whether the voice host PC can run a message's plugins: it has the
/// current chains and every plugin they need – and something is switched on.
pub fn on_host(state: &AppState, variant: Option<&str>) -> bool {
    let current = chains(state);
    let g = state.host_plugins.read();
    let Some((view, sent)) = g.as_ref() else { return false };
    if *sent != current || view.installed.is_none() || view.scanning {
        return false;
    }
    let main: Vec<&str> = current.main.iter().filter(|s| s.enabled).map(|s| s.id.as_str()).collect();
    let needs: Vec<&str> = main.iter().copied().chain(variant).collect();
    !needs.is_empty() && needs.iter().all(|id| !view.missing.iter().any(|m| m == id) && !view.errors.contains_key(*id))
}

/// Keeps a plugin's settings (the plugin thread hands them over when its
/// editor closes or it leaves the chain; the voice host PC after an edit there).
pub fn keep_state(state: &Arc<AppState>, id: &str, data: &[u8]) {
    keep_text(state, id, ttv_plugins::state_to_text(data));
}

fn keep_text(state: &Arc<AppState>, id: &str, b64: String) {
    let changed = {
        let mut c = state.cfg.write();
        let mut changed = false;
        for s in c.plugins.iter_mut().filter(|s| s.id == id && s.state.as_deref() != Some(b64.as_str())) {
            s.state = Some(b64.clone());
            changed = true;
        }
        for v in c.viewer_plugins.iter_mut().filter(|v| v.id == id && v.state.as_deref() != Some(b64.as_str())) {
            v.state = Some(b64.clone());
            changed = true;
        }
        changed
    };
    if changed {
        let _ = state.save_config();
        // the voice host PC gets the new settings too
        let st = state.clone();
        tauri::async_runtime::spawn(async move { push_host(&st).await });
    }
}

/// Settings changed in an editor on the voice host PC.
pub fn host_state(state: &Arc<AppState>, slot: &str, b64: String) {
    keep_text(state, slot, b64);
    // here too, when the plugin runs here
    let st = state.clone();
    tauri::async_runtime::spawn(async move {
        let has = { st.cfg.read().plugins.iter().any(|s| s.enabled) || st.cfg.read().viewer_plugins.iter().any(|v| v.enabled) };
        if has {
            apply(&st).await;
        }
    });
}

/// The last stage before the speakers for one message: the viewer's plugin
/// effect and the chain on every voice, if any of them is switched on here.
pub struct Stage {
    keys: Vec<String>,
    rate: u32,
}

impl Stage {
    /// `on_host`: the voice host PC ran the plugins already.
    pub async fn new(state: &Arc<AppState>, variant: Option<&str>, on_host: bool) -> Self {
        if on_host {
            return Self { keys: Vec::new(), rate: 0 };
        }
        let (main, var) = {
            let c = state.cfg.read();
            (c.plugins.iter().any(|s| s.enabled), variant.filter(|v| c.viewer_plugins.iter().any(|x| x.id == *v && x.enabled)).map(str::to_string))
        };
        // the device (and its rate) may have changed since the chains were loaded
        if (main || var.is_some()) && state.rack.rate() != state.audio.rate() {
            apply(state).await;
        }
        let mut keys = Vec::new();
        if let Some(v) = var {
            let k = variant_key(&v);
            if !state.rack.chain_idle(&k) {
                keys.push(k);
            }
        }
        if main && !state.rack.is_idle() {
            keys.push(ttv_plugins::MAIN.to_string());
        }
        Self { keys, rate: state.rack.rate() }
    }

    /// Samples at `rate` → what to play, at the rate to play it at.
    pub async fn run(&self, state: &AppState, samples: Vec<f32>, rate: u32) -> (Vec<f32>, u32) {
        if self.keys.is_empty() {
            return (samples, rate);
        }
        let x = state.audio.resample(samples, rate, self.rate);
        (state.rack.process_chains(&self.keys, x).await, self.rate)
    }

    /// What rings out after the message.
    pub async fn tail(&self, state: &AppState) -> Option<(Vec<f32>, u32)> {
        if self.keys.is_empty() {
            return None;
        }
        let t = state.rack.tail_chains(&self.keys).await;
        (!t.is_empty()).then_some((t, self.rate))
    }

    fn on(&self) -> bool {
        !self.keys.is_empty()
    }
}

#[derive(Serialize)]
pub struct PluginsView {
    pub slots: Vec<PluginSlot>,
    pub variants: Vec<ViewerPlugin>,
    /// slot id -> why it isn't loaded
    pub errors: HashMap<String, String>,
    /// Editors open in a window of TTV-TTS on this system.
    pub editors: bool,
    /// The voice host PC's plugins (with our own proxy, newer versions).
    pub host: Option<HostView>,
    /// Plugins run on the voice host PC ("effects on the voice host PC").
    pub host_runs: bool,
}

fn plugins_view(state: &AppState) -> PluginsView {
    let c = state.cfg.read();
    PluginsView {
        slots: c.plugins.clone(),
        variants: c.viewer_plugins.clone(),
        errors: state.plugin_errors.lock().clone(),
        editors: ttv_plugins::editors_supported(),
        host: state.host_plugins.read().as_ref().map(|(v, _)| v.clone()),
        host_runs: c.effects_on_host,
    }
}

#[tauri::command]
pub fn plugins_list(state: St) -> PluginsView {
    plugins_view(&state)
}

/// Every effect in the standard VST3/CLAP folders (loads each file once).
#[tauri::command]
pub async fn plugins_scan() -> Vec<PluginInfo> {
    tokio::task::spawn_blocking(ttv_plugins::scan).await.unwrap_or_default()
}

/// The effects in a plugin file the user picks (.vst3 or .clap).
#[tauri::command]
pub async fn plugins_pick(app: AppHandle) -> Result<Vec<PluginInfo>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().add_filter("VST3 / CLAP", &["vst3", "clap"]).pick_file(move |p| {
        let _ = tx.send(p);
    });
    let Some(path) = rx.await.ok().flatten().and_then(|p| p.into_path().ok()) else { return Ok(Vec::new()) };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let found = tokio::task::spawn_blocking(move || ttv_plugins::scan_file(&path)).await.map_err(|e| e.to_string())?.map_err(|e| format!("{name}: {e}"))?;
    if found.is_empty() {
        return Err(format!("{name}: no effect in this file (instruments can't be used)."));
    }
    Ok(found)
}

async fn changed(state: &Arc<AppState>) -> Result<PluginsView, String> {
    state.save_config().map_err(|e| e.to_string())?;
    apply(state).await;
    Ok(plugins_view(state))
}

/// Installed on the voice host PC only (not here)?
fn only_on_host(state: &AppState, plugin: &PluginInfo) -> bool {
    !std::path::Path::new(&plugin.path).exists()
        && state.host_plugins.read().as_ref().and_then(|(v, _)| v.installed.as_ref()).is_some_and(|l| l.iter().any(|p| p.format == plugin.format && p.id == plugin.id))
}

/// Adds an effect at the end of the chain (switched on).
#[tauri::command]
pub async fn plugins_add(state: St<'_>, plugin: PluginInfo) -> Result<PluginsView, String> {
    let id = crate::secrets::random_key()[..10].to_ascii_lowercase();
    let host_only = only_on_host(&state, &plugin);
    state.cfg.write().plugins.push(PluginSlot { id: id.clone(), plugin, enabled: true, state: None });
    let v = changed(&state).await?;
    if let Some(e) = v.errors.get(&id).filter(|_| !host_only) {
        // a plugin that doesn't load isn't kept (unless it's meant for the voice host PC)
        let e = e.clone();
        state.cfg.write().plugins.retain(|s| s.id != id);
        changed(&state).await?;
        return Err(e);
    }
    Ok(v)
}

#[tauri::command]
pub async fn plugins_remove(state: St<'_>, id: String) -> Result<PluginsView, String> {
    state.rack.close_editor(&id);
    state.cfg.write().plugins.retain(|s| s.id != id);
    changed(&state).await
}

#[tauri::command]
pub async fn plugins_toggle(state: St<'_>, id: String, enabled: bool) -> Result<PluginsView, String> {
    if let Some(s) = state.cfg.write().plugins.iter_mut().find(|s| s.id == id) {
        s.enabled = enabled;
    }
    changed(&state).await
}

/// Moves an effect up (-1) or down (+1) in the chain.
#[tauri::command]
pub async fn plugins_move(state: St<'_>, id: String, delta: i32) -> Result<PluginsView, String> {
    {
        let mut c = state.cfg.write();
        if let Some(i) = c.plugins.iter().position(|s| s.id == id) {
            let j = (i as i32 + delta).clamp(0, c.plugins.len() as i32 - 1) as usize;
            c.plugins.swap(i, j);
        }
    }
    changed(&state).await
}

/// Opens the plugin's own editor (its settings are kept when it closes).
#[tauri::command]
pub async fn plugins_edit(state: St<'_>, id: String) -> Result<(), String> {
    state.rack.open_editor(&id).await
}

/// Opens the plugin's editor on the voice host PC's screen.
#[tauri::command]
pub async fn plugins_edit_on_host(state: St<'_>, id: String) -> Result<(), String> {
    let c = own_proxy(&state).ok_or("No TTV-TTS Proxy of your own is connected.")?;
    c.post::<_, ttv_link::Ack>(&format!("/ttv/v1/plugins/edit/{id}"), &serde_json::json!({}), Duration::from_secs(20)).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Scans the voice host PC's plugins again.
#[tauri::command]
pub async fn plugins_host_scan(state: St<'_>) -> Result<PluginsView, String> {
    let c = own_proxy(&state).ok_or("No TTV-TTS Proxy of your own is connected.")?;
    let v: HostView = c.post("/ttv/v1/plugins/scan", &serde_json::json!({}), Duration::from_secs(20)).await.map_err(|e| e.to_string())?;
    let sent = chains(&state);
    *state.host_plugins.write() = Some((v, sent));
    Ok(plugins_view(&state))
}

/// The voice host PC's plugins, fresh (it scans them the first time).
#[tauri::command]
pub async fn plugins_host(state: St<'_>) -> Result<PluginsView, String> {
    let Some(c) = own_proxy(&state) else { return Ok(plugins_view(&state)) };
    match c.get::<HostView>("/ttv/v1/plugins", Duration::from_secs(20)).await {
        Ok(v) => {
            // the proxy still has older chains (it restarted, or we never sent them)
            let fresh = v.chains == chains(&state).without_states();
            let sent = chains(&state);
            *state.host_plugins.write() = Some((v, sent));
            if !fresh {
                push_host(&state).await;
            }
        }
        Err(ttv_vcp::Error::Http { status: 404, .. }) => *state.host_plugins.write() = None,
        Err(e) => return Err(e.to_string()),
    }
    Ok(plugins_view(&state))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct VariantPatch {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub weight: Option<u32>,
}

/// A new plugin effect for viewers.
#[tauri::command]
pub async fn viewer_plugins_add(state: St<'_>, plugin: PluginInfo, name: String) -> Result<PluginsView, String> {
    let id = crate::secrets::random_key()[..10].to_ascii_lowercase();
    let name = Some(name.trim()).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| plugin.name.clone());
    let host_only = only_on_host(&state, &plugin);
    {
        let mut c = state.cfg.write();
        if c.viewer_plugins.len() >= MAX_VARIANTS {
            return Err(format!("At most {MAX_VARIANTS} plugin effects for viewers."));
        }
        c.viewer_plugins.push(ViewerPlugin { id: id.clone(), name, plugin, enabled: true, weight: 1, state: None });
    }
    let v = changed(&state).await?;
    if let Some(e) = v.errors.get(&id).filter(|_| !host_only) {
        let e = e.clone();
        state.cfg.write().viewer_plugins.retain(|s| s.id != id);
        changed(&state).await?;
        return Err(e);
    }
    Ok(v)
}

/// Each viewer effect is a plugin instance in memory: a dozen is plenty.
pub const MAX_VARIANTS: usize = 12;

#[tauri::command]
pub async fn viewer_plugins_set(state: St<'_>, id: String, patch: VariantPatch) -> Result<PluginsView, String> {
    {
        let mut c = state.cfg.write();
        let v = c.viewer_plugins.iter_mut().find(|v| v.id == id).ok_or("unknown plugin effect")?;
        if let Some(n) = patch.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            v.name = n.chars().take(40).collect();
        }
        if let Some(e) = patch.enabled {
            v.enabled = e;
        }
        if let Some(w) = patch.weight {
            v.weight = w.clamp(1, 5);
        }
    }
    changed(&state).await
}

#[tauri::command]
pub async fn viewer_plugins_remove(state: St<'_>, id: String) -> Result<PluginsView, String> {
    state.rack.close_editor(&id);
    state.rack.drop_chain(&variant_key(&id));
    state.cfg.write().viewer_plugins.retain(|v| v.id != id);
    changed(&state).await
}

/// A viewer's plugin effect: an id, "none", or None (at random again).
#[tauri::command]
pub async fn set_viewer_plugin(state: St<'_>, profile: String, user_id: String, plugin: Option<String>) -> Result<(), String> {
    state.reader.set_plugin(&profile, &user_id, plugin);
    Ok(())
}

/// Keeps every plugin's current settings (also called before quitting).
pub async fn save_all(state: &Arc<AppState>) {
    for (id, data) in state.rack.save_states().await {
        keep_state(state, &id, &data);
    }
}

fn dev_only() -> Result<(), String> {
    if cfg!(debug_assertions) { Ok(()) } else { Err("only in development builds".into()) }
}

/// Development builds only: the effects in a plugin file (without the dialog).
#[tauri::command]
pub async fn debug_plugin_file(path: String) -> Result<Vec<PluginInfo>, String> {
    dev_only()?;
    tokio::task::spawn_blocking(move || ttv_plugins::scan_file(std::path::Path::new(&path))).await.map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct Probe {
    on: bool,
    rate: u32,
    peak_in: f32,
    peak_out: f32,
    out_len: usize,
    tail_len: usize,
    ms: f64,
    /// The plugins would run on the voice host PC.
    on_host: bool,
}

/// Development builds only: one second of a tone at 24 kHz through the stage
/// speech goes through (resampling, the chains, their tail), with a viewer's
/// effect if given.
#[tauri::command]
pub async fn debug_plugins_probe(state: St<'_>, variant: Option<String>) -> Result<Probe, String> {
    dev_only()?;
    let x: Vec<f32> = (0..24_000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin() * 0.8).collect();
    let peak = |v: &[f32]| v.iter().skip(v.len() / 4).take(v.len() / 2).fold(0f32, |m, s| m.max(s.abs()));
    let peak_in = peak(&x);
    let t0 = std::time::Instant::now();
    let stage = Stage::new(&state, variant.as_deref(), false).await;
    let (out, rate) = stage.run(&state, x, 24_000).await;
    let tail = stage.tail(&state).await.map(|t| t.0.len()).unwrap_or(0);
    Ok(Probe { on: stage.on(), rate, peak_in, peak_out: peak(&out), out_len: out.len(), tail_len: tail, ms: t0.elapsed().as_secs_f64() * 1000.0, on_host: on_host(&state, variant.as_deref()) })
}

/// Development builds only: how many windows the plugin's open editor shows.
#[tauri::command]
pub async fn debug_plugin_editor(state: St<'_>, id: String, close: bool) -> Result<usize, String> {
    dev_only()?;
    let n = state.rack.editor_children(&id).await;
    if close {
        state.rack.close_editor(&id);
    }
    Ok(n)
}
