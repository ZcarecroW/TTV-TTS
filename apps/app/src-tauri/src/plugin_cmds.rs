//! Audio plugins (VST3, CLAP) on all speech: the chain is set in the
//! settings, runs on the plugin thread (crates/plugins) at the output
//! device's rate, after the voice effects. Each plugin's settings are kept
//! in the config as the plugin saves them.

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine as _;
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use ttv_plugins::{PluginInfo, SlotSpec};

use crate::config::PluginSlot;
use crate::state::AppState;

type St<'a> = tauri::State<'a, Arc<AppState>>;

fn specs(slots: &[PluginSlot]) -> Vec<SlotSpec> {
    slots
        .iter()
        .map(|s| SlotSpec { id: s.id.clone(), plugin: s.plugin.clone(), enabled: s.enabled, state: s.state.as_deref().and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok()) })
        .collect()
}

/// Loads the configured chain at the output device's rate; remembers why a
/// plugin couldn't be loaded.
pub async fn apply(state: &Arc<AppState>) {
    let slots = state.cfg.read().plugins.clone();
    let results = state.rack.set(state.audio.rate(), specs(&slots)).await;
    let mut errors = state.plugin_errors.lock();
    errors.clear();
    for (id, r) in results {
        if let Err(e) = r {
            errors.insert(id, e);
        }
    }
}

/// Keeps a plugin's settings (the plugin thread hands them over when its
/// editor closes or it leaves the chain).
pub fn keep_state(state: &AppState, id: &str, data: &[u8]) {
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    let changed = {
        let mut c = state.cfg.write();
        match c.plugins.iter_mut().find(|s| s.id == id) {
            Some(s) if s.state.as_deref() != Some(b64.as_str()) => {
                s.state = Some(b64);
                true
            }
            _ => false,
        }
    };
    if changed {
        let _ = state.save_config();
    }
}

/// The last stage before the speakers for one message: the plugin chain,
/// if any plugin is switched on.
pub struct Stage {
    on: bool,
    rate: u32,
}

impl Stage {
    pub async fn new(state: &Arc<AppState>) -> Self {
        let configured = state.cfg.read().plugins.iter().any(|s| s.enabled);
        // the device (and its rate) may have changed since the chain was loaded
        if configured && state.rack.rate() != state.audio.rate() {
            apply(state).await;
        }
        Self { on: configured && !state.rack.is_idle(), rate: state.rack.rate() }
    }

    /// Samples at `rate` → what to play, at the rate to play it at.
    pub async fn run(&self, state: &AppState, samples: Vec<f32>, rate: u32) -> (Vec<f32>, u32) {
        if !self.on {
            return (samples, rate);
        }
        let x = state.audio.resample(samples, rate, self.rate);
        (state.rack.process(x).await, self.rate)
    }

    /// The chain's ring-out after the message.
    pub async fn tail(&self, state: &AppState) -> Option<(Vec<f32>, u32)> {
        if !self.on {
            return None;
        }
        let t = state.rack.tail().await;
        (!t.is_empty()).then_some((t, self.rate))
    }
}

#[derive(Serialize)]
pub struct PluginsView {
    pub slots: Vec<PluginSlot>,
    /// slot id -> why it isn't loaded
    pub errors: HashMap<String, String>,
    /// Editors open in a window of TTV-TTS on this system.
    pub editors: bool,
}

fn plugins_view(state: &AppState) -> PluginsView {
    PluginsView { slots: state.cfg.read().plugins.clone(), errors: state.plugin_errors.lock().clone(), editors: ttv_plugins::editors_supported() }
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

/// Adds an effect at the end of the chain (switched on).
#[tauri::command]
pub async fn plugins_add(state: St<'_>, plugin: PluginInfo) -> Result<PluginsView, String> {
    let id = crate::secrets::random_key()[..10].to_ascii_lowercase();
    state.cfg.write().plugins.push(PluginSlot { id: id.clone(), plugin, enabled: true, state: None });
    let v = changed(&state).await?;
    if let Some(e) = v.errors.get(&id) {
        // a plugin that doesn't load isn't kept
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
}

/// Development builds only: one second of a tone at 24 kHz through the stage
/// speech goes through (resampling, the chain, its tail).
#[tauri::command]
pub async fn debug_plugins_probe(state: St<'_>) -> Result<Probe, String> {
    dev_only()?;
    let x: Vec<f32> = (0..24_000).map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin() * 0.8).collect();
    let peak = |v: &[f32]| v.iter().skip(v.len() / 4).take(v.len() / 2).fold(0f32, |m, s| m.max(s.abs()));
    let peak_in = peak(&x);
    let t0 = std::time::Instant::now();
    let stage = Stage::new(&state).await;
    let (out, rate) = stage.run(&state, x, 24_000).await;
    let tail = stage.tail(&state).await.map(|t| t.0.len()).unwrap_or(0);
    Ok(Probe { on: stage.on, rate, peak_in, peak_out: peak(&out), out_len: out.len(), tail_len: tail, ms: t0.elapsed().as_secs_f64() * 1000.0 })
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
