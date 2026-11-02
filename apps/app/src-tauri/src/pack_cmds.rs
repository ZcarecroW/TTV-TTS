//! Window commands for voice packs: share presets and computed voices with
//! friends (export), and add packs from friends or downloads (import).

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use ttv_link::{ExportRequest, ExportVoice, JobView, PackManifest, PackPreset};

use crate::presets::{self, Preset};
use crate::state::AppState;

type St<'a> = State<'a, Arc<AppState>>;

#[derive(Debug, Clone, Serialize)]
pub struct PackPresetInfo {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub voices: u32,
    /// "known" (the app has it), "new" (will be added), "copy" (an own preset
    /// with this id but other voices exists – added as a copy)
    pub status: String,
}

/// What a pack holds, for the import dialog.
#[derive(Debug, Clone, Serialize)]
pub struct PackInfo {
    pub path: String,
    pub title: String,
    pub created: i64,
    pub app: String,
    pub bytes: u64,
    pub voices: u32,
    /// Voices this library doesn't have yet.
    pub new_voices: u32,
    pub languages: Vec<String>,
    pub presets: Vec<PackPresetInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub path: String,
    pub voices: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub job: JobView,
    /// Own presets that were added.
    pub presets: Vec<String>,
}

const FILTER: &str = "TTV-TTS voice pack";

/// Lets the user pick a pack file.
#[tauri::command]
pub async fn pick_pack(app: AppHandle) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().add_filter(FILTER, &[ttv_engine::pack::EXTENSION]).pick_file(move |p| {
        let _ = tx.send(p);
    });
    rx.await.ok().flatten().and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned())
}

fn status(state: &AppState, p: &PackPreset) -> (String, Option<presets::Parsed>) {
    if p.builtin && presets::builtins().iter().any(|b| b.id == p.id) {
        return ("known".into(), None);
    }
    let parsed = presets::parse(&p.text);
    let hashes: Vec<String> = parsed.defs.iter().map(|d| d.hash()).collect();
    let s = match state.presets.get(&p.id) {
        Some(e) if e.defs.iter().map(|d| d.hash()).collect::<Vec<_>>() == hashes => "known",
        Some(_) => "copy",
        None => "new",
    };
    (s.into(), Some(parsed))
}

/// Reads a pack's manifest (nothing is changed).
#[tauri::command]
pub fn pack_info(state: St, path: String) -> Result<PackInfo, String> {
    let m = ttv_engine::pack::read_manifest(Path::new(&path))?;
    Ok(info(&state, &path, &m))
}

fn info(state: &AppState, path: &str, m: &PackManifest) -> PackInfo {
    let bytes = std::fs::metadata(path).map(|x| x.len()).unwrap_or(0);
    let new_voices = state.library.with(|map, _| m.voices.iter().filter(|v| !map.contains_key(&v.key)).count()) as u32;
    let languages: BTreeSet<String> = m.voices.iter().map(|v| v.lang.clone()).collect();
    let presets = m
        .presets
        .iter()
        .map(|p| PackPresetInfo { id: p.id.clone(), name: p.name.clone(), builtin: p.builtin, voices: m.voices.iter().filter(|v| v.preset_id == p.id).count() as u32, status: status(state, p).0 })
        .collect();
    PackInfo { path: path.into(), title: m.title.clone(), created: m.created, app: m.app.clone(), bytes, voices: m.voices.len() as u32, new_voices, languages: languages.into_iter().collect(), presets }
}

/// Adds the pack's own presets (built-ins the app knows stay as they are).
fn add_presets(state: &AppState, m: &PackManifest) -> Vec<String> {
    let mut added = Vec::new();
    for p in &m.presets {
        let (st, parsed) = status(state, p);
        let Some(parsed) = parsed.filter(|x| st != "known" && !x.has_errors() && !x.defs.is_empty()) else { continue };
        let copy = st == "copy";
        let name = if p.name.is_empty() { parsed.name.clone() } else { p.name.clone() };
        let preset = Preset {
            id: if copy { String::new() } else { p.id.clone() },
            name: if copy { format!("{name} (imported)") } else { name },
            description: parsed.description,
            icon: if parsed.icon.is_empty() { "Waveform".into() } else { parsed.icon },
            builtin: false,
            target: parsed.defs.len() as u32,
            defs: parsed.defs,
        };
        match state.presets.put(preset) {
            Ok(p) => added.push(p.name),
            Err(e) => log::warn!("pack preset {}: {e}", p.name),
        }
    }
    added
}

