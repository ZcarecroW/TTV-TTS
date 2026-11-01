//! Window commands for presets, voice computation and the library.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use ttv_link::{EngineStatus, JobDef, JobOptions, JobRequest, JobView};

use crate::presets::{self, Parsed, Preset, PresetDetail, PresetView, PromptRequest};
use crate::speech::{self, SayReport, Utterance};
use crate::state::AppState;

type St<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub fn list_presets(state: St) -> Vec<PresetView> {
    state.library.counts(|c| state.presets.summaries(&c))
}

#[tauri::command]
pub fn get_preset(state: St, id: String) -> Result<PresetDetail, String> {
    state.library.counts(|c| state.presets.with(&id, |p| PresetDetail::new(p, &c))).ok_or_else(|| "This preset doesn't exist (any more).".into())
}

/// Creates or replaces an own preset.
#[tauri::command]
pub fn save_preset(state: St, preset: Preset) -> Result<PresetView, String> {
    let p = state.presets.put(preset)?;
    Ok(state.library.counts(|c| PresetView::new(&p, &c)))
}

/// Copies any preset (built-ins too) into a new own one.
#[tauri::command]
pub fn duplicate_preset(state: St, id: String) -> Result<PresetView, String> {
    let mut p = state.presets.get(&id).ok_or("This preset doesn't exist (any more).")?;
    p.id.clear();
    p.name = format!("{} (copy)", p.name).chars().take(60).collect();
    save_preset(state, p)
}

/// Deletes an own preset. Computed voices stay in the library (other
/// presets may share them); delete them separately if wanted.
#[tauri::command]
pub fn delete_preset(state: St, id: String) {
    state.presets.delete(&id);
}

#[tauri::command]
pub fn parse_preset(text: String) -> Parsed {
    presets::parse(&text)
}

#[tauri::command]
pub fn export_preset(state: St, id: String) -> Result<String, String> {
    state.presets.with(&id, presets::serialize).ok_or_else(|| "This preset doesn't exist (any more).".into())
}

#[tauri::command]
pub fn preset_prompt(request: PromptRequest) -> String {
    presets::prompt(&request)
}

// ── computing ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_status(state: St<'_>) -> Result<EngineStatus, String> {
    state.library.status(&state).await
}

#[tauri::command]
pub async fn estimate(state: St<'_>, voices: u32, takes: u8, check: bool) -> Result<Option<u64>, String> {
    state.library.estimate(&state, voices, takes, check).await
}

/// Designs two short samples on the voice host to measure its speed.
#[tauri::command]
pub async fn benchmark(state: St<'_>) -> Result<f32, String> {
    state.library.benchmark(&state).await
}

#[derive(Debug, Deserialize)]
pub struct ComputeRequest {
    preset_id: String,
    /// The preset's first N voices (0 = all).
    count: u32,
    languages: Vec<String>,
    #[serde(default)]
    options: JobOptions,
}

#[tauri::command]
pub async fn compute(state: St<'_>, request: ComputeRequest) -> Result<JobView, String> {
    let r = request;
    if r.languages.is_empty() {
        return Err("Pick at least one language.".into());
    }
    if let Some(bad) = r.languages.iter().find(|l| ttv_engine::langs::get(l).is_none()) {
        return Err(format!("Voices can't be designed for the language \"{bad}\" yet."));
    }
    let job = state
        .presets
        .with(&r.preset_id, |p| {
            let n = if r.count == 0 { p.defs.len() } else { (r.count as usize).min(p.defs.len()) };
            JobRequest {
                preset_id: p.id.clone(),
                preset_name: p.name.clone(),
                defs: p.defs[..n].iter().enumerate().map(|(i, d)| JobDef { index: i as u32, def: d.clone() }).collect(),
                languages: r.languages.clone(),
                options: JobOptions { takes: r.options.takes.clamp(1, 3), ..r.options.clone() },
                ..Default::default()
            }
        })
        .ok_or("This preset doesn't exist (any more).")?;
    state.library.submit(&state, job).await
}

/// pause, resume, cancel or remove
#[tauri::command]
pub async fn job_control(state: St<'_>, id: String, action: String) -> Result<(), String> {
    state.library.control(&state, &id, &action).await
}

