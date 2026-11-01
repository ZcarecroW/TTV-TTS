//! Window commands for the quality check: the list of voices that may sound
//! odd, keeping them, improving them, checking existing voices, and playing
//! a voice's original sample.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use base64::Engine as _;
use serde::Serialize;
use tauri::State;
use ttv_link::{JobDef, JobKind, JobOptions, JobRequest, JobView, LibraryEntry, Quality, VoiceDef};

use crate::library::is_odd;
use crate::state::AppState;

type St<'a> = State<'a, Arc<AppState>>;

/// A voice the quality check lists.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewItem {
    pub key: String,
    pub preset_id: String,
    pub preset_name: String,
    pub index: u32,
    pub name: String,
    pub gender: String,
    pub age: u8,
    pub description: String,
    pub lang: String,
    pub quality: Quality,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ReviewList {
    /// Odd voices, oddest first.
    pub items: Vec<ReviewItem>,
    /// Computed voices with a quality report / without one.
    pub checked: u32,
    pub unchecked: u32,
    /// Odd voices someone listened to and kept.
    pub kept: u32,
}

/// Where a definition lives: (preset id, preset name, index, definition).
type Place = (String, String, u32, VoiceDef);

/// Finds the preset definitions of library entries: their own preset first
/// (cheap), else any preset containing the same definition.
fn locate(state: &AppState, entries: &[&LibraryEntry]) -> HashMap<String, Place> {
    let mut found = HashMap::new();
    let mut missing = Vec::new();
    for e in entries {
        let hit = state.presets.with(&e.preset_id, |p| p.defs.get(e.index as usize).filter(|d| d.hash() == e.def_hash).map(|d| (p.id.clone(), p.name.clone(), e.index, d.clone()))).flatten();
        match hit {
            Some(place) => {
                found.insert(e.def_hash.clone(), place);
            }
            None => missing.push(e.def_hash.clone()),
        }
    }
    if !missing.is_empty() {
        let wanted: BTreeSet<String> = missing.into_iter().collect();
        state.presets.each(|p| {
            for (i, d) in p.defs.iter().enumerate() {
                let h = d.hash();
                if wanted.contains(&h) && !found.contains_key(&h) {
                    found.insert(h, (p.id.clone(), p.name.clone(), i as u32, d.clone()));
                }
            }
        });
    }
    found
}

#[tauri::command]
pub fn quality_list(state: St) -> ReviewList {
    let (odd, checked, unchecked, kept) = state.library.with(|map, _| {
        let mut odd: Vec<LibraryEntry> = Vec::new();
        let (mut checked, mut unchecked, mut kept) = (0, 0, 0);
        for e in map.values() {
            match &e.quality {
                None => unchecked += 1,
                Some(q) => {
                    checked += 1;
                    if q.kept && q.score < ttv_engine::quality::FLAG {
                        kept += 1;
                    }
                }
            }
            if is_odd(e) {
                odd.push(e.clone());
            }
        }
        (odd, checked, unchecked, kept)
    });
    let refs: Vec<&LibraryEntry> = odd.iter().collect();
    let places = locate(&state, &refs);
    let mut items: Vec<ReviewItem> = odd
        .iter()
        .filter_map(|e| {
            let (preset_id, preset_name, index, def) = places.get(&e.def_hash)?.clone();
            Some(ReviewItem {
                key: e.key.clone(),
                preset_id,
                preset_name,
                index,
                name: def.name,
                gender: def.gender,
                age: def.age,
                description: def.description,
                lang: e.lang.clone(),
                quality: e.quality.clone().unwrap_or_default(),
            })
        })
        .collect();
    items.sort_by(|a, b| a.quality.score.total_cmp(&b.quality.score).then_with(|| a.name.cmp(&b.name)));
    ReviewList { items, checked, unchecked, kept }
}

/// Listened to it and it's fine (or not, after all).
#[tauri::command]
pub async fn keep_voices(state: St<'_>, keys: Vec<String>, kept: bool) -> Result<u32, String> {
    state.library.keep(&state, keys, kept).await
}

