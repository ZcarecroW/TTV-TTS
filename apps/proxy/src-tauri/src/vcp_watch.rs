//! Watches Voice Creator Pro on this PC: is its API up, is our key accepted,
//! which model is loaded, is its process running. Starts, stops and restarts
//! it on request, and optionally keeps it running.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use ttv_vcp::Error as VcpError;

use crate::state::AppState;

const TICK: Duration = Duration::from_secs(3);
const DETAIL_EVERY: Duration = Duration::from_secs(15);
const SCAN_EVERY: Duration = Duration::from_secs(15);
const START_TIMEOUT: Duration = Duration::from_secs(240);
const READY_AFTER_START: Duration = Duration::from_secs(8);

/// Set while the user stopped VCP on purpose (no automatic restart).
static STOPPED_ON_PURPOSE: AtomicBool = AtomicBool::new(false);
/// Only one start/stop at a time.
static BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Serialize)]
pub struct FoundProcess {
    pub pid: u32,
    pub name: String,
    pub exe: Option<String>,
    pub parent: Option<u32>,
}

fn looks_like_vcp(name: &str) -> bool {
    let n = name.to_ascii_lowercase().replace(['-', '_'], " ");
    n.contains("voice creator") || n.contains("voicecreator")
}

/// VCP's processes. The first entry is the main one (its parent is not VCP).
pub fn find_processes(sys: &mut Option<System>, configured: Option<&str>) -> Vec<FoundProcess> {
    let Some(sys) = sys.as_mut() else { return Vec::new() };
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet));
    let configured = configured.map(|c| c.to_ascii_lowercase());
    let mut found: Vec<FoundProcess> = sys
        .processes()
        .values()
        .filter(|p| {
            let name = p.name().to_string_lossy();
            let exe = p.exe().map(|e| e.to_string_lossy().to_ascii_lowercase());
            looks_like_vcp(&name) || (configured.is_some() && exe.as_deref() == configured.as_deref())
        })
        .map(|p| FoundProcess {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
            parent: p.parent().map(|x| x.as_u32()),
        })
        .collect();
    let pids: Vec<u32> = found.iter().map(|p| p.pid).collect();
    found.sort_by_key(|p| (p.parent.is_some_and(|pp| pids.contains(&pp)), p.pid));
    found
}