/// Designs one computed voice again: deletes it (in VCP too) and queues a
/// job for just this voice and language. For voices that came out different
/// from their description.
#[tauri::command]
pub async fn redo_voice(state: St<'_>, preset_id: String, index: u32, lang: String) -> Result<JobView, String> {
    if ttv_engine::langs::get(&lang).is_none() {
        return Err(format!("Voices can't be designed for the language \"{lang}\" yet."));
    }
    let (id, name, def) = state
        .presets
        .with(&preset_id, |p| p.defs.get(index as usize).map(|d| (p.id.clone(), p.name.clone(), d.clone())))
        .flatten()
        .ok_or("This voice doesn't exist (any more).")?;
    state.library.delete(&state, vec![def.key(&lang)]).await?;
    // with the quality check: the new design gets a report right away
    let options = JobOptions { quality: true, ..JobOptions::default() };
    let job = JobRequest { preset_id: id, preset_name: name, defs: vec![JobDef { index, def }], languages: vec![lang], options, ..Default::default() };
    state.library.submit(&state, job).await
}

#[tauri::command]
pub async fn delete_voices(state: St<'_>, keys: Vec<String>) -> Result<u32, String> {
    state.library.delete(&state, keys).await
}

/// Removes library entries whose VCP voice was deleted in VCP.
#[tauri::command]
pub async fn verify_library(state: St<'_>) -> Result<ttv_link::Verified, String> {
    state.library.verify(&state).await
}

/// Speaks `text` with a computed voice of a preset.
#[tauri::command]
pub async fn listen(state: St<'_>, preset_id: String, index: u32, lang: String, text: String) -> Result<SayReport, String> {
    let vcp = state.vcp().ok_or("No voice host set up yet.")?;
    let key = state.presets.with(&preset_id, |p| p.defs.get(index as usize).map(|d| d.key(&lang))).flatten().ok_or("This voice doesn't exist (any more).")?;
    let e = state.library.get(&key).ok_or("This voice isn't computed for this language yet.")?;
    let language = ttv_engine::langs::get(&lang).map(|l| l.vcp).unwrap_or("Auto").to_string();
    let text = if text.trim().is_empty() { ttv_engine::langs::get(&lang).map(|l| l.verify.to_string()).unwrap_or_default() } else { text };
    speech::say(vcp, state.audio.clone(), Utterance { text, voice: e.voice_id, language, emotion: None, intensity: None, gain: e.gain }).await
}

// ── voices from TTV-TTS 1.x ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ClassicResult {
    pub roles: u32,
    pub adopted: u32,
}

#[tauri::command]
pub async fn detect_classic(state: St<'_>) -> Result<crate::classic::Found, String> {
    let vcp = state.vcp().ok_or("No voice host set up yet.")?;
    let voices = vcp.voices().await.map_err(|e| e.to_string())?;
    Ok(crate::classic::detect(&voices))
}

/// Turns the 1.x voices into the own preset "Classic N" and adds them to
/// the library as computed (nothing is generated).
#[tauri::command]
pub async fn adopt_classic(state: St<'_>) -> Result<ClassicResult, String> {
    let vcp = state.vcp().ok_or("No voice host set up yet.")?;
    let voices = vcp.voices().await.map_err(|e| e.to_string())?;
    let (preset, entries) = crate::classic::build(&voices).ok_or("No voices from TTV-TTS 1.x found (names like \"TTS-de 01 Lena\").")?;
    let roles = preset.defs.len() as u32;
    state.presets.put(preset)?;
    let adopted = state.library.adopt(&state, entries).await?;
    log::info!("classic: {roles} roles, {adopted} voices adopted");
    Ok(ClassicResult { roles, adopted })
}

/// Rebuilds a Classic preset adopted by 2.0.0-alpha.8 or older (spoken words
/// as descriptions, no genders) once the library is loaded. Moves the library
/// entries to the new definitions; nothing changes in VCP.
pub async fn migrate_classic(state: &Arc<AppState>) {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let Some(old) = state.presets.get(crate::classic::ID).filter(crate::classic::outdated) else { return };
    if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let Some(vcp) = state.vcp() else { return DONE.store(false, std::sync::atomic::Ordering::SeqCst) };
    let voices = match vcp.voices().await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("classic: can't list VCP's voices: {e}");
            return DONE.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    };
    let entries: Vec<ttv_link::LibraryEntry> = state.library.with(|map, _| map.values().filter(|e| e.preset_id == crate::classic::ID).cloned().collect());
    let Some((preset, moved, remove)) = crate::classic::migrate(&old, &voices, &entries) else { return };
    // an older proxy can't re-key: then nothing changes and it's tried again next time
    match state.library.rekey(state, moved, remove).await {
        Ok(n) => {
            if let Err(e) = state.presets.put(preset) {
                log::warn!("classic: {e}");
            }
            log::info!("classic: rebuilt the definitions from the role names, {n} voices moved");
        }
        Err(e) => {
            log::info!("classic: not rebuilt yet ({e})");
            DONE.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