/// Adds a pack: its own presets now, its voices as an import job (they are
/// saved in VCP one by one, in pauses between chat messages).
#[tauri::command]
pub async fn import_pack(state: St<'_>, path: String) -> Result<ImportResult, String> {
    let m = ttv_engine::pack::read_manifest(Path::new(&path))?;
    let presets = add_presets(&state, &m);
    let job = state.library.import_pack(&state, Path::new(&path)).await?;
    log::info!("pack {}: {} voices queued, presets added: {presets:?}", m.title, job.total);
    Ok(ImportResult { job, presets })
}

/// Saves a pack of a preset's computed voices: `keys` (selected voices) or
/// the first `count` voices (0 = all) in `languages`.
#[tauri::command]
pub async fn export_pack(app: AppHandle, state: St<'_>, preset_id: String, languages: Vec<String>, count: u32, keys: Vec<String>) -> Result<Option<ExportResult>, String> {
    let p = state.presets.get(&preset_id).ok_or("This preset doesn't exist (any more).")?;
    let wanted: HashSet<&str> = keys.iter().map(String::as_str).collect();
    let n = if count == 0 { p.defs.len() } else { (count as usize).min(p.defs.len()) };
    let mut langs_used = BTreeSet::new();
    let voices: Vec<ExportVoice> = state.library.with(|map, _| {
        let mut out = Vec::new();
        for (i, d) in p.defs.iter().enumerate() {
            for l in &languages {
                let key = d.key(l);
                let pick = if wanted.is_empty() { i < n } else { wanted.contains(key.as_str()) };
                if pick && map.contains_key(&key) {
                    langs_used.insert(l.clone());
                    out.push(ExportVoice { key, preset_id: p.id.clone(), index: i as u32, name: d.name.clone(), description: d.description.clone() });
                }
            }
        }
        out
    });
    if voices.is_empty() {
        return Err("None of these voices is computed yet.".into());
    }
    let langs: Vec<String> = langs_used.into_iter().collect();
    let title = format!("{} – {} voices ({})", p.name, voices.len(), langs.join(", "));
    let file_name: String = format!("{} {}.{}", p.name, langs.join(" "), ttv_engine::pack::EXTENSION).chars().map(|c| if r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().add_filter(FILTER, &[ttv_engine::pack::EXTENSION]).set_file_name(file_name).save_file(move |f| {
        let _ = tx.send(f);
    });
    let Some(to) = rx.await.ok().flatten().and_then(|f| f.into_path().ok()) else { return Ok(None) };
    let to: PathBuf = if to.extension().is_some_and(|e| e == ttv_engine::pack::EXTENSION) { to } else { to.with_extension(ttv_engine::pack::EXTENSION) };
    let req = ExportRequest { title, presets: vec![PackPreset { id: p.id.clone(), name: p.name.clone(), builtin: p.builtin, text: presets::serialize(&p) }], voices };
    let n = req.voices.len() as u32;
    let bytes = state.library.export_pack(&state, req, &to).await?;
    Ok(Some(ExportResult { path: to.to_string_lossy().into_owned(), voices: n, bytes }))
}

// ── ready-made packs (GitHub release "voice-packs") ─────────────────────

/// Where the ready-made packs live. Only files listed in its catalog, with
/// their SHA-256, are downloaded.
/// All ready-made packs, with the release each one is on (a release holds
/// at most 1000 files: one per language). `catalog.json` next to it lists
/// only the packs on this release, for TTV-TTS 2.0.
const CATALOG: &str = "https://github.com/ZcarecroW/TTV-TTS/releases/download/voice-packs/catalog-v2.json";
const RELEASES: &str = "https://github.com/ZcarecroW/TTV-TTS/releases/download/";

/// One ready-made pack: voices `from`–`to` (1-based) of a built-in preset in one language.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CatalogPack {
    pub file: String,
    /// The release the file is on ("voice-packs" when not given).
    #[serde(default)]
    pub tag: Option<String>,
    pub preset: String,
    pub lang: String,
    pub from: u32,
    pub to: u32,
    pub voices: u32,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub version: u32,
    pub updated: String,
    pub packs: Vec<CatalogPack>,
}

fn web() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("TTV-TTS/", env!("CARGO_PKG_VERSION"), " (+https://github.com/ZcarecroW/TTV-TTS)"))
        .build()
        .expect("web client")
}