/// One monitoring step. Returns true if the visible state changed.
async fn tick(state: &Arc<AppState>, sys: &mut Option<System>, last_detail: &mut Option<Instant>, last_scan: &mut Option<Instant>) -> bool {
    let vcp = state.vcp();
    let before = state.vcp_state.read().clone();
    let mut s = before.clone();

    match vcp.health().await {
        Ok(()) => {
            let was_down = !s.reachable;
            s.reachable = true;
            s.error = None;
            if was_down || s.version.is_none() {
                s.version = vcp.version().await.ok();
                *last_detail = None;
                *last_scan = None;
            }
            match vcp.generation_queue().await {
                Ok(q) => {
                    s.key_ok = Some(true);
                    if vcp.key().is_none() {
                        s.key_required = Some(false);
                    }
                    s.queue_size = q.queue_size;
                    s.generating = q.is_generating;
                }
                Err(VcpError::Unauthorized) => {
                    s.key_ok = Some(false);
                    s.key_required = Some(true);
                    s.error = Some(if vcp.key().is_some() {
                        "VCP rejected the API key stored in the proxy.".into()
                    } else {
                        "VCP has an API key set - enter it in the proxy.".into()
                    });
                    // Take the key over from VCP's own settings, if possible.
                    if crate::vcp_key::auto(state).await {
                        s.key_ok = None;
                        s.error = None;
                    }
                }
                Err(e) => s.error = Some(e.to_string()),
            }
            if s.key_ok == Some(true) && last_detail.is_none_or(|t| t.elapsed() > DETAIL_EVERY) {
                *last_detail = Some(Instant::now());
                if let Ok(m) = vcp.model_status().await {
                    s.model_loaded = m.loaded;
                    s.model_kind = m.kind;
                    s.model_name = m.name;
                }
                if let Ok(a) = vcp.agent_settings().await {
                    s.active_model = a.default_model;
                }
                if s.hardware.is_none() {
                    s.hardware = vcp.system_info().await.ok();
                }
            }
        }
        Err(e) => {
            if s.reachable {
                log::warn!("VCP stopped answering: {e}");
                *last_scan = None;
            }
            s.reachable = false;
            s.key_ok = None;
            s.generating = false;
            s.queue_size = 0;
            s.model_loaded = false;
            s.error = Some(e.to_string());
        }
    }

    // The process list is comparatively expensive: scan rarely, and right
    // away when reachability changed.
    if last_scan.is_none_or(|t| t.elapsed() > SCAN_EVERY) {
        *last_scan = Some(Instant::now());
        let configured = state.cfg.read().vcp_exe.clone();
        let procs = find_processes(sys, configured.as_deref());
        match procs.first() {
            Some(p) => {
                s.running = true;
                s.pid = Some(p.pid);
                if let Some(exe) = &p.exe {
                    s.exe = Some(exe.clone());
                    let mut cfg = state.cfg.write();
                    if cfg.vcp_exe.as_deref() != Some(exe.as_str()) {
                        log::info!("found Voice Creator Pro at {exe}");
                        cfg.vcp_exe = Some(exe.clone());
                        drop(cfg);
                        let _ = state.save_config();
                    }
                }
            }
            None => {
                s.running = false;
                s.pid = None;
                s.exe = state.cfg.read().vcp_exe.clone();
            }
        }
    }
    // An API that answers means VCP runs, even if its process has an odd name.
    if s.reachable {
        s.running = true;
    }

    if s.model_loaded
        && let Some(name) = s.model_name.as_deref()
    {
        ttv_vcp::perf::set_loaded(s.model_kind.as_deref(), name);
    }
    // rounded, so the status isn't sent again for every clip
    let sp = ttv_vcp::perf::speed();
    let round = |v: Option<f32>| v.map(|x| (x * 20.0).round() / 20.0);
    s.speed = round(sp.now);
    s.usual_speed = round(sp.usual);

    s.benchmark = ttv_vcp::exclusive();

    let changed = s != before;
    *state.vcp_state.write() = s;
    changed
}

/// Automatic restarts (hanging or slow VCP) at most this often.
const AUTO_RESTART_EVERY: i64 = 1800;
static LAST_AUTO_RESTART: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Claims the next automatic restart; false if one happened recently.
pub fn auto_restart_due() -> bool {
    let now = chrono::Utc::now().timestamp();
    let last = LAST_AUTO_RESTART.load(Ordering::Relaxed);
    if now - last < AUTO_RESTART_EVERY {
        return false;
    }
    LAST_AUTO_RESTART.compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed).is_ok()
}

static SLOW_RESTART: AtomicBool = AtomicBool::new(false);
/// Speed before the last restart for slowness (bits of an f32; 0 = none).
static SLOW_BEFORE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// A restart didn't make VCP faster: this machine is just slow.
static SLOW_GAVE_UP: AtomicBool = AtomicBool::new(false);