/// Jobs for voices of several presets: one per preset (the voice names in
/// VCP carry the preset).
fn jobs_for(state: &AppState, keys: &[String], kind: JobKind, options: JobOptions) -> Vec<JobRequest> {
    let entries: Vec<LibraryEntry> = keys.iter().filter_map(|k| state.library.get(k)).collect();
    let refs: Vec<&LibraryEntry> = entries.iter().collect();
    let places = locate(state, &refs);
    let mut by_preset: BTreeMap<String, (String, BTreeMap<u32, VoiceDef>, BTreeSet<String>, Vec<String>)> = BTreeMap::new();
    for e in &entries {
        let Some((pid, pname, index, def)) = places.get(&e.def_hash) else { continue };
        let slot = by_preset.entry(pid.clone()).or_insert_with(|| (pname.clone(), BTreeMap::new(), BTreeSet::new(), Vec::new()));
        slot.1.insert(*index, def.clone());
        slot.2.insert(e.lang.clone());
        slot.3.push(e.key.clone());
    }
    by_preset
        .into_iter()
        .map(|(preset_id, (preset_name, defs, langs, only))| JobRequest {
            preset_id,
            preset_name,
            defs: defs.into_iter().map(|(index, def)| JobDef { index, def }).collect(),
            languages: langs.into_iter().collect(),
            options: options.clone(),
            kind,
            only,
            rounds: 2,
        })
        .collect()
}

async fn submit_all(state: &AppState, reqs: Vec<JobRequest>) -> Result<Vec<JobView>, String> {
    if reqs.is_empty() {
        return Err("These voices aren't in any preset (any more).".into());
    }
    let mut out = Vec::new();
    for r in reqs {
        out.push(state.library.submit(state, r).await?);
    }
    Ok(out)
}

/// Designs odd voices again; a new design replaces the old voice only if it
/// sounds better. Up to two tries per voice.
#[tauri::command]
pub async fn improve_voices(state: St<'_>, keys: Vec<String>) -> Result<Vec<JobView>, String> {
    let options = JobOptions { quality: true, takes: 2, ..JobOptions::default() };
    let reqs = jobs_for(&state, &keys, JobKind::Improve, options);
    submit_all(&state, reqs).await
}

/// Checks the quality of computed voices: of one preset or all, only those
/// without a report unless `again`.
#[tauri::command]
pub async fn check_quality(state: St<'_>, preset_id: Option<String>, again: bool) -> Result<Vec<JobView>, String> {
    let mut keys = Vec::new();
    let mut seen = BTreeSet::new();
    let mut collect = |p: &crate::presets::Preset| {
        state.library.with(|map, by_def| {
            for d in &p.defs {
                let h = d.hash();
                for l in by_def.get(&h).into_iter().flatten() {
                    let key = d.key(l);
                    if let Some(e) = map.get(&key)
                        && (again || e.quality.is_none())
                        && seen.insert(key.clone())
                    {
                        keys.push(key);
                    }
                }
            }
        })
    };
    match &preset_id {
        Some(id) => {
            state.presets.with(id, &mut collect).ok_or("This preset doesn't exist (any more).")?;
        }
        None => state.presets.each(collect),
    }
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let reqs = jobs_for(&state, &keys, JobKind::Review, JobOptions::default());
    submit_all(&state, reqs).await
}

/// Plays the sample a voice was cloned from (what the designer made).
#[tauri::command]
pub async fn play_reference(state: St<'_>, key: String) -> Result<f32, String> {
    let e = state.library.get(&key).ok_or("This voice isn't computed (any more).")?;
    let vcp = state.vcp().ok_or("No voice host set up yet.")?;
    let sample = vcp.voice_sample(&e.voice_id).await.map_err(|e| e.to_string())?;
    let wav = base64::engine::general_purpose::STANDARD.decode(sample.audio_base64.as_bytes()).map_err(|e| e.to_string())?;
    let (samples, rate) = ttv_engine::decode_wav(&wav).ok_or("Voice Creator Pro sent unreadable audio.")?;
    let secs = samples.len() as f32 / rate.max(1) as f32;
    state.audio.skip_all();
    let id = state.audio.play(samples, rate, e.gain);
    state.audio.wait(id).await;
    Ok(secs)
}