/// The list of ready-made packs (kept for 10 minutes).
#[tauri::command]
pub async fn pack_catalog() -> Result<Catalog, String> {
    static CACHE: parking_lot::Mutex<Option<(std::time::Instant, Catalog)>> = parking_lot::Mutex::new(None);
    if let Some((t, c)) = CACHE.lock().as_ref()
        && t.elapsed() < std::time::Duration::from_secs(600)
    {
        return Ok(c.clone());
    }
    let r = web().get(CATALOG).timeout(std::time::Duration::from_secs(20)).send().await.map_err(|e| format!("The list of ready-made voices can't be loaded: {e}"))?;
    if r.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Catalog::default());
    }
    let c: Catalog = r.error_for_status().map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    *CACHE.lock() = Some((std::time::Instant::now(), c.clone()));
    Ok(c)
}

#[derive(Debug, Clone, Serialize)]
struct DownloadProgress {
    file: String,
    done: u64,
    total: u64,
}

/// Downloads ready-made packs (checks each one's SHA-256) and imports them.
#[tauri::command]
pub async fn download_packs(app: AppHandle, state: St<'_>, files: Vec<String>) -> Result<Vec<JobView>, String> {
    use sha2::Digest;
    use tauri::Emitter;
    use tokio::io::AsyncWriteExt;
    let catalog = pack_catalog().await?;
    let dir = std::env::temp_dir().join("ttv-tts-downloads");
    tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
    let http = web();
    let mut jobs = Vec::new();
    for name in &files {
        let pack = catalog.packs.iter().find(|p| &p.file == name).ok_or_else(|| format!("{name} isn't in the list of ready-made voices."))?;
        let tag = pack.tag.as_deref().unwrap_or("voice-packs");
        if pack.file.contains(['/', '\\']) || pack.file.contains("..") || !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
            return Err("The list of ready-made voices is broken.".into());
        }
        let path = dir.join(&pack.file);
        let mut r = http.get(format!("{RELEASES}{tag}/{}", pack.file)).timeout(std::time::Duration::from_secs(3600)).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
        let mut f = tokio::fs::File::create(&path).await.map_err(|e| e.to_string())?;
        let mut hash = sha2::Sha256::new();
        let mut done = 0u64;
        let mut last = std::time::Instant::now();
        while let Some(chunk) = r.chunk().await.map_err(|e| e.to_string())? {
            hash.update(&chunk);
            done += chunk.len() as u64;
            if done > pack.bytes + (1 << 20) {
                return Err(format!("{}: larger than announced", pack.file));
            }
            f.write_all(&chunk).await.map_err(|e| e.to_string())?;
            if last.elapsed() > std::time::Duration::from_millis(250) {
                last = std::time::Instant::now();
                let _ = app.emit("app://download", DownloadProgress { file: pack.file.clone(), done, total: pack.bytes });
            }
        }
        f.sync_all().await.map_err(|e| e.to_string())?;
        drop(f);
        if hex::encode(hash.finalize()) != pack.sha256.to_lowercase() {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(format!("{}: the download is damaged (checksum) – try again.", pack.file));
        }
        let _ = app.emit("app://download", DownloadProgress { file: pack.file.clone(), done, total: pack.bytes });
        let m = ttv_engine::pack::read_manifest(&path)?;
        add_presets(&state, &m);
        let job = state.library.import_pack(&state, &path).await;
        let _ = tokio::fs::remove_file(&path).await;
        jobs.push(job?);
    }
    Ok(jobs)
}

#[derive(Serialize)]
pub struct ProxyAnswer {
    pub status: u16,
    /// The answer's body, base64.
    pub body: String,
}

/// Development builds only: one request to the paired proxy with this app's
/// pairing – the maintainer's pack builder (scripts/build-voice-packs.mjs)
/// uses it, so no token has to leave the app. Only `/ttv/v1/` paths.
#[tauri::command]
pub async fn debug_proxy_fetch(state: St<'_>, method: String, path: String, body: Option<String>) -> Result<ProxyAnswer, String> {
    use base64::Engine as _;
    if !cfg!(debug_assertions) {
        return Err("only in development builds".into());
    }
    if !path.starts_with("/ttv/v1/") {
        return Err("only /ttv/v1/ paths".into());
    }
    let c = state.vcp().ok_or("No voice host set up yet.")?;
    let m = reqwest::Method::from_bytes(method.to_ascii_uppercase().as_bytes()).map_err(|e| e.to_string())?;
    let mut r = c.request(m, &path).timeout(std::time::Duration::from_secs(3600));
    if let Some(b) = body {
        r = r.header("Content-Type", "application/json").body(b);
    }
    let resp = r.send().await.map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    Ok(ProxyAnswer { status, body: base64::engine::general_purpose::STANDARD.encode(bytes) })
}