/// VCP has become clearly slower than usual: restart it once chat has been
/// quiet for a minute, while nobody else may use it, and warm it up again.
async fn restart_slow(state: Arc<AppState>) {
    loop {
        if !ttv_vcp::perf::speed().slow() || !state.cfg.read().restart_when_slow {
            return;
        }
        if crate::forward::last_work().is_none_or(|t| t.elapsed() > Duration::from_secs(60)) {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    if !auto_restart_due() {
        return;
    }
    let sp = ttv_vcp::perf::speed();
    let (now, usual) = (sp.now.unwrap_or(0.0), sp.usual.unwrap_or(0.0));
    let before = f32::from_bits(SLOW_BEFORE.load(Ordering::Relaxed));
    if before > 0.0 && now >= before * 0.8 {
        log::info!("restarting VCP didn't make it faster ({before:.2} before, {now:.2} after) - not restarting it for slowness again");
        SLOW_GAVE_UP.store(true, Ordering::Relaxed);
        return;
    }
    let _turn = ttv_vcp::turn("/ttv/restart").await;
    SLOW_BEFORE.store(now.to_bits(), Ordering::Relaxed);
    log::warn!(
        "Voice Creator Pro has become {:.1}x slower than usual ({now:.2} instead of {usual:.2} s per second of speech) - restarting it",
        if usual > 0.0 { now / usual } else { 0.0 }
    );
    let t0 = Instant::now();
    match restart(&state).await {
        Ok(m) => {
            ttv_vcp::perf::clear_recent();
            warm_up(&state).await;
            log::info!("{m} (restart and warm-up took {:.0} s)", t0.elapsed().as_secs_f32());
        }
        Err(e) => log::warn!("restarting VCP failed: {e}"),
    }
    state.publish();
}

/// One short sentence with a saved voice: VCP loads its speaking model now,
/// not with the next chat message. Called while holding the turn, so it goes
/// to VCP directly.
pub async fn warm_up(state: &Arc<AppState>) {
    let vcp = state.vcp();
    let Ok(voices) = vcp.voices().await else { return };
    let Some(v) = voices.iter().find(|v| v.has_voice_clone_prompt) else { return };
    let body = serde_json::json!({ "voice_id": v.id, "target_text": "Hello, this is a short test.", "language": "English" });
    let t0 = Instant::now();
    let r = vcp.request(ttv_vcp::Method::POST, "/api/v1/generation/clone?sync=true").json(&body).timeout(Duration::from_secs(300)).send().await;
    match r {
        Ok(r) if r.status().is_success() => {
            let _ = r.bytes().await;
            log::info!("VCP warmed up in {:.1} s", t0.elapsed().as_secs_f32());
        }
        Ok(r) => log::info!("warming VCP up: HTTP {}", r.status()),
        Err(e) => log::info!("warming VCP up: {e}"),
    }
}

/// Usual speeds per model are kept, so a proxy restart doesn't forget what
/// a fresh VCP manages.
fn perf_path(state: &AppState) -> std::path::PathBuf {
    state.data_dir.join("vcp-speed.json")
}

pub async fn run(state: Arc<AppState>) {
    let mut sys = System::new().ok();
    let mut last_detail = None;
    let mut last_scan = None;
    let mut down_since: Option<Instant> = None;
    let mut first = true;
    if let Some(m) = std::fs::read(perf_path(&state)).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
        ttv_vcp::perf::set_usual(m);
    }
    let mut saved_usual = ttv_vcp::perf::usual();
    loop {
        if tick(&state, &mut sys, &mut last_detail, &mut last_scan).await {
            state.publish();
        }
        let (running, reachable, starting) = {
            let s = state.vcp_state.read();
            (s.running, s.reachable, s.starting)
        };
        let (start_vcp, keep, hang, hang_secs, when_slow) = {
            let c = state.cfg.read();
            (c.start_vcp, c.keep_vcp_running, c.restart_on_hang, c.hang_restart_secs, c.restart_when_slow)
        };
        let usual = ttv_vcp::perf::usual();
        if usual != saved_usual {
            if let Ok(b) = serde_json::to_vec(&usual) {
                let _ = std::fs::write(perf_path(&state), b);
            }
            saved_usual = usual;
        }
        if when_slow && reachable && !starting && !ttv_vcp::exclusive() && !SLOW_GAVE_UP.load(Ordering::Relaxed) && ttv_vcp::perf::speed().slow() && !SLOW_RESTART.swap(true, Ordering::Relaxed) {
            let st = state.clone();
            tokio::spawn(async move {
                restart_slow(st).await;
                SLOW_RESTART.store(false, Ordering::Relaxed);
            });
        }
        if first {
            first = false;
            if start_vcp && !running && !reachable {
                log::info!("starting Voice Creator Pro (\"start with the proxy\" is on)");
                let st = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = start(&st).await {
                        log::warn!("{e}");
                    }
                });
            }
        }
        if reachable || starting {
            down_since = None;
        } else {
            let since = *down_since.get_or_insert_with(Instant::now);
            let purposely = STOPPED_ON_PURPOSE.load(Ordering::Relaxed);
            if keep && !running && !purposely && since.elapsed() > Duration::from_secs(30) {
                log::info!("Voice Creator Pro is not running - starting it (\"keep running\" is on)");
                down_since = None;
                let st = state.clone();
                tokio::spawn(async move {
                    let _ = start(&st).await;
                });
            } else if hang && running && !purposely && !ttv_vcp::exclusive() && since.elapsed() > Duration::from_secs(hang_secs as u64) {
                log::warn!("Voice Creator Pro runs but has not answered for {hang_secs} s - restarting it");
                down_since = None;
                let st = state.clone();
                tokio::spawn(async move {
                    let _ = restart(&st).await;
                });
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(TICK) => {}
            _ = state.poke.notified() => { last_detail = None; }
        }
    }
}

