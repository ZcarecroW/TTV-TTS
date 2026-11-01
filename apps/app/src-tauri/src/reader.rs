//! The chat reader: Twitch messages -> profile rules -> speech queue ->
//! generator (one message ahead) -> player.
//!
//! The generator prepares the next message (name announcement, voice,
//! language) and has VCP generate its pieces while the current one plays.
//! The player hands a message's pieces to the audio output and waits for
//! them; only the current message is ever inside the audio queue, so skips
//! and removals (timeouts, bans, deleted messages, mutes) are exact.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::{Notify, mpsc};

use crate::profile::{Announce, ReadMode, Role, VoiceMode};
use crate::speech;
use crate::state::AppState;
use crate::text::{self, lang::LangMemory, langs};
use crate::twitch::emotes::{Emotes, Providers};
use crate::twitch::irc::{Chat, ChatEvent, ChatMessage, Kind};

const HISTORY: usize = 150;

// ── persisted data ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileStore {
    pub profiles: Vec<crate::profile::Profile>,
    /// Profiles that read their channels right now.
    pub active: Vec<String>,
}

/// Which voice (role index) each viewer got, per profile. Kept forever so a
/// growing voice pool never changes anyone's voice.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Assignments {
    pub roles: HashMap<String, HashMap<String, u32>>,
    /// profile -> user id -> who that is (for the viewer list).
    pub viewers: HashMap<String, HashMap<String, Viewer>>,
    /// profile -> user id -> voice effect set in the viewer list ("none"
    /// switches the voice's own effect off).
    pub fx: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Viewer {
    pub login: String,
    pub display: String,
    pub color: Option<String>,
    /// Unix seconds.
    pub last_seen: i64,
    pub messages: u32,
}

/// A row of the viewer list.
#[derive(Debug, Clone, Serialize)]
pub struct ViewerView {
    pub user_id: String,
    #[serde(flatten)]
    pub info: Viewer,
    /// Voice role (index into the preset or pool).
    pub role: Option<u32>,
    /// Name of the voice the role stands for (preset mode).
    pub voice: Option<String>,
    /// A fixed voice set in the profile (login -> voice).
    pub pinned: Option<String>,
    pub muted: bool,
    /// Effect set for this viewer ("none" = off); None = the voice's own.
    pub fx: Option<String>,
    /// The effect of the voice itself (preset definition).
    pub voice_fx: Option<String>,
}

// ── views for the window ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    Queued,
    Speaking,
    Spoken,
    Skipped,
    Removed,
    Failed,
    Filtered,
    /// Approval mode: waits for the streamer.
    Pending,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    pub id: u64,
    pub channel: String,
    pub login: String,
    pub display: String,
    pub color: Option<String>,
    pub original: String,
    /// What is (or was) spoken.
    pub speech: String,
    pub lang: String,
    pub voice: Option<String>,
    pub state: ItemState,
    /// Why a message was not read (filtered) or failed.
    pub reason: Option<String>,
    pub at: i64,
    pub first_audio_ms: Option<u64>,
    /// Emotion picked from the message's cues, and how strongly.
    pub emotion: Option<String>,
    pub intensity: Option<String>,
    /// "Translate for me": the message in the streamer's language.
    pub translation: Option<String>,
    /// An event announcement: sub, resub, subgift, submysterygift, raid, bits.
    pub event: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ChannelState {
    pub name: String,
    pub joined: bool,
    pub room_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ReaderStatus {
    pub connected: bool,
    pub error: Option<String>,
    pub channels: Vec<ChannelState>,
    pub paused: bool,
    pub queue: Vec<ItemView>,
    pub current: Option<ItemView>,
    /// Approval mode: messages waiting for the streamer.
    pub pending: Vec<ItemView>,
    /// Spoken messages that can be played again (ids, newest last).
    pub replayable: Vec<u64>,
}

// ── runtime data ────────────────────────────────────────────────────────

struct Item {
    view: ItemView,
    cue: Option<text::emotion::Cue>,
    profile: String,
    user_id: String,
    msg_id: String,
    name: String,
    action: bool,
    cancel: Arc<AtomicBool>,
}

/// A spoken message kept for "play again": (id, rate, clips as played).
type Replay = (u64, u32, Vec<Vec<f32>>);

/// (id, user_id, msg_id, login, cancel) of a message being generated or played.
type Inflight = (u64, String, String, String, Arc<AtomicBool>);

struct Prepared {
    item: Item,
    clips: mpsc::Receiver<Result<(Vec<f32>, u32), String>>,
    /// Loudness correction of the voice.
    gain: f32,
    fx: Option<crate::fx::Fx>,
    /// Set when the system voice had to read (why the voice host failed).
    fell_back: Arc<Mutex<Option<String>>>,
}

#[derive(Default)]
struct VoiceCache {
    all: Vec<ttv_vcp::Voice>,
    /// language code -> voices of that pool (sorted)
    pools: HashMap<String, Vec<ttv_vcp::Voice>>,
    /// Languages the active model speaks (VCP names, lower case).
    languages: Option<Vec<String>>,
    default_voice: Option<String>,
    /// The active model speaks with emotions (VCP's emotional clone).
    emotional: bool,
    loaded: Option<Instant>,
}

pub struct ReaderData {
    pub store: RwLock<ProfileStore>,
    store_path: std::path::PathBuf,
    assign: Mutex<Assignments>,
    assign_path: std::path::PathBuf,
    assign_dirty: AtomicBool,
    chat: Mutex<Option<Chat>>,
    emotes: Emotes,
    queue: Mutex<VecDeque<Item>>,
    /// Approval mode: waiting for the streamer.
    pending: Mutex<VecDeque<Item>>,
    /// The audio of the last spoken messages: (id, rate, clips).
    replays: Mutex<VecDeque<Replay>>,
    wake: Notify,
    paused: AtomicBool,
    status: RwLock<ReaderStatus>,
    history: Mutex<VecDeque<ItemView>>,
    lang: Mutex<HashMap<String, LangMemory>>,
    cooldown: Mutex<HashMap<String, Instant>>,
    /// Who spoke last and when (name announcements).
    last: Mutex<Option<(String, Instant)>>,
    voices: RwLock<VoiceCache>,
    /// Items being generated or played.
    inflight: Mutex<Vec<Inflight>>,
    seq: AtomicU64,
}

impl ReaderData {
    pub fn load(data_dir: &std::path::Path) -> Self {
        let store_path = data_dir.join("profiles.json");
        let assign_path = data_dir.join("assignments.json");
        let mut store: ProfileStore = crate::storage::read_json(&store_path).unwrap_or_default();
        if store.profiles.is_empty() {
            store.profiles.push(crate::profile::Profile::default());
        }
        store.profiles.iter_mut().for_each(|p| p.normalize());
        Self {
            store: RwLock::new(store),
            store_path,
            assign: Mutex::new(crate::storage::read_json(&assign_path).unwrap_or_default()),
            assign_path,
            assign_dirty: AtomicBool::new(false),
            chat: Mutex::new(None),
            emotes: Emotes::new(),
            queue: Mutex::new(VecDeque::new()),
            pending: Mutex::new(VecDeque::new()),
            replays: Mutex::new(VecDeque::new()),
            wake: Notify::new(),
            paused: AtomicBool::new(false),
            status: RwLock::new(ReaderStatus::default()),
            history: Mutex::new(VecDeque::new()),
            lang: Mutex::new(HashMap::new()),
            cooldown: Mutex::new(HashMap::new()),
            last: Mutex::new(None),
            voices: RwLock::new(VoiceCache::default()),
            inflight: Mutex::new(Vec::new()),
            seq: AtomicU64::new(1),
        }
    }

    pub fn save_store(&self) {
        let s = self.store.read().clone();
        if let Err(e) = crate::storage::write_json(&self.store_path, &s) {
            log::warn!("saving profiles failed: {e}");
        }
    }

    fn save_assignments(&self) {
        if self.assign_dirty.swap(false, Ordering::Relaxed) {
            let a = self.assign.lock().clone();
            if let Err(e) = crate::storage::write_json(&self.assign_path, &a) {
                log::warn!("saving voice assignments failed: {e}");
            }
        }
    }

    pub fn history(&self) -> Vec<ItemView> {
        self.history.lock().iter().cloned().collect()
    }

    pub fn status(&self) -> ReaderStatus {
        self.status.read().clone()
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

// ── start & events ──────────────────────────────────────────────────────

pub fn start(state: Arc<AppState>) {
    let (tx, mut rx) = mpsc::channel::<ChatEvent>(512);
    *state.reader.chat.lock() = Some(Chat::start(tx));
    apply_channels(&state);
    let st = state.clone();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            handle_event(&st, ev);
        }
    });
    tokio::spawn(generator(state.clone()));
    tokio::spawn(housekeeping(state));
}

/// Joins exactly the channels of the active profiles.
pub fn apply_channels(state: &Arc<AppState>) {
    let channels: BTreeSet<String> = {
        let s = state.reader.store.read();
        s.profiles.iter().filter(|p| s.active.contains(&p.id)).flat_map(|p| p.channels.iter().cloned()).collect()
    };
    {
        let mut st = state.reader.status.write();
        let old = std::mem::take(&mut st.channels);
        st.channels = channels
            .iter()
            .map(|c| old.iter().find(|o| &o.name == c).cloned().unwrap_or(ChannelState { name: c.clone(), ..Default::default() }))
            .collect();
    }
    if let Some(chat) = state.reader.chat.lock().as_ref() {
        chat.set_channels(channels);
    }
    publish(state);
}

fn publish(state: &Arc<AppState>) {
    let r = &state.reader;
    {
        let mut st = r.status.write();
        st.paused = r.paused.load(Ordering::Relaxed);
        st.queue = r.queue.lock().iter().map(|i| i.view.clone()).collect();
        st.pending = r.pending.lock().iter().map(|i| i.view.clone()).collect();
        st.replayable = r.replays.lock().iter().map(|(id, ..)| *id).collect();
    }
    if let Some(app) = state.app() {
        let _ = app.emit("app://reader", r.status());
    }
}

fn record(state: &Arc<AppState>, view: ItemView) {
    {
        let mut h = state.reader.history.lock();
        if let Some(existing) = h.iter_mut().find(|x| x.id == view.id) {
            *existing = view.clone();
        } else {
            h.push_back(view.clone());
            while h.len() > HISTORY {
                h.pop_front();
            }
        }
    }
    if let Some(app) = state.app() {
        let _ = app.emit("app://history", &view);
    }
}

fn profile_for(state: &AppState, channel: &str) -> Option<crate::profile::Profile> {
    let s = state.reader.store.read();
    s.profiles.iter().find(|p| s.active.contains(&p.id) && p.channels.iter().any(|c| c == channel)).cloned()
}

fn handle_event(state: &Arc<AppState>, ev: ChatEvent) {
    match ev {
        ChatEvent::Connection { connected, error } => {
            {
                let mut st = state.reader.status.write();
                st.connected = connected;
                st.error = error;
                if !connected {
                    st.channels.iter_mut().for_each(|c| c.joined = false);
                }
            }
            publish(state);
        }
        ChatEvent::Joined { channel, room_id } => {
            {
                let mut st = state.reader.status.write();
                if let Some(c) = st.channels.iter_mut().find(|c| c.name == channel) {
                    c.joined = true;
                    c.room_id = Some(room_id.clone());
                }
            }
            publish(state);
            log::info!("joined #{channel} (room {room_id})");
            if let Some(p) = profile_for(state, &channel) {
                let st = state.clone();
                let prov = Providers { bttv: p.emotes.bttv, ffz: p.emotes.ffz, seventv: p.emotes.seventv };
                tokio::spawn(async move { st.reader.emotes.ensure(&room_id, prov).await });
            }
        }
        ChatEvent::Message(m) => on_message(state, *m),
        ChatEvent::ClearUser { channel, user_id } => {
            if profile_for(state, &channel).is_some_and(|p| p.mod_commands.skip_on_timeout) {
                match user_id {
                    Some(uid) => remove_where(state, |user, _, _| user == uid, "timeout/ban"),
                    None => clear(state, "chat cleared"),
                }
            }
        }
        ChatEvent::ClearMessage { channel, msg_id } => {
            if profile_for(state, &channel).is_some_and(|p| p.mod_commands.skip_on_timeout) {
                remove_where(state, |_, mid, _| mid == msg_id, "message deleted");
            }
        }
    }
}

fn has_role(m: &ChatMessage, roles: &[Role]) -> bool {
    roles.iter().any(|r| match r {
        Role::Broadcaster => m.roles.broadcaster,
        Role::Moderator => m.roles.moderator,
        Role::Vip => m.roles.vip,
        Role::Subscriber => m.roles.subscriber,
        Role::Everyone => true,
    })
}

/// Mod commands (!skip, !ttsclear, …). Returns true if the message was one.
fn mod_command(state: &Arc<AppState>, p: &crate::profile::Profile, m: &ChatMessage) -> bool {
    let mc = &p.mod_commands;
    if !mc.enabled || !has_role(m, &mc.roles) {
        return false;
    }
    let mut words = m.text.split_whitespace();
    let cmd = words.next().unwrap_or("").to_lowercase();
    let arg = words.next().unwrap_or("").trim_start_matches('@').to_lowercase();
    let is = |c: &str| !c.is_empty() && cmd == c.to_lowercase();
    if is(&mc.skip) {
        skip(state);
    } else if is(&mc.clear) {
        clear(state, &m.display_name);
    } else if is(&mc.pause) {
        set_paused(state, true);
    } else if is(&mc.resume) {
        set_paused(state, false);
    } else if is(&mc.volume) && !arg.is_empty() {
        if let Ok(v) = arg.trim_end_matches('%').parse::<f32>() {
            state.audio.set_volume(v / 100.0);
            state.cfg.write().audio.volume = (v / 100.0).clamp(0.0, 1.0);
            let _ = state.save_config();
        }
    } else if is(&mc.mute) && !arg.is_empty() {
        set_muted(state, &p.id, &arg, true);
    } else if is(&mc.unmute) && !arg.is_empty() {
        set_muted(state, &p.id, &arg, false);
    } else {
        return false;
    }
    log::info!("{} used {cmd} in #{}", m.display_name, m.channel);
    true
}

pub fn set_muted(state: &Arc<AppState>, profile: &str, login: &str, on: bool) {
    {
        let mut s = state.reader.store.write();
        if let Some(p) = s.profiles.iter_mut().find(|p| p.id == profile) {
            p.muted.retain(|x| x != login);
            if on {
                p.muted.push(login.to_string());
            }
        }
    }
    state.reader.save_store();
    if on {
        let login = login.to_string();
        remove_where(state, move |_, _, l| l == login, "muted");
    }
}

fn filtered(state: &Arc<AppState>, m: &ChatMessage, reason: &str) {
    record(state, view_of(state, m, String::new(), "", ItemState::Filtered, Some(reason.to_string())));
}