fn set_starting(state: &Arc<AppState>, on: bool) {
    state.vcp_state.write().starting = on;
    state.publish();
}

fn spawn_vcp(exe: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        // Start the .app bundle the program file belongs to.
        let bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).unwrap_or(exe);
        std::process::Command::new("open").arg("-a").arg(bundle).spawn()?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let mut cmd = std::process::Command::new(exe);
        if let Some(dir) = exe.parent() {
            cmd.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        }
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        cmd.spawn()?;
        Ok(())
    }
}

/// Starts VCP and waits until its API answers.
pub async fn start(state: &Arc<AppState>) -> Result<String, String> {
    let _busy = BUSY.lock().await;
    let vcp = state.vcp();
    if vcp.health().await.is_ok() {
        return Ok("Voice Creator Pro is already running.".into());
    }
    let exe = state.cfg.read().vcp_exe.clone().map(PathBuf::from);
    let Some(exe) = exe.filter(|p| p.exists()) else {
        return Err("The proxy doesn't know where Voice Creator Pro is installed yet. Start it once by hand - the proxy remembers it - or choose its program file in the VCP tab.".into());
    };
    STOPPED_ON_PURPOSE.store(false, Ordering::Relaxed);
    log::info!("starting {}", exe.display());
    spawn_vcp(&exe).map_err(|e| format!("Starting Voice Creator Pro failed: {e}"))?;
    set_starting(state, true);
    let t0 = Instant::now();
    let result = loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if vcp.health().await.is_ok() {
            break Ok(format!("Voice Creator Pro is up after {} s.", t0.elapsed().as_secs()));
        }
        if t0.elapsed() > START_TIMEOUT {
            break Err("Voice Creator Pro was started, but its HTTP API did not answer within 4 minutes. Is the HTTP API enabled in its settings?".into());
        }
    };
    set_starting(state, false);
    state.poke.notify_one();
    match &result {
        Ok(m) => log::info!("{m}"),
        Err(e) => log::warn!("{e}"),
    }
    result
}