fn view_of(state: &AppState, m: &ChatMessage, speech: String, lang: &str, s: ItemState, reason: Option<String>) -> ItemView {
    ItemView {
        id: state.reader.seq.fetch_add(1, Ordering::Relaxed),
        channel: m.channel.clone(),
        login: m.login.clone(),
        display: m.display_name.clone(),
        color: m.color.clone(),
        original: m.text.clone(),
        speech,
        lang: lang.to_string(),
        voice: None,
        state: s,
        reason,
        at: chrono::Utc::now().timestamp_millis(),
        first_audio_ms: None,
        emotion: None,
        intensity: None,
        translation: None,
        event: None,
    }
}

fn on_message(state: &Arc<AppState>, m: ChatMessage) {
    let Some(p) = profile_for(state, &m.channel) else { return };
    remember_viewer(state, &p.id, &m);
    if m.kind == Kind::Chat && mod_command(state, &p, &m) {
        return;
    }
    if let Some((kind, text)) = crate::announce::for_message(&p.events, &m, langs::intern(&p.language.default)) {
        queue_event(state, &p, &m, kind, text);
    }
    let r = &p.reading;
    if m.kind == Kind::Notice && (!r.read_sub_messages || m.text.trim().is_empty()) {
        return;
    }
    if !r.reward_id.is_empty() && m.reward_id.as_deref() != Some(r.reward_id.as_str()) {
        return filtered(state, &m, "not the chosen channel-points reward");
    }
    if r.ignore_users.iter().any(|u| u.eq_ignore_ascii_case(&m.login)) {
        return filtered(state, &m, "ignored user (bot)");
    }
    if p.muted.contains(&m.login) {
        return filtered(state, &m, "muted");
    }
    if r.ignore_broadcaster && m.roles.broadcaster {
        return filtered(state, &m, "broadcaster ignored");
    }
    let mut text = m.text.trim().to_string();
    match r.mode {
        ReadMode::Command => {
            let cmd = r.command.to_lowercase();
            let lower = text.to_lowercase();
            if !lower.starts_with(&format!("{cmd} ")) {
                return;
            }
            text = text[cmd.len()..].trim().to_string();
        }
        ReadMode::Privileged if !has_role(&m, &r.allowed_roles) => return filtered(state, &m, "no allowed role"),
        _ => {}
    }
    if r.mode != ReadMode::Command && r.ignore_prefixes.iter().any(|x| !x.is_empty() && text.starts_with(x.as_str())) {
        return filtered(state, &m, "command / prefix");
    }
    let now = Instant::now();
    if r.user_cooldown_secs > 0 {
        let cd = state.reader.cooldown.lock();
        if cd.get(&m.login).is_some_and(|t| now.duration_since(*t) < Duration::from_secs(r.user_cooldown_secs as u64)) {
            drop(cd);
            return filtered(state, &m, "cooldown");
        }
    }
    let emotes = &state.reader.emotes;
    let room = m.room_id.clone();
    let a = text::analyze(&text, &m.text, &m.emotes, m.bits, &|w| emotes.is_emote(&room, w));
    if a.has_link && p.text.links == crate::profile::LinkMode::Skip {
        return filtered(state, &m, "contains a link");
    }
    if a.emote_only && p.emotes.skip_emote_only {
        return filtered(state, &m, "emotes only");
    }
    let default = langs::intern(&p.language.default);
    let (lang, _how) = {
        let mut mem = state.reader.lang.lock();
        let ttl = Duration::from_secs(p.language.remember_minutes as u64 * 60);
        let lm = mem.entry(p.id.clone()).or_insert_with(|| LangMemory::new(ttl));
        lm.set_ttl(ttl);
        lm.choose(&a.plain, &m.login, default)
    };
    let Some(speech) = text::render(&a, lang, &p) else {
        return filtered(state, &m, "nothing to read (blocked word or only symbols)");
    };
    state.reader.cooldown.lock().insert(m.login.clone(), now);
    let cue = if p.voices.emotions { text::emotion::detect(&a, &text, &p.voices.emotion_strength) } else { None };
    let mut view = view_of(state, &m, speech, lang, ItemState::Queued, None);
    view.emotion = cue.map(|c| c.emotion.to_string());
    view.intensity = cue.map(|c| c.intensity.to_string());
    let item = Item {
        cue,
        name: text::spoken_name(&m.login, &m.display_name, &p),
        action: m.action,
        profile: p.id.clone(),
        user_id: m.user_id.clone(),
        msg_id: m.id.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        view,
    };
    if r.approval {
        // waits on the Live page until the streamer approves it
        let mut item = item;
        item.view.state = ItemState::Pending;
        record(state, item.view.clone());
        {
            let mut q = state.reader.pending.lock();
            q.push_back(item);
            while q.len() > r.max_queue.max(1) * 2 {
                if let Some(mut dropped) = q.pop_front() {
                    dropped.view.state = ItemState::Removed;
                    dropped.view.reason = Some("too many waiting for approval".into());
                    record(state, dropped.view);
                }
            }
        }
        publish(state);
        return;
    }
    record(state, item.view.clone());
    {
        let mut q = state.reader.queue.lock();
        q.push_back(item);
        while q.len() > r.max_queue {
            if let Some(mut dropped) = q.pop_front() {
                dropped.view.state = ItemState::Removed;
                dropped.view.reason = Some("queue full".into());
                record(state, dropped.view);
            }
        }
    }
    state.reader.wake.notify_waiters();
    publish(state);
}

// ── queue control ───────────────────────────────────────────────────────

/// Removes queued and in-flight messages matching (user_id, msg_id, login).
pub fn remove_where(state: &Arc<AppState>, pred: impl Fn(&str, &str, &str) -> bool, reason: &str) {
    let removed: Vec<Item> = {
        let mut q = state.reader.queue.lock();
        let (gone, keep): (Vec<Item>, Vec<Item>) = q.drain(..).partition(|i| pred(&i.user_id, &i.msg_id, &i.view.login));
        q.extend(keep);
        gone
    };
    for mut i in removed {
        i.view.state = ItemState::Removed;
        i.view.reason = Some(reason.into());
        record(state, i.view);
    }
    let current_hit = {
        let f = state.reader.inflight.lock();
        let mut hit = false;
        for (_, uid, mid, login, cancel) in f.iter() {
            if pred(uid, mid, login) {
                cancel.store(true, Ordering::Relaxed);
                hit = true;
            }
        }
        hit
    };
    if current_hit {
        // Only the current message is in the audio queue.
        if state.reader.status.read().current.as_ref().is_some_and(|c| state.reader.inflight.lock().iter().any(|(id, _, _, _, cancel)| *id == c.id && cancel.load(Ordering::Relaxed))) {
            state.audio.skip_all();
        }
    }
    publish(state);
}

pub fn skip(state: &Arc<AppState>) {
    let cur = state.reader.status.read().current.as_ref().map(|c| c.id);
    if let Some(id) = cur {
        for (iid, _, _, _, cancel) in state.reader.inflight.lock().iter() {
            if *iid == id {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    }
    state.audio.skip_all();
}

pub fn clear(state: &Arc<AppState>, by: &str) {
    let mut items: Vec<Item> = state.reader.queue.lock().drain(..).collect();
    items.extend(state.reader.pending.lock().drain(..));
    for mut i in items {
        i.view.state = ItemState::Removed;
        i.view.reason = Some(format!("queue cleared ({by})"));
        record(state, i.view);
    }
    for (_, _, _, _, cancel) in state.reader.inflight.lock().iter() {
        cancel.store(true, Ordering::Relaxed);
    }
    state.audio.skip_all();
    publish(state);
}

/// Approval mode: reads a waiting message (`id`, or the oldest).
pub fn approve(state: &Arc<AppState>, id: Option<u64>) -> bool {
    let item = {
        let mut p = state.reader.pending.lock();
        let at = match id {
            Some(id) => p.iter().position(|i| i.view.id == id),
            None => (!p.is_empty()).then_some(0),
        };
        at.and_then(|i| p.remove(i))
    };
    let Some(mut item) = item else { return false };
    item.view.state = ItemState::Queued;
    record(state, item.view.clone());
    state.reader.queue.lock().push_back(item);
    state.reader.wake.notify_waiters();
    publish(state);
    true
}

/// Approval mode: drops a waiting message (`id`, or the oldest).
pub fn reject(state: &Arc<AppState>, id: Option<u64>) -> bool {
    let item = {
        let mut p = state.reader.pending.lock();
        let at = match id {
            Some(id) => p.iter().position(|i| i.view.id == id),
            None => (!p.is_empty()).then_some(0),
        };
        at.and_then(|i| p.remove(i))
    };
    let Some(mut item) = item else { return false };
    item.view.state = ItemState::Removed;
    item.view.reason = Some("not approved".into());
    record(state, item.view);
    publish(state);
    true
}

/// Plays a spoken message again from memory (`id`, or the last one).
pub fn replay(state: &Arc<AppState>, id: Option<u64>) -> Result<(), String> {
    let found = {
        let r = state.reader.replays.lock();
        match id {
            Some(id) => r.iter().find(|(i, ..)| *i == id).cloned(),
            None => r.back().cloned(),
        }
    };
    let (_, rate, clips) = found.ok_or("Nothing to play again (only the last 20 messages are kept).")?;
    for c in clips {
        state.audio.play(c, rate, 1.0);
    }
    Ok(())
}

pub fn set_paused(state: &Arc<AppState>, on: bool) {
    state.reader.paused.store(on, Ordering::Relaxed);
    if !on {
        state.reader.wake.notify_waiters();
    }
    publish(state);
}

/// An event announcement goes straight into the queue (no approval needed),
/// read with joy by the announcer voice.
fn queue_event(state: &Arc<AppState>, p: &crate::profile::Profile, m: &ChatMessage, kind: &str, text: String) {
    let mut view = view_of(state, m, text, langs::intern(&p.language.default), ItemState::Queued, None);
    view.event = Some(kind.to_string());
    view.emotion = Some("joy".into());
    view.intensity = Some("mild".into());
    let item = Item {
        cue: Some(text::emotion::Cue { emotion: "joy", intensity: "mild" }),
        name: m.display_name.clone(),
        action: false,
        profile: p.id.clone(),
        user_id: format!("event-{}", m.user_id),
        msg_id: m.id.clone(),
        cancel: Arc::new(AtomicBool::new(false)),
        view,
    };
    record(state, item.view.clone());
    state.reader.queue.lock().push_back(item);
    state.reader.wake.notify_waiters();
    publish(state);
}

/// An example announcement, to hear how it sounds.
pub fn test_event(state: &Arc<AppState>, profile: &str, kind: &str) -> Result<(), String> {
    if !crate::announce::KINDS.contains(&kind) {
        return Err(format!("unknown event {kind}"));
    }
    let p = state.reader.store.read().profiles.iter().find(|x| x.id == profile).cloned().ok_or("unknown profile")?;
    let channel = p.channels.first().cloned().ok_or("Add a channel to this profile first.")?;
    let params: &[(&str, &str)] = match kind {
        "resub" => &[("cumulative-months", "12")],
        "subgift" => &[("recipient-display-name", "TestViewer")],
        "submysterygift" => &[("mass-gift-count", "5")],
        "raid" => &[("displayName", "TestRaider"), ("viewerCount", "42")],
        _ => &[],
    };
    let m = ChatMessage {
        channel,
        room_id: String::new(),
        id: format!("test-event-{}", state.reader.seq.load(Ordering::Relaxed)),
        login: "testuser".into(),
        display_name: "TestUser".into(),
        user_id: "test-testuser".into(),
        text: String::new(),
        action: false,
        roles: Default::default(),
        emotes: String::new(),
        bits: if kind == "bits" { 500 } else { 0 },
        reward_id: None,
        first_message: false,
        kind: if kind == "bits" { Kind::Chat } else { Kind::Notice },
        notice_type: (kind != "bits").then(|| kind.to_string()),
        system_msg: None,
        color: None,
        notice_params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    };
    // the example is read even while announcements are off
    let events = crate::profile::Announcements { enabled: true, subs: true, gifts: true, raids: true, bits: true, min_bits: 1, min_raid: 1, ..p.events.clone() };
    let (k, text) = crate::announce::for_message(&events, &m, langs::intern(&p.language.default)).ok_or("unknown event")?;
    queue_event(state, &p, &m, k, text);
    Ok(())
}

/// The announcer's voice: the one set in the profile, else the fallback voice.
fn announcer(state: &AppState, p: &crate::profile::Profile) -> Option<Picked> {
    let vc = state.reader.voices.read();
    let find = |key: &str| {
        let k = key.to_lowercase();
        vc.all.iter().find(|v| v.id.to_lowercase() == k || v.name.to_lowercase() == k).map(|v| Picked::from((v.id.clone(), v.name.clone())))
    };
    (!p.events.voice.is_empty())
        .then(|| find(&p.events.voice))
        .flatten()
        .or_else(|| p.voices.default_voice.as_deref().and_then(find))
        .or_else(|| vc.default_voice.as_deref().and_then(|d| find(d).or_else(|| Some((d.to_string(), d.to_string()).into()))))
}

/// Feeds a test message through the whole pipeline (as if typed in chat).
pub fn test_message(state: &Arc<AppState>, channel: &str, user: &str, text: &str) {
    let login = user.to_ascii_lowercase().replace(' ', "_");
    on_message(
        state,
        ChatMessage {
            channel: channel.to_string(),
            room_id: String::new(),
            id: format!("test-{}", state.reader.seq.load(Ordering::Relaxed)),
            login: login.clone(),
            display_name: user.to_string(),
            user_id: format!("test-{login}"),
            text: text.to_string(),
            action: false,
            roles: Default::default(),
            emotes: String::new(),
            bits: 0,
            reward_id: None,
            first_message: false,
            kind: Kind::Chat,
            notice_type: None,
            system_msg: None,
            color: None,
            notice_params: Default::default(),
        },
    );
}

// ── voices ──────────────────────────────────────────────────────────────

fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    // Numbers compare by value ("TTS-de 2" before "TTS-de 10").
    let key = |s: &str| {
        let mut parts: Vec<(String, u64)> = Vec::new();
        let mut buf = String::new();
        let mut num = String::new();
        for c in s.to_lowercase().chars() {
            if c.is_ascii_digit() {
                num.push(c);
            } else {
                if !num.is_empty() {
                    parts.push((std::mem::take(&mut buf), num.parse().unwrap_or(0)));
                    num.clear();
                }
                buf.push(c);
            }
        }
        parts.push((buf, num.parse().unwrap_or(0)));
        parts
    };
    key(a).cmp(&key(b))
}

async fn refresh_voices(state: &Arc<AppState>) {
    let Some(vcp) = state.vcp() else { return };
    let Ok(mut all) = vcp.voices().await else { return };
    all.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    let languages = vcp.languages().await.ok().map(|l| l.into_iter().flat_map(|x| [x.id.to_lowercase(), x.name.to_lowercase()]).collect());
    let default_voice = vcp.agent_settings().await.ok().and_then(|a| a.default_voice);
    let emotional = vcp.active_model().await.is_ok_and(|m| m.supports_emotional_clone);
    let prefix = state.reader.store.read().profiles.first().map(|p| p.voices.prefix.clone()).unwrap_or_else(|| "TTS-{lang} ".into());
    let mut pools: HashMap<String, Vec<ttv_vcp::Voice>> = HashMap::new();
    for l in langs::LANGS {
        let pre = prefix.replace("{lang}", l.code).to_lowercase();
        let pool: Vec<ttv_vcp::Voice> = all.iter().filter(|v| v.name.to_lowercase().starts_with(&pre)).cloned().collect();
        if !pool.is_empty() {
            pools.insert(l.code.to_string(), pool);
        }
    }
    log::info!("voices: {} in VCP, language pools: {}", all.len(), pools.iter().map(|(k, v)| format!("{k} {}", v.len())).collect::<Vec<_>>().join(", "));
    *state.reader.voices.write() = VoiceCache { all, pools, languages, default_voice, emotional, loaded: Some(Instant::now()) };
}

fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

// ── viewers ─────────────────────────────────────────────────────────────

/// Notes who wrote (cheap: one map entry per message, saved with the roles).
fn remember_viewer(state: &AppState, profile: &str, m: &ChatMessage) {
    if m.user_id.is_empty() || m.kind != Kind::Chat {
        return;
    }
    let mut a = state.reader.assign.lock();
    let v = a.viewers.entry(profile.to_string()).or_default().entry(m.user_id.clone()).or_default();
    v.login.clone_from(&m.login);
    v.display.clone_from(&m.display_name);
    if m.color.is_some() {
        v.color.clone_from(&m.color);
    }
    v.last_seen = chrono::Utc::now().timestamp();
    v.messages = v.messages.saturating_add(1);
    state.reader.assign_dirty.store(true, Ordering::Relaxed);
}

/// Name of the voice a role stands for in preset mode.
fn role_voice(state: &AppState, p: &crate::profile::Profile, role: u32) -> Option<String> {
    if p.voices.mode != VoiceMode::Preset {
        return None;
    }
    state
        .presets
        .with(&p.voices.preset, |preset| {
            let n = match p.voices.preset_voices {
                0 => preset.defs.len(),
                k => (k as usize).min(preset.defs.len()),
            };
            (n > 0).then(|| preset.defs[role as usize % n].name.clone())
        })
        .flatten()
}

/// The effect of the voice a role stands for (preset mode).
fn role_fx(state: &AppState, p: &crate::profile::Profile, role: u32) -> Option<String> {
    if p.voices.mode != VoiceMode::Preset {
        return None;
    }
    state
        .presets
        .with(&p.voices.preset, |preset| {
            let n = match p.voices.preset_voices {
                0 => preset.defs.len(),
                k => (k as usize).min(preset.defs.len()),
            };
            (n > 0).then(|| preset.defs[role as usize % n].fx.clone()).flatten()
        })
        .flatten()
}

/// The effect set for a viewer in the viewer list: Some(None) = switched
/// off, None = nothing set (the voice's own effect plays).
fn viewer_fx(state: &AppState, profile: &str, user_id: &str) -> Option<Option<crate::fx::Fx>> {
    let a = state.reader.assign.lock();
    let f = a.fx.get(profile)?.get(user_id)?;
    Some(crate::fx::Fx::parse(f))
}

/// Viewers of a profile, most recently active first. `query` filters by
/// name; at most `limit` rows (the list can hold thousands).
pub fn viewers(state: &AppState, profile: &str, query: &str, limit: usize) -> (Vec<ViewerView>, usize) {
    let Some(p) = state.reader.store.read().profiles.iter().find(|x| x.id == profile).cloned() else { return (Vec::new(), 0) };
    let q = query.trim().to_lowercase();
    let a = state.reader.assign.lock();
    let roles = a.roles.get(profile);
    let mut rows: Vec<(&String, &Viewer)> = a
        .viewers
        .get(profile)
        .map(|m| m.iter().filter(|(_, v)| q.is_empty() || v.login.contains(&q) || v.display.to_lowercase().contains(&q)).collect())
        .unwrap_or_default();
    let total = rows.len();
    rows.sort_unstable_by_key(|x| std::cmp::Reverse(x.1.last_seen));
    rows.truncate(limit.max(1));
    let fxs = a.fx.get(profile);
    let rows: Vec<(String, Viewer, Option<u32>, Option<String>)> = rows.into_iter().map(|(id, v)| (id.clone(), v.clone(), roles.and_then(|r| r.get(id).copied()), fxs.and_then(|m| m.get(id).cloned()))).collect();
    drop(a);
    let out = rows
        .into_iter()
        .map(|(user_id, info, role, fx)| ViewerView {
            voice: role.and_then(|r| role_voice(state, &p, r)),
            voice_fx: role.and_then(|r| role_fx(state, &p, r)),
            fx,
            pinned: p.voices.users.get(&info.login).cloned(),
            muted: p.muted.contains(&info.login),
            role,
            user_id,
            info,
        })
        .collect();
    (out, total)
}

/// Gives a viewer another voice role (or a fresh random one with None).
pub fn set_role(state: &AppState, profile: &str, user_id: &str, role: Option<u32>) {
    let mut a = state.reader.assign.lock();
    let map = a.roles.entry(profile.to_string()).or_default();
    match role {
        Some(r) => {
            map.insert(user_id.to_string(), r);
        }
        None => {
            map.remove(user_id);
        }
    }
    state.reader.assign_dirty.store(true, Ordering::Relaxed);
}

/// Sets a viewer's voice effect: a name, "none" (no effect) or None (the
/// voice's own effect).
pub fn set_fx(state: &AppState, profile: &str, user_id: &str, fx: Option<String>) {
    let mut a = state.reader.assign.lock();
    let map = a.fx.entry(profile.to_string()).or_default();
    match fx.map(|f| f.trim().to_ascii_lowercase()).filter(|f| f == "none" || crate::fx::Fx::parse(f).is_some()) {
        Some(f) => {
            map.insert(user_id.to_string(), f);
        }
        None => {
            map.remove(user_id);
        }
    }
    state.reader.assign_dirty.store(true, Ordering::Relaxed);
}

/// Forgets a viewer completely (voice role and list entry).
pub fn forget_viewer(state: &AppState, profile: &str, user_id: &str) {
    let mut a = state.reader.assign.lock();
    if let Some(m) = a.roles.get_mut(profile) {
        m.remove(user_id);
    }
    if let Some(m) = a.viewers.get_mut(profile) {
        m.remove(user_id);
    }
    state.reader.assign_dirty.store(true, Ordering::Relaxed);
}

/// A voice for one message.
struct Picked {
    id: String,
    name: String,
    /// Loudness correction measured when the voice was computed.
    gain: f32,
    /// The voice definition's effect.
    fx: Option<crate::fx::Fx>,
}

impl From<(String, String)> for Picked {
    fn from((id, name): (String, String)) -> Self {
        Self { id, name, gain: 1.0, fx: None }
    }
}

/// The voice for a viewer, following the profile's mode.
fn pick_voice(state: &AppState, p: &crate::profile::Profile, user_id: &str, login: &str, lang: &str) -> Option<Picked> {
    let vc = state.reader.voices.read();
    let find = |key: &str| {
        let k = key.to_lowercase();
        vc.all.iter().find(|v| v.id.to_lowercase() == k || v.name.to_lowercase() == k).map(|v| Picked::from((v.id.clone(), v.name.clone())))
    };
    if let Some(v) = p.voices.users.get(login).and_then(|k| find(k)) {
        return Some(v);
    }
    let excluded = |v: &&ttv_vcp::Voice| !p.voices.exclude.iter().any(|x| x.eq_ignore_ascii_case(&v.id) || x.eq_ignore_ascii_case(&v.name));
    let role = |len: usize| -> usize {
        let mut a = state.reader.assign.lock();
        let map = a.roles.entry(p.id.clone()).or_default();
        let idx = *map.entry(user_id.to_string()).or_insert_with(|| {
            state.reader.assign_dirty.store(true, Ordering::Relaxed);
            fnv1a(user_id) % len.max(1) as u32
        });
        idx as usize % len.max(1)
    };
    match p.voices.mode {
        VoiceMode::Preset => {
            let lib = &state.library;
            let fallback = p.voices.fallback_language.as_str();
            let entry = state
                .presets
                .with(&p.voices.preset, |preset| {
                    let n = match p.voices.preset_voices {
                        0 => preset.defs.len(),
                        k => (k as usize).min(preset.defs.len()),
                    };
                    if n == 0 {
                        return None;
                    }
                    let r = role(n);
                    let hash = preset.defs[r].hash();
                    let fx = |i: usize| preset.defs[i].fx.as_deref().and_then(crate::fx::Fx::parse);
                    let get = |l: &str| lib.get(&format!("{hash}:{l}"));
                    if let Some(e) = get(lang).or_else(|| get(fallback)) {
                        return Some((e, fx(r)));
                    }
                    // This viewer's voice isn't computed yet: a computed one
                    // of the same preset stands in until it is.
                    for l in [lang, fallback] {
                        let ready: Vec<usize> = (0..n).filter(|&i| lib.get(&preset.defs[i].key(l)).is_some()).collect();
                        if !ready.is_empty() {
                            let i = ready[r % ready.len()];
                            return lib.get(&preset.defs[i].key(l)).map(|e| (e, fx(i)));
                        }
                    }
                    None
                })
                .flatten();
            if let Some((e, fx)) = entry {
                return Some(Picked { id: e.voice_id, name: e.name, gain: e.gain, fx });
            }
        }
        VoiceMode::PerUserLanguage => {
            let pool: Vec<&ttv_vcp::Voice> = vc.pools.get(lang).or_else(|| vc.pools.get(&p.voices.fallback_language)).map(|v| v.iter().filter(excluded).collect()).unwrap_or_default();
            if !pool.is_empty() {
                let v = pool[role(pool.len())];
                return Some((v.id.clone(), v.name.clone()).into());
            }
        }
        VoiceMode::PerUser => {
            let pool: Vec<&ttv_vcp::Voice> = vc.all.iter().filter(excluded).collect();
            if !pool.is_empty() {
                let v = pool[role(pool.len())];
                return Some((v.id.clone(), v.name.clone()).into());
            }
        }
        VoiceMode::PerLanguage => {
            if let Some(v) = p.voices.per_language.get(lang).and_then(|k| find(k)) {
                return Some(v);
            }
        }
        VoiceMode::Single => {}
    }
    p.voices.default_voice.as_deref().and_then(find).or_else(|| vc.default_voice.as_deref().and_then(|d| find(d).or_else(|| Some((d.to_string(), d.to_string()).into()))))
}

// ── generator & player ──────────────────────────────────────────────────

async fn generator(state: Arc<AppState>) {
    let (tx, rx) = mpsc::channel::<Prepared>(1);
    tokio::spawn(play_loop(state.clone(), rx));
    loop {
        let next = {
            let paused = state.reader.paused.load(Ordering::Relaxed);
            if paused { None } else { state.reader.queue.lock().pop_front() }
        };
        let Some(mut item) = next else {
            tokio::select! {
                _ = state.reader.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
            continue;
        };
        publish(&state);
        let Some(p) = state.reader.store.read().profiles.iter().find(|x| x.id == item.profile).cloned() else { continue };
        if state.reader.voices.read().loaded.is_none() {
            refresh_voices(&state).await;
        }
        // Name announcement: when the speaker changes (or always / never).
        let recent_other = {
            let last = state.reader.last.lock();
            match &*last {
                Some((login, t)) => *login != item.view.login || t.elapsed() > Duration::from_secs(p.names.reset_secs as u64),
                None => true,
            }
        };
        let announce = match p.names.announce {
            Announce::Always => true,
            Announce::Never => false,
            Announce::OnChange => recent_other,
        };
        // translate for me: chat in a language the streamer doesn't speak
        let tr = &p.translate;
        let into = langs::intern(if tr.into.is_empty() { &p.language.default } else { &tr.into });
        let event = item.view.event.is_some();
        let mut translated = None;
        if tr.enabled && !event && item.view.lang != into && !tr.mine.contains(&item.view.lang) && !item.view.speech.trim().is_empty() {
            let email = state.cfg.read().translate_email.clone();
            let deepl = state.secrets.get(crate::secrets::DEEPL_KEY);
            let host = state.vcp();
            let svc = crate::translate::Service { provider: &tr.provider, email: &email, deepl_key: deepl.as_deref(), vcp: host.as_ref() };
            match state.translator.translate(&svc, &item.view.speech, &item.view.lang, into).await {
                Ok(t) => translated = Some(t),
                Err(e) => {
                    log::info!("translating failed: {e}");
                    item.view.reason = Some(format!("not translated: {e}"));
                }
            }
            item.view.translation = translated.clone();
        }
        // read: the original, the translation, or the original and then the translation
        let (lang, body, then): (String, String, Option<String>) = match (translated, tr.read.as_str()) {
            (Some(t), "translation") => (into.to_string(), t, None),
            (Some(t), "both") => (item.view.lang.clone(), item.view.speech.clone(), Some(t)),
            _ => (item.view.lang.clone(), item.view.speech.clone(), None),
        };
        let l = langs::get(&lang);
        let text = if event {
            body
        } else if item.action {
            format!("{} {}", item.name, body)
        } else if announce {
            l.map(|l| l.say).unwrap_or("{user}: {msg}").replace("{user}", &item.name).replace("{msg}", &body)
        } else {
            body
        };
        if !event {
            *state.reader.last.lock() = Some((item.view.login.clone(), Instant::now()));
        }
        let voice = if event { announcer(&state, &p) } else { pick_voice(&state, &p, &item.user_id, &item.view.login, &lang) };
        item.view.voice = voice.as_ref().map(|v| v.name.clone());
        let vcp_lang = {
            let vc = state.reader.voices.read();
            langs::vcp_name(&lang, vc.languages.as_deref(), &p.language.unsupported)
        };
        // the translation after the original: the viewer's voice in that language
        let then = then.and_then(|t| {
            let v = pick_voice(&state, &p, &item.user_id, &item.view.login, into)?;
            let vl = langs::vcp_name(into, state.reader.voices.read().languages.as_deref(), &p.language.unsupported);
            Some((t, v, vl))
        });
        let fallback = p.voices.os_fallback;
        let (vcp, voice) = match (state.vcp(), voice) {
            (Some(c), Some(v)) => (c, v),
            (host, _) => {
                let why = if host.is_none() { "no voice host" } else { "no voice found - set a default voice in VCP or create voices" };
                if !fallback {
                    item.view.state = ItemState::Failed;
                    item.view.reason = Some(why.into());
                    record(&state, item.view);
                    continue;
                }
                // the system's own voice reads instead
                item.view.voice = Some("system voice".into());
                item.view.reason = Some(format!("{why} – read by the system voice"));
                state.reader.inflight.lock().push((item.view.id, item.user_id.clone(), item.msg_id.clone(), item.view.login.clone(), item.cancel.clone()));
                let (ctx, crx) = mpsc::channel(8);
                let cancel = item.cancel.clone();
                if tx.send(Prepared { item, clips: crx, gain: 1.0, fx: None, fell_back: Arc::default() }).await.is_err() {
                    return;
                }
                for piece in speech::split(&text) {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let r = crate::os_voice::speak(&piece.0, &lang).await;
                    let failed = r.is_err();
                    if ctx.send(r).await.is_err() || failed {
                        break;
                    }
                }
                continue;
            }
        };
        let Picked { id: voice_id, gain, fx: voice_fx, .. } = voice;
        state.reader.inflight.lock().push((item.view.id, item.user_id.clone(), item.msg_id.clone(), item.view.login.clone(), item.cancel.clone()));
        let (ctx, crx) = mpsc::channel(8);
        let cancel = item.cancel.clone();
        let fx = if p.voices.effects { viewer_fx(&state, &p.id, &item.user_id).unwrap_or(voice_fx) } else { None };
        let cue = item.cue.filter(|_| state.reader.voices.read().emotional);
        let fell_back: Arc<Mutex<Option<String>>> = Arc::default();
        if tx.send(Prepared { item, clips: crx, gain, fx, fell_back: fell_back.clone() }).await.is_err() {
            return;
        }
        let mut u = speech::Utterance {
            text,
            voice: voice_id,
            language: vcp_lang,
            emotion: cue.map(|c| c.emotion.to_string()),
            intensity: cue.map(|c| c.intensity.to_string()),
            gain,
            fx: None,
        };
        let streaming = crate::bench::mode(&state) == crate::config::GenerationMode::Streaming;
        for piece in speech::split(&u.text) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if streaming {
                match speech::generate_streaming(&vcp, &u, &piece.0, &ctx).await {
                    Ok(_) => continue,
                    Err(e) if e.sent => {
                        let _ = ctx.send(Err(e.message)).await;
                        break;
                    }
                    // nothing played yet: this piece as a whole sentence
                    Err(e) => log::info!("streaming failed ({}) - whole sentence", e.message),
                }
            }
            let mut r = speech::generate(&vcp, &u, &piece.0).await;
            if let Err(e) = &r
                && u.emotion.is_some()
            {
                // an emotion VCP can't do must not cost the message
                log::info!("speaking with emotion {:?} failed ({e}) - without it", u.emotion);
                u.emotion = None;
                u.intensity = None;
                r = speech::generate(&vcp, &u, &piece.0).await;
            }
            if let Err(e) = &r
                && fallback
            {
                // the voice host can't speak right now: the system voice reads this piece
                log::info!("voice host failed ({e}) - system voice");
                fell_back.lock().get_or_insert_with(|| e.clone());
                r = crate::os_voice::speak(&piece.0, &lang).await.map(|(mut s, rate)| {
                    // the player applies the voice's loudness correction
                    if gain > 0.0 {
                        s.iter_mut().for_each(|x| *x /= gain);
                    }
                    (s, rate)
                });
            }
            let failed = r.is_err();
            if ctx.send(r).await.is_err() || failed {
                break;
            }
        }
        if let Some((t, v, vl)) = then {
            // its voice entry has its own loudness correction; the player applies the first one's
            let ratio = if gain > 0.0 { v.gain / gain } else { 1.0 };
            let u2 = speech::Utterance { text: t, voice: v.id, language: vl, emotion: u.emotion.clone(), intensity: u.intensity.clone(), gain: v.gain, fx: None };
            for piece in speech::split(&u2.text) {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let r = speech::generate(&vcp, &u2, &piece.0).await.map(|(mut s, rate)| {
                    if (ratio - 1.0).abs() > 1e-3 {
                        s.iter_mut().for_each(|x| *x *= ratio);
                    }
                    (s, rate)
                });
                let failed = r.is_err();
                if ctx.send(r).await.is_err() || failed {
                    break;
                }
            }
        }
    }
}

async fn play_loop(state: Arc<AppState>, mut rx: mpsc::Receiver<Prepared>) {
    while let Some(Prepared { mut item, mut clips, gain, fx, fell_back }) = rx.recv().await {
        let max_secs = state.reader.store.read().profiles.iter().find(|p| p.id == item.profile).map(|p| p.reading.max_seconds).unwrap_or(60) as f32;
        let t0 = Instant::now();
        item.view.state = ItemState::Speaking;
        state.reader.status.write().current = Some(item.view.clone());
        publish(&state);
        record(&state, item.view.clone());
        let mut last_clip = 0;
        let mut secs = 0f32;
        let mut error = None;
        let mut chain: Option<crate::fx::Chain> = None;
        // what was played, for "play again"
        let mut played: Vec<Vec<f32>> = Vec::new();
        let mut played_rate = 0;
        let stage = crate::plugin_cmds::Stage::new(&state).await;
        while let Some(r) = clips.recv().await {
            if item.cancel.load(Ordering::Relaxed) {
                break;
            }
            match r {
                Ok((samples, rate)) => {
                    let dur = samples.len() as f32 / rate.max(1) as f32;
                    if secs >= max_secs {
                        break;
                    }
                    secs += dur;
                    let out = match fx {
                        Some(f) => {
                            if chain.as_ref().is_some_and(|c| c.rate != rate) {
                                chain = None;
                            }
                            chain.get_or_insert_with(|| crate::fx::Chain::new(f, rate)).process(samples, gain)
                        }
                        None => {
                            let mut s = samples;
                            if gain != 1.0 {
                                s.iter_mut().for_each(|x| *x *= gain);
                            }
                            s
                        }
                    };
                    let (out, out_rate) = stage.run(&state, out, rate).await;
                    if played_rate == 0 || played_rate == out_rate {
                        played_rate = out_rate;
                        played.push(out.clone());
                    }
                    last_clip = state.audio.play(out, out_rate, 1.0);
                    if item.view.first_audio_ms.is_none() {
                        item.view.first_audio_ms = Some(t0.elapsed().as_millis() as u64);
                        state.reader.status.write().current = Some(item.view.clone());
                        publish(&state);
                    }
                }
                Err(e) => {
                    error = Some(e);
                    break;
                }
            }
        }
        if let Some(c) = chain
            && !item.cancel.load(Ordering::Relaxed)
        {
            let rate = c.rate;
            let tail = c.finish();
            if !tail.is_empty() {
                let (tail, tail_rate) = stage.run(&state, tail, rate).await;
                if played_rate == tail_rate {
                    played.push(tail.clone());
                }
                last_clip = state.audio.play(tail, tail_rate, 1.0);
            }
        }
        if !item.cancel.load(Ordering::Relaxed)
            && let Some((t, r)) = stage.tail(&state).await
        {
            if played_rate == r {
                played.push(t.clone());
            }
            last_clip = state.audio.play(t, r, 1.0);
        }
        if last_clip != 0 {
            state.audio.wait(last_clip).await;
        }
        let cancelled = item.cancel.load(Ordering::Relaxed);
        if let Some(why) = fell_back.lock().take() {
            item.view.voice = Some("system voice".into());
            item.view.reason = Some(format!("{why} – read by the system voice"));
        }
        item.view.state = if let Some(e) = &error {
            log::warn!("speaking failed: {e}");
            item.view.reason = Some(e.clone());
            ItemState::Failed
        } else if cancelled {
            ItemState::Skipped
        } else {
            ItemState::Spoken
        };
        state.reader.inflight.lock().retain(|(id, ..)| *id != item.view.id);
        if item.view.state == ItemState::Spoken && !played.is_empty() {
            let mut r = state.reader.replays.lock();
            r.push_back((item.view.id, played_rate, played));
            // the last 20 messages, at most 3 minutes of audio
            while r.len() > 20 || r.iter().map(|(_, rate, c)| c.iter().map(Vec::len).sum::<usize>() as f32 / *rate.max(&1) as f32).sum::<f32>() > 180.0 {
                r.pop_front();
            }
        }
        state.reader.status.write().current = None;
        record(&state, item.view);
        publish(&state);
        let gap = state.cfg.read().audio.gap_ms;
        tokio::time::sleep(Duration::from_millis(gap as u64)).await;
    }
}

async fn housekeeping(state: Arc<AppState>) {
    // every 10 s for (re)connects, the rest every minute
    let mut tick = tokio::time::interval(Duration::from_secs(10));
    let mut was_connected = false;
    let mut round = 0u32;
    loop {
        tick.tick().await;
        round += 1;
        let connected = state.host.read().connected;
        if connected && !was_connected {
            refresh_voices(&state).await;
            if !state.reader.store.read().active.is_empty() {
                let st = state.clone();
                tokio::spawn(async move { crate::bench::warm_up(&st).await });
            }
            if state.cfg.read().generation == crate::config::GenerationMode::Auto && crate::bench::current(&state).is_none() {
                tokio::spawn(measure_when_quiet(state.clone()));
            }
        }
        was_connected = connected;
        if round % 6 != 1 {
            continue;
        }
        state.reader.save_assignments();
        // Refresh voices every 10 min (and above when the host (re)connects).
        if connected && state.reader.voices.read().loaded.is_none_or(|t| t.elapsed() > Duration::from_secs(600)) {
            refresh_voices(&state).await;
        }
        for lm in state.reader.lang.lock().values_mut() {
            lm.prune();
        }
        let cutoff = Duration::from_secs(3600);
        state.reader.cooldown.lock().retain(|_, t| t.elapsed() < cutoff);
        let minutes: HashMap<String, u32> = state.reader.store.read().profiles.iter().map(|p| (p.id.clone(), p.reading.approval_minutes.max(1))).collect();
        let now = chrono::Utc::now().timestamp_millis();
        let expired: Vec<Item> = {
            let mut p = state.reader.pending.lock();
            let (old, keep): (Vec<Item>, Vec<Item>) = p.drain(..).partition(|i| now - i.view.at > *minutes.get(&i.profile).unwrap_or(&10) as i64 * 60_000);
            p.extend(keep);
            old
        };
        if !expired.is_empty() {
            for mut i in expired {
                i.view.state = ItemState::Removed;
                i.view.reason = Some("not approved in time".into());
                record(&state, i.view);
            }
            publish(&state);
        }
    }
}

/// "Automatic" generation: measure both ways once chat has been quiet for
/// two minutes (it keeps VCP busy for ~30 s on a slow setup).
async fn measure_when_quiet(state: Arc<AppState>) {
    for _ in 0..120 {
        tokio::time::sleep(Duration::from_secs(30)).await;
        if crate::bench::current(&state).is_some() || !state.host.read().connected {
            return;
        }
        let quiet = speech::last_vcp_use().is_none_or(|t| t.elapsed() > Duration::from_secs(120));
        if quiet && state.reader.queue.lock().is_empty() {
            if let Err(e) = crate::bench::run(&state).await {
                log::info!("generation benchmark: {e}");
            }
            return;
        }
    }
}

/// Saves pending data (call on exit).
pub fn flush(state: &AppState) {
    state.reader.save_assignments();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order() {
        let mut v = vec!["TTS-de 10 X", "TTS-de 2 Y", "TTS-de 1 Z"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["TTS-de 1 Z", "TTS-de 2 Y", "TTS-de 10 X"]);
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(fnv1a("12345"), fnv1a("12345"));
        assert_ne!(fnv1a("a"), fnv1a("b"));
    }
}

/// Live end-to-end test through a real proxy (plays two short messages):
///   TTV_LIVE_PROXY=192.168.178.25:8110 TTV_LIVE_TOKEN=<key or token>
///   cargo test -p ttv-tts live_reader -- --ignored --nocapture
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_reader_end_to_end() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let address = std::env::var("TTV_LIVE_PROXY").expect("TTV_LIVE_PROXY");
        let token = std::env::var("TTV_LIVE_TOKEN").expect("TTV_LIVE_TOKEN");
        let dir = std::env::temp_dir().join(format!("ttv-reader-live-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();

        let probe = crate::host::probe(&address).await.expect("probe");
        let mut cfg = crate::config::Config::default();
        cfg.normalize();
        cfg.audio.volume = 0.25;
        cfg.audio.gap_ms = 150;
        cfg.host = crate::config::HostConfig::Proxy { id: probe.info.id.clone(), name: probe.info.name.clone(), address: probe.address.clone(), fingerprint: probe.fingerprint.clone(), mac: None, guest: false };
        let state = crate::state::AppState::new(cfg, dir.join("config.json"), dir.clone());
        let token_key = crate::secrets::proxy_token(&probe.info.id);
        state.secrets.set(&token_key, Some(&token)).unwrap();
        state.host_updated();
        {
            let mut s = state.reader.store.write();
            s.profiles[0].channels = vec!["ttvtts_e2e_test".into()];
            s.profiles[0].language.default = "de".into();
            let id = s.profiles[0].id.clone();
            s.active = vec![id];
        }
        start(state.clone());
        let t0 = Instant::now();
        test_message(&state, "ttvtts_e2e_test", "Klaus_Dieter123", "Hallo zusammen, gg! Das war ein richtig gutes Spiel 😂😂");
        test_message(&state, "ttvtts_e2e_test", "Anya", "Привет всем, как дела у вас сегодня?");
        let done = |s: &ItemState| matches!(s, ItemState::Spoken | ItemState::Failed | ItemState::Skipped | ItemState::Removed);
        while t0.elapsed() < Duration::from_secs(150) {
            let h = state.reader.history();
            if h.len() >= 2 && h.iter().all(|i| done(&i.state)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let h = state.reader.history();
        for i in &h {
            println!("{:?} [{} · {:?} · first audio {:?} ms] {}", i.state, i.lang, i.voice, i.first_audio_ms, i.speech);
            if let Some(r) = &i.reason {
                println!("   reason: {r}");
            }
        }
        println!("total {} ms", t0.elapsed().as_millis());
        let _ = state.secrets.set(&token_key, None);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(h.len(), 2);
        assert!(h.iter().all(|i| i.state == ItemState::Spoken), "both messages spoken");
        assert_eq!(h[0].lang, "de");
        assert_eq!(h[1].lang, "ru");
        assert!(h[0].voice.as_deref().is_some_and(|v| v.starts_with("TTS-de")), "German pool voice");
        assert!(h[1].voice.as_deref().is_some_and(|v| v.starts_with("TTS-ru")), "Russian pool voice");
    }
}