/// Closes VCP: politely through its API, by force if it doesn't react.
pub async fn stop(state: &Arc<AppState>) -> Result<String, String> {
    let _busy = BUSY.lock().await;
    STOPPED_ON_PURPOSE.store(true, Ordering::Relaxed);
    let vcp = state.vcp();
    let polite = vcp.shutdown().await.is_ok();
    if polite {
        log::info!("asked Voice Creator Pro to close");
    }
    let configured = state.cfg.read().vcp_exe.clone();
    let deadline = Instant::now() + Duration::from_secs(if polite { 30 } else { 2 });
    let mut sys = System::new().ok();
    loop {
        let procs = find_processes(&mut sys, configured.as_deref());
        if procs.is_empty() && vcp.health().await.is_err() {
            state.poke.notify_one();
            return Ok("Voice Creator Pro is closed.".into());
        }
        if Instant::now() > deadline {
            for p in &procs {
                if let Some(proc_) = sys.as_ref().and_then(|s| s.process(sysinfo::Pid::from_u32(p.pid))) {
                    proc_.kill();
                }
            }
            log::warn!("Voice Creator Pro did not close by itself - ended it");
            tokio::time::sleep(Duration::from_secs(2)).await;
            state.poke.notify_one();
            return Ok("Voice Creator Pro was ended.".into());
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
}

pub async fn restart(state: &Arc<AppState>) -> Result<String, String> {
    stop(state).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let m = start(state).await?;
    // its API answers seconds before it can work (HTTP 500 until then), and
    // whoever restarted it goes straight back to work
    tokio::time::sleep(READY_AFTER_START).await;
    Ok(m)
}

// ── diagnostics ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ConfigCandidate {
    pub path: String,
    pub size: u64,
    /// The file mentions an API-key setting (the value is never read out).
    pub mentions_api_key: bool,
}

#[derive(Serialize)]
pub struct Diagnostics {
    pub proxy_version: String,
    pub os: String,
    pub proxy_exe: Option<String>,
    pub data_dir: String,
    pub vcp: ttv_link::VcpState,
    pub processes: Vec<FoundProcess>,
    pub config_candidates: Vec<ConfigCandidate>,
    pub listen_error: Option<String>,
}

impl Diagnostics {
    fn empty() -> Self {
        Self {
            proxy_version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            proxy_exe: None,
            data_dir: String::new(),
            vcp: Default::default(),
            processes: Vec::new(),
            config_candidates: Vec::new(),
            listen_error: None,
        }
    }
}

/// Folders that never hold settings (VCP's projects, caches, Python packages).
const SKIP_DIRS: &[&str] = &[
    "models", "cache", "node_modules", "site-packages", "lib", "venv", ".venv", "python", "torch", "huggingface", "packages",
    "audiobooks", "history", "dubbing_projects", "voices", "mplcache", "clips", "aicut", "talking_head", "uploads", "youtube",
    "temp", "tmp", "logs",
];

fn scan_dir(dir: &Path, depth: u32, out: &mut Vec<ConfigCandidate>) {
    if out.len() >= 200 || depth > 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            let n = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            // Skip bulky folders that never hold settings.
            if SKIP_DIRS.iter().any(|s| n == *s) {
                continue;
            }
            scan_dir(&p, depth + 1, out);
        } else if meta.len() < 1_048_576 {
            let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if ["json", "toml", "ini", "yaml", "yml", "cfg", "conf", "env"].contains(&ext.as_str()) {
                let mentions = std::fs::read(&p)
                    .map(|b| {
                        let s = String::from_utf8_lossy(&b).to_ascii_lowercase();
                        s.contains("api_key") || s.contains("apikey") || s.contains("api-key")
                    })
                    .unwrap_or(false);
                out.push(ConfigCandidate { path: p.to_string_lossy().into_owned(), size: meta.len(), mentions_api_key: mentions });
            }
        }
        if out.len() >= 200 {
            return;
        }
    }
}

/// Where VCP keeps its files: folders next to its program and in the user's
/// app-data folders whose name mentions "voice" or "vcp".
fn candidate_dirs(exe: Option<&str>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = exe.and_then(|e| Path::new(e).parent()) {
        dirs.push(dir.to_path_buf());
    }
    let roots: Vec<PathBuf> = ["APPDATA", "LOCALAPPDATA", "USERPROFILE", "HOME"]
        .iter()
        .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
        .chain(std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support")))
        .collect();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_ascii_lowercase();
            if (n.contains("voice") || n.contains("vcp") || n.contains("mortar")) && e.path().is_dir() {
                dirs.push(e.path());
            }
        }
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

pub async fn diagnostics(state: &Arc<AppState>) -> Diagnostics {
    let configured = state.cfg.read().vcp_exe.clone();
    let vcp = state.vcp_state.read().clone();
    let data_dir = state.data_dir.to_string_lossy().into_owned();
    let listen_error = state.listen_error.read().clone();
    tokio::task::spawn_blocking(move || {
        let mut sys = System::new().ok();
        let processes = find_processes(&mut sys, configured.as_deref());
        let exe = configured.or_else(|| processes.first().and_then(|p| p.exe.clone()));
        let mut config_candidates = Vec::new();
        for d in candidate_dirs(exe.as_deref()) {
            scan_dir(&d, 0, &mut config_candidates);
        }
        Diagnostics {
            proxy_version: env!("CARGO_PKG_VERSION").into(),
            os: format!("{} {}", std::env::consts::OS, sysinfo::System::long_os_version().unwrap_or_default()),
            proxy_exe: std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned()),
            data_dir,
            vcp,
            processes,
            config_candidates,
            listen_error,
        }
    })
    .await
    .unwrap_or_else(|e| {
        log::warn!("diagnostics task failed: {e}");
        Diagnostics::empty()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(looks_like_vcp("Voice Creator Pro.exe"));
        assert!(looks_like_vcp("voice-creator-pro"));
        assert!(looks_like_vcp("VoiceCreatorPro"));
        assert!(!looks_like_vcp("voicemeeter.exe"));
    }
}
