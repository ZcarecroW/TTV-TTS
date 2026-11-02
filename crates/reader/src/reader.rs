//! The chat reader: Twitch messages -> profile rules -> speech queue ->
//! generator (one message ahead) -> player.
//!
//! The generator prepares the next message (name announcement, voice,
//! language) and has VCP generate its pieces while the current one plays.
//! The player hands a message's pieces to the audio output and waits for
//! them; only the current message is ever inside the audio queue, so skips
//! and removals (timeouts, bans, deleted messages, mutes) are exact.
//!
//! Where the reader runs is the [`Host`]'s business: the app (VCP through the
//! proxy or on this PC, audio devices, plugins, its window) or the proxy (VCP
//! next door, OBS links). Each host keeps its profiles and viewers in its own
//! data folder.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, mpsc};

use crate::output::Output;
use crate::profile::{Announce, Profile, ReadMode, Role, VoiceMode};
use crate::speech;
use crate::text::{self, lang::LangMemory, langs};
use crate::translate::Translator;
use crate::twitch::emotes::{Emotes, Providers};
use crate::twitch::irc::{Chat, ChatEvent, ChatMessage, Kind};

const HISTORY: usize = 150;

// ── the host ────────────────────────────────────────────────────────────

/// Settings of the host that the reader reads for every message.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Pause between two messages (ms).
    pub gap_ms: u32,
    /// Generate with VCP's streaming (else sentence by sentence).
    pub streaming: bool,
    /// "Translate for me": e-mail for MyMemory's bigger free quota.
    pub translate_email: String,
}

/// What the reader needs from the program it runs in.
pub trait Host: Send + Sync + 'static {
    /// Speech generation (VCP directly or through the proxy); None = no host.
    fn vcp(&self) -> Option<ttv_vcp::Client>;
    /// The voice host answers (voices are loaded again when it comes back).
    fn connected(&self) -> bool;
    /// A computed preset voice by `VoiceDef::key` (definition hash + language).
    fn library_entry(&self, key: &str) -> Option<ttv_link::LibraryEntry>;
    /// Runs `f` with a preset's voice definitions; None if there is no such preset.
    fn with_preset<R>(&self, id: &str, f: impl FnOnce(&[ttv_link::VoiceDef]) -> R) -> Option<R>;
    /// Where a profile's voices play.
    fn output(&self, profile: &Profile) -> Arc<dyn Output>;
    /// This host reads the profile's chat (the app leaves profiles that the
    /// voice host PC reads for OBS to it).
    fn reads(&self, _profile: &Profile) -> bool {
        true
    }
    /// The reader's status changed (queue, current message, connection).
    fn status_changed(&self, status: &ReaderStatus);
    /// A message was added to the history or changed there.
    fn item_changed(&self, item: &ItemView);
    fn settings(&self) -> Settings;
    /// "Translate for me" with DeepL: the user's key (asked only when translating).
    fn deepl_key(&self) -> Option<String>;
    /// The mod command !volume.
    fn set_volume(&self, volume: f32);
    fn translator(&self) -> &Translator;
    /// The system's own voice (fallback when the voice host can't speak).
    fn os_speak(&self, text: &str, lang: &str) -> impl Future<Output = Result<(Vec<f32>, u32), String>> + Send;
    /// Audio plugins after the built-in effects, set up once per message.
    type Stage: Send + Sync;
    fn stage(&self) -> impl Future<Output = Self::Stage> + Send;
    fn stage_run(&self, stage: &Self::Stage, samples: Vec<f32>, rate: u32) -> impl Future<Output = (Vec<f32>, u32)> + Send;
    /// What the plugins still play after the last clip (reverb tails).
    fn stage_tail(&self, stage: &Self::Stage) -> impl Future<Output = Option<(Vec<f32>, u32)>> + Send;
}

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

/// A spoken message kept for "play again": (id, profile, rate, clips as played).
type Replay = (u64, String, u32, Vec<Vec<f32>>);

/// (id, user_id, msg_id, login, cancel) of a message being generated or played.
type Inflight = (u64, String, String, String, Arc<AtomicBool>);

struct Prepared {
    item: Item,
    /// Where it plays.
    out: Arc<dyn Output>,
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

/// The reader of one host: its profiles, viewers, queue and player.
pub struct Reader<H: Host> {
    pub host: H,
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
    /// The audio of the last spoken messages.
    replays: Mutex<VecDeque<Replay>>,
    /// Where the current message plays (for skipping).
    playing: Mutex<Option<Arc<dyn Output>>>,
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
    /// Where chat (and EventSub) events arrive.
    inbox: std::sync::OnceLock<mpsc::Sender<ChatEvent>>,
    /// Shut down for good (a removed OBS link).
    stopped: AtomicBool,
}

impl<H: Host> Reader<H> {
    /// Loads the profiles and viewers kept in `data_dir`. Call [`Reader::start`] to read chat.
    pub fn new(host: H, data_dir: &std::path::Path) -> Self {
        let store_path = data_dir.join("profiles.json");
        let assign_path = data_dir.join("assignments.json");
        let mut store: ProfileStore = crate::storage::read_json(&store_path).unwrap_or_default();
        if store.profiles.is_empty() {
            store.profiles.push(crate::profile::Profile::default());
        }
        store.profiles.iter_mut().for_each(|p| p.normalize());
        Self {
            host,
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
            playing: Mutex::new(None),
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
            inbox: std::sync::OnceLock::new(),
            stopped: AtomicBool::new(false),
        }
    }

    /// For other sources of chat events (EventSub).
    pub fn inbox(&self) -> Option<mpsc::Sender<ChatEvent>> {
        self.inbox.get().cloned()
    }

    pub fn save_store(&self) {
        let s = self.store.read().clone();
        if let Err(e) = crate::storage::write_json(&self.store_path, &s) {
            log::warn!("saving profiles failed: {e}");
        }
    }

    pub fn save_assignments(&self) {
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

    /// Messages waiting to be read.
    pub fn queue_len(&self) -> usize {
        self.queue.lock().len()
    }

    /// Stops reading for good: leaves the chat, ends the generator and the
    /// housekeeping, drops what's queued.
    pub fn shutdown(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        *self.chat.lock() = None;
        self.queue.lock().clear();
        self.pending.lock().clear();
        for (_, _, _, _, cancel) in self.inflight.lock().iter() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.wake.notify_waiters();
    }

    /// Takes over viewers' voices and effects from elsewhere (a profile that
    /// moves from the app to the voice host PC); viewers known here keep theirs.
    pub fn adopt_viewers(&self, profile: &str, roles: &HashMap<String, u32>, fx: &HashMap<String, String>) {
        if roles.is_empty() && fx.is_empty() {
            return;
        }
        let mut a = self.assign.lock();
        let r = a.roles.entry(profile.to_string()).or_default();
        for (user, role) in roles {
            r.entry(user.clone()).or_insert(*role);
        }
        let f = a.fx.entry(profile.to_string()).or_default();
        for (user, effect) in fx {
            f.entry(user.clone()).or_insert_with(|| effect.clone());
        }
        drop(a);
        self.assign_dirty.store(true, Ordering::Relaxed);
        self.save_assignments();
    }

    /// The viewers' voices and effects of a profile (to hand them over).
    pub fn viewer_roles(&self, profile: &str) -> (HashMap<String, u32>, HashMap<String, String>) {
        let a = self.assign.lock();
        (a.roles.get(profile).cloned().unwrap_or_default(), a.fx.get(profile).cloned().unwrap_or_default())
    }

    // ── start & events ──────────────────────────────────────────────────────

    /// Connects to Twitch chat and starts reading.
    pub fn start(self: Arc<Self>) {
        let (tx, mut rx) = mpsc::channel::<ChatEvent>(512);
        let _ = self.inbox.set(tx.clone());
        *self.chat.lock() = Some(Chat::start(tx));
        self.apply_channels();
        let st = self.clone();
        tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                st.handle_event(ev);
            }
        });
        self.run();
    }

    /// The generator, the player and the housekeeping, without chat (tests).
    pub fn run(self: Arc<Self>) {
        tokio::spawn(self.clone().generator());
        tokio::spawn(self.housekeeping());
    }

    /// Joins exactly the channels of the active profiles.
    pub fn apply_channels(self: &Arc<Self>) {
        let channels: BTreeSet<String> = {
            let s = self.store.read();
            s.profiles.iter().filter(|p| s.active.contains(&p.id) && self.host.reads(p)).flat_map(|p| p.channels.iter().cloned()).collect()
        };
        {
            let mut st = self.status.write();
            let old = std::mem::take(&mut st.channels);
            st.channels = channels
                .iter()
                .map(|c| old.iter().find(|o| &o.name == c).cloned().unwrap_or(ChannelState { name: c.clone(), ..Default::default() }))
                .collect();
        }
        if let Some(chat) = self.chat.lock().as_ref() {
            chat.set_channels(channels);
        }
        self.publish();
    }

    fn publish(self: &Arc<Self>) {
        let status = {
            let mut st = self.status.write();
            st.paused = self.paused.load(Ordering::Relaxed);
            st.queue = self.queue.lock().iter().map(|i| i.view.clone()).collect();
            st.pending = self.pending.lock().iter().map(|i| i.view.clone()).collect();
            st.replayable = self.replays.lock().iter().map(|(id, ..)| *id).collect();
            st.clone()
        };
        self.host.status_changed(&status);
    }

    fn record(self: &Arc<Self>, view: ItemView) {
        {
            let mut h = self.history.lock();
            if let Some(existing) = h.iter_mut().find(|x| x.id == view.id) {
                *existing = view.clone();
            } else {
                h.push_back(view.clone());
                while h.len() > HISTORY {
                    h.pop_front();
                }
            }
        }
        self.host.item_changed(&view);
    }

    fn profile_for(&self, channel: &str) -> Option<crate::profile::Profile> {
        let s = self.store.read();
        s.profiles.iter().find(|p| s.active.contains(&p.id) && self.host.reads(p) && p.channels.iter().any(|c| c == channel)).cloned()
    }

    fn handle_event(self: &Arc<Self>, ev: ChatEvent) {
        match ev {
            ChatEvent::Connection { connected, error } => {
                {
                    let mut st = self.status.write();
                    st.connected = connected;
                    st.error = error;
                    if !connected {
                        st.channels.iter_mut().for_each(|c| c.joined = false);
                    }
                }
                self.publish();
            }
            ChatEvent::Joined { channel, room_id } => {
                {
                    let mut st = self.status.write();
                    if let Some(c) = st.channels.iter_mut().find(|c| c.name == channel) {
                        c.joined = true;
                        c.room_id = Some(room_id.clone());
                    }
                }
                self.publish();
                log::info!("joined #{channel} (room {room_id})");
                if let Some(p) = self.profile_for(&channel) {
                    let st = self.clone();
                    let prov = Providers { bttv: p.emotes.bttv, ffz: p.emotes.ffz, seventv: p.emotes.seventv };
                    tokio::spawn(async move { st.emotes.ensure(&room_id, prov).await });
                }
            }
            ChatEvent::Message(m) => self.on_message(*m),
            ChatEvent::ClearUser { channel, user_id } => {
                if self.profile_for(&channel).is_some_and(|p| p.mod_commands.skip_on_timeout) {
                    match user_id {
                        Some(uid) => self.remove_where(|user, _, _| user == uid, "timeout/ban"),
                        None => self.clear("chat cleared"),
                    }
                }
            }
            ChatEvent::ClearMessage { channel, msg_id } => {
                if self.profile_for(&channel).is_some_and(|p| p.mod_commands.skip_on_timeout) {
                    self.remove_where(|_, mid, _| mid == msg_id, "message deleted");
                }
            }
        }
    }

    /// Mod commands (!skip, !ttsclear, …). Returns true if the message was one.
    fn mod_command(self: &Arc<Self>, p: &crate::profile::Profile, m: &ChatMessage) -> bool {
        let mc = &p.mod_commands;
        if !mc.enabled || !has_role(m, &mc.roles) {
            return false;
        }
        let mut words = m.text.split_whitespace();
        let cmd = words.next().unwrap_or("").to_lowercase();
        let arg = words.next().unwrap_or("").trim_start_matches('@').to_lowercase();
        let is = |c: &str| !c.is_empty() && cmd == c.to_lowercase();
        if is(&mc.skip) {
            self.skip();
        } else if is(&mc.clear) {
            self.clear(&m.display_name);
        } else if is(&mc.pause) {
            self.set_paused(true);
        } else if is(&mc.resume) {
            self.set_paused(false);
        } else if is(&mc.volume) && !arg.is_empty() {
            if let Ok(v) = arg.trim_end_matches('%').parse::<f32>() {
                self.host.set_volume((v / 100.0).clamp(0.0, 1.0));
            }
        } else if is(&mc.mute) && !arg.is_empty() {
            self.set_muted(&p.id, &arg, true);
        } else if is(&mc.unmute) && !arg.is_empty() {
            self.set_muted(&p.id, &arg, false);
        } else {
            return false;
        }
        log::info!("{} used {cmd} in #{}", m.display_name, m.channel);
        true
    }

    pub fn set_muted(self: &Arc<Self>, profile: &str, login: &str, on: bool) {
        {
            let mut s = self.store.write();
            if let Some(p) = s.profiles.iter_mut().find(|p| p.id == profile) {
                p.muted.retain(|x| x != login);
                if on {
                    p.muted.push(login.to_string());
                }
            }
        }
        self.save_store();
        if on {
            let login = login.to_string();
            self.remove_where(move |_, _, l| l == login, "muted");
        }
    }

    fn filtered(self: &Arc<Self>, m: &ChatMessage, reason: &str) {
        self.record(self.view_of(m, String::new(), "", ItemState::Filtered, Some(reason.to_string())));
    }

    fn view_of(&self, m: &ChatMessage, speech: String, lang: &str, s: ItemState, reason: Option<String>) -> ItemView {
        ItemView {
            id: self.seq.fetch_add(1, Ordering::Relaxed),
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

    fn on_message(self: &Arc<Self>, m: ChatMessage) {
        let Some(p) = self.profile_for(&m.channel) else { return };
        self.remember_viewer(&p.id, &m);
        if m.kind == Kind::Chat && self.mod_command(&p, &m) {
            return;
        }
        if let Some((kind, text)) = crate::announce::for_message(&p.events, &m, langs::intern(&p.language.default))
            // the reward that triggers TTS isn't announced on top
            && !(kind == "redeem" && !p.reading.reward_id.is_empty() && m.notice_params.get("reward-id") == Some(&p.reading.reward_id))
        {
            self.queue_event(&p, &m, kind, text);
        }
        let r = &p.reading;
        if m.kind == Kind::Notice && (!r.read_sub_messages || m.text.trim().is_empty()) {
            return;
        }
        if !r.reward_id.is_empty() && m.reward_id.as_deref() != Some(r.reward_id.as_str()) {
            return self.filtered(&m, "not the chosen channel-points reward");
        }
        if r.ignore_users.iter().any(|u| u.eq_ignore_ascii_case(&m.login)) {
            return self.filtered(&m, "ignored user (bot)");
        }
        if p.muted.contains(&m.login) {
            return self.filtered(&m, "muted");
        }
        if r.ignore_broadcaster && m.roles.broadcaster {
            return self.filtered(&m, "broadcaster ignored");
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
            ReadMode::Privileged if !has_role(&m, &r.allowed_roles) => return self.filtered(&m, "no allowed role"),
            _ => {}
        }
        if r.mode != ReadMode::Command && r.ignore_prefixes.iter().any(|x| !x.is_empty() && text.starts_with(x.as_str())) {
            return self.filtered(&m, "command / prefix");
        }
        let now = Instant::now();
        if r.user_cooldown_secs > 0 {
            let cd = self.cooldown.lock();
            if cd.get(&m.login).is_some_and(|t| now.duration_since(*t) < Duration::from_secs(r.user_cooldown_secs as u64)) {
                drop(cd);
                return self.filtered(&m, "cooldown");
            }
        }
        let emotes = &self.emotes;
        let room = m.room_id.clone();
        let a = text::analyze(&text, &m.text, &m.emotes, m.bits, &|w| emotes.is_emote(&room, w));
        if a.has_link && p.text.links == crate::profile::LinkMode::Skip {
            return self.filtered(&m, "contains a link");
        }
        if a.emote_only && p.emotes.skip_emote_only {
            return self.filtered(&m, "emotes only");
        }
        let default = langs::intern(&p.language.default);
        let (lang, _how) = {
            let mut mem = self.lang.lock();
            let ttl = Duration::from_secs(p.language.remember_minutes as u64 * 60);
            let lm = mem.entry(p.id.clone()).or_insert_with(|| LangMemory::new(ttl));
            lm.set_ttl(ttl);
            lm.choose(&a.plain, &m.login, default)
        };
        let Some(speech) = text::render(&a, lang, &p) else {
            return self.filtered(&m, "nothing to read (blocked word or only symbols)");
        };
        self.cooldown.lock().insert(m.login.clone(), now);
        let cue = if p.voices.emotions { text::emotion::detect(&a, &text, &p.voices.emotion_strength) } else { None };
        let mut view = self.view_of(&m, speech, lang, ItemState::Queued, None);
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
            self.record(item.view.clone());
            {
                let mut q = self.pending.lock();
                q.push_back(item);
                while q.len() > r.max_queue.max(1) * 2 {
                    if let Some(mut dropped) = q.pop_front() {
                        dropped.view.state = ItemState::Removed;
                        dropped.view.reason = Some("too many waiting for approval".into());
                        self.record(dropped.view);
                    }
                }
            }
            self.publish();
            return;
        }
        self.record(item.view.clone());
        {
            let mut q = self.queue.lock();
            q.push_back(item);
            while q.len() > r.max_queue {
                if let Some(mut dropped) = q.pop_front() {
                    dropped.view.state = ItemState::Removed;
                    dropped.view.reason = Some("queue full".into());
                    self.record(dropped.view);
                }
            }
        }
        self.wake.notify_waiters();
        self.publish();
    }

    // ── queue control ───────────────────────────────────────────────────────

    /// Removes queued and in-flight messages matching (user_id, msg_id, login).
    pub fn remove_where(self: &Arc<Self>, pred: impl Fn(&str, &str, &str) -> bool, reason: &str) {
        let removed: Vec<Item> = {
            let mut q = self.queue.lock();
            let (gone, keep): (Vec<Item>, Vec<Item>) = q.drain(..).partition(|i| pred(&i.user_id, &i.msg_id, &i.view.login));
            q.extend(keep);
            gone
        };
        for mut i in removed {
            i.view.state = ItemState::Removed;
            i.view.reason = Some(reason.into());
            self.record(i.view);
        }
        let current_hit = {
            let f = self.inflight.lock();
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
            if self.status.read().current.as_ref().is_some_and(|c| self.inflight.lock().iter().any(|(id, _, _, _, cancel)| *id == c.id && cancel.load(Ordering::Relaxed))) {
                self.skip_playing();
            }
        }
        self.publish();
    }

    pub fn skip(self: &Arc<Self>) {
        let cur = self.status.read().current.as_ref().map(|c| c.id);
        if let Some(id) = cur {
            for (iid, _, _, _, cancel) in self.inflight.lock().iter() {
                if *iid == id {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        }
        self.skip_playing();
    }

    /// Stops what plays right now (only the current message is ever queued at the output).
    fn skip_playing(&self) {
        if let Some(out) = self.playing.lock().as_ref() {
            out.skip_all();
        }
    }

    pub fn clear(self: &Arc<Self>, by: &str) {
        let mut items: Vec<Item> = self.queue.lock().drain(..).collect();
        items.extend(self.pending.lock().drain(..));
        for mut i in items {
            i.view.state = ItemState::Removed;
            i.view.reason = Some(format!("queue cleared ({by})"));
            self.record(i.view);
        }
        for (_, _, _, _, cancel) in self.inflight.lock().iter() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.skip_playing();
        self.publish();
    }

    /// Approval mode: reads a waiting message (`id`, or the oldest).
    pub fn approve(self: &Arc<Self>, id: Option<u64>) -> bool {
        let item = {
            let mut p = self.pending.lock();
            let at = match id {
                Some(id) => p.iter().position(|i| i.view.id == id),
                None => (!p.is_empty()).then_some(0),
            };
            at.and_then(|i| p.remove(i))
        };
        let Some(mut item) = item else { return false };
        item.view.state = ItemState::Queued;
        self.record(item.view.clone());
        self.queue.lock().push_back(item);
        self.wake.notify_waiters();
        self.publish();
        true
    }

    /// Approval mode: drops a waiting message (`id`, or the oldest).
    pub fn reject(self: &Arc<Self>, id: Option<u64>) -> bool {
        let item = {
            let mut p = self.pending.lock();
            let at = match id {
                Some(id) => p.iter().position(|i| i.view.id == id),
                None => (!p.is_empty()).then_some(0),
            };
            at.and_then(|i| p.remove(i))
        };
        let Some(mut item) = item else { return false };
        item.view.state = ItemState::Removed;
        item.view.reason = Some("not approved".into());
        self.record(item.view);
        self.publish();
        true
    }

    /// Plays a spoken message again from memory (`id`, or the last one).
    pub fn replay(self: &Arc<Self>, id: Option<u64>) -> Result<(), String> {
        let found = {
            let r = self.replays.lock();
            match id {
                Some(id) => r.iter().find(|(i, ..)| *i == id).cloned(),
                None => r.back().cloned(),
            }
        };
        let (_, profile, rate, clips) = found.ok_or("Nothing to play again (only the last 20 messages are kept).")?;
        let p = self.store.read().profiles.iter().find(|p| p.id == profile).cloned().ok_or("This profile doesn't exist any more.")?;
        let out = self.host.output(&p);
        for c in clips {
            out.play(c, rate);
        }
        Ok(())
    }

    pub fn set_paused(self: &Arc<Self>, on: bool) {
        self.paused.store(on, Ordering::Relaxed);
        if !on {
            self.wake.notify_waiters();
        }
        self.publish();
    }

    /// An event announcement goes straight into the queue (no approval needed),
    /// read with joy by the announcer voice.
    fn queue_event(self: &Arc<Self>, p: &crate::profile::Profile, m: &ChatMessage, kind: &str, text: String) {
        let mut view = self.view_of(m, text, langs::intern(&p.language.default), ItemState::Queued, None);
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
        self.record(item.view.clone());
        self.queue.lock().push_back(item);
        self.wake.notify_waiters();
        self.publish();
    }

    /// An example announcement, to hear how it sounds.
    pub fn test_event(self: &Arc<Self>, profile: &str, kind: &str) -> Result<(), String> {
        if !crate::announce::KINDS.contains(&kind) {
            return Err(format!("unknown event {kind}"));
        }
        let p = self.store.read().profiles.iter().find(|x| x.id == profile).cloned().ok_or("unknown profile")?;
        let channel = p.channels.first().cloned().ok_or("Add a channel to this profile first.")?;
        let params: &[(&str, &str)] = match kind {
            "resub" => &[("cumulative-months", "12")],
            "subgift" => &[("recipient-display-name", "TestViewer")],
            "submysterygift" => &[("mass-gift-count", "5")],
            "raid" => &[("displayName", "TestRaider"), ("viewerCount", "42")],
            "redeem" => &[("reward", "Hydrate!")],
            _ => &[],
        };
        let m = ChatMessage {
            channel,
            room_id: String::new(),
            id: format!("test-event-{}", self.seq.load(Ordering::Relaxed)),
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
        let events = crate::profile::Announcements { enabled: true, subs: true, gifts: true, raids: true, bits: true, follows: true, redemptions: true, min_bits: 1, min_raid: 1, ..p.events.clone() };
        let (k, text) = crate::announce::for_message(&events, &m, langs::intern(&p.language.default)).ok_or("unknown event")?;
        self.queue_event(&p, &m, k, text);
        Ok(())
    }

    /// The announcer's voice: the one set in the profile, else the fallback voice.
    fn announcer(&self, p: &crate::profile::Profile) -> Option<Picked> {
        let vc = self.voices.read();
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
    pub fn test_message(self: &Arc<Self>, channel: &str, user: &str, text: &str) {
        let login = user.to_ascii_lowercase().replace(' ', "_");
        self.on_message(
            ChatMessage {
                channel: channel.to_string(),
                room_id: String::new(),
                id: format!("test-{}", self.seq.load(Ordering::Relaxed)),
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

    async fn refresh_voices(self: &Arc<Self>) {
        let Some(vcp) = self.host.vcp() else { return };
        let Ok(mut all) = vcp.voices().await else { return };
        all.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        let languages = vcp.languages().await.ok().map(|l| l.into_iter().flat_map(|x| [x.id.to_lowercase(), x.name.to_lowercase()]).collect());
        let default_voice = vcp.agent_settings().await.ok().and_then(|a| a.default_voice);
        let emotional = vcp.active_model().await.is_ok_and(|m| m.supports_emotional_clone);
        let prefix = self.store.read().profiles.first().map(|p| p.voices.prefix.clone()).unwrap_or_else(|| "TTS-{lang} ".into());
        let mut pools: HashMap<String, Vec<ttv_vcp::Voice>> = HashMap::new();
        for l in langs::LANGS {
            let pre = prefix.replace("{lang}", l.code).to_lowercase();
            let pool: Vec<ttv_vcp::Voice> = all.iter().filter(|v| v.name.to_lowercase().starts_with(&pre)).cloned().collect();
            if !pool.is_empty() {
                pools.insert(l.code.to_string(), pool);
            }
        }
        log::info!("voices: {} in VCP, language pools: {}", all.len(), pools.iter().map(|(k, v)| format!("{k} {}", v.len())).collect::<Vec<_>>().join(", "));
        *self.voices.write() = VoiceCache { all, pools, languages, default_voice, emotional, loaded: Some(Instant::now()) };
    }

    // ── viewers ─────────────────────────────────────────────────────────────

    /// Notes who wrote (cheap: one map entry per message, saved with the roles).
    fn remember_viewer(&self, profile: &str, m: &ChatMessage) {
        if m.user_id.is_empty() || m.kind != Kind::Chat {
            return;
        }
        let mut a = self.assign.lock();
        let v = a.viewers.entry(profile.to_string()).or_default().entry(m.user_id.clone()).or_default();
        v.login.clone_from(&m.login);
        v.display.clone_from(&m.display_name);
        if m.color.is_some() {
            v.color.clone_from(&m.color);
        }
        v.last_seen = chrono::Utc::now().timestamp();
        v.messages = v.messages.saturating_add(1);
        self.assign_dirty.store(true, Ordering::Relaxed);
    }

    /// Name of the voice a role stands for in preset mode.
    fn role_voice(&self, p: &crate::profile::Profile, role: u32) -> Option<String> {
        if p.voices.mode != VoiceMode::Preset {
            return None;
        }
        self.host
            .with_preset(&p.voices.preset, |defs| {
                let n = match p.voices.preset_voices {
                    0 => defs.len(),
                    k => (k as usize).min(defs.len()),
                };
                (n > 0).then(|| defs[role as usize % n].name.clone())
            })
            .flatten()
    }

    /// The effect of the voice a role stands for (preset mode).
    fn role_fx(&self, p: &crate::profile::Profile, role: u32) -> Option<String> {
        if p.voices.mode != VoiceMode::Preset {
            return None;
        }
        self.host
            .with_preset(&p.voices.preset, |defs| {
                let n = match p.voices.preset_voices {
                    0 => defs.len(),
                    k => (k as usize).min(defs.len()),
                };
                (n > 0).then(|| defs[role as usize % n].fx.clone()).flatten()
            })
            .flatten()
    }

    /// The effect set for a viewer in the viewer list: Some(None) = switched
    /// off, None = nothing set (the voice's own effect plays).
    fn viewer_fx(&self, profile: &str, user_id: &str) -> Option<Option<crate::fx::Fx>> {
        let a = self.assign.lock();
        let f = a.fx.get(profile)?.get(user_id)?;
        Some(crate::fx::Fx::parse(f))
    }

    /// Viewers of a profile, most recently active first. `query` filters by
    /// name; at most `limit` rows (the list can hold thousands).
    pub fn viewers(&self, profile: &str, query: &str, limit: usize) -> (Vec<ViewerView>, usize) {
        let Some(p) = self.store.read().profiles.iter().find(|x| x.id == profile).cloned() else { return (Vec::new(), 0) };
        let q = query.trim().to_lowercase();
        let a = self.assign.lock();
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
                voice: role.and_then(|r| self.role_voice(&p, r)),
                voice_fx: role.and_then(|r| self.role_fx(&p, r)),
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
    pub fn set_role(&self, profile: &str, user_id: &str, role: Option<u32>) {
        let roles = self.preset_roles(profile);
        let mut a = self.assign.lock();
        let map = a.roles.entry(profile.to_string()).or_default();
        let r = match role {
            Some(r) => r,
            // Stored, not just removed: without a stored role the viewer's ID
            // would give the same voice again.
            None => other_role(map.get(user_id).copied().unwrap_or_else(|| fnv1a(user_id)), roles, rand::random()),
        };
        map.insert(user_id.to_string(), r);
        self.assign_dirty.store(true, Ordering::Relaxed);
    }

    /// How many voices the profile's preset offers (0 = not in preset mode).
    fn preset_roles(&self, profile: &str) -> u32 {
        let (preset, wanted) = {
            let s = self.store.read();
            match s.profiles.iter().find(|p| p.id == profile) {
                Some(p) if p.voices.mode == VoiceMode::Preset => (p.voices.preset.clone(), p.voices.preset_voices),
                _ => return 0,
            }
        };
        self.host.with_preset(&preset, |defs| if wanted == 0 { defs.len() } else { (wanted as usize).min(defs.len()) }).unwrap_or(0) as u32
    }

    /// Sets a viewer's voice effect: a name, "none" (no effect) or None (the
    /// voice's own effect).
    pub fn set_fx(&self, profile: &str, user_id: &str, fx: Option<String>) {
        let mut a = self.assign.lock();
        let map = a.fx.entry(profile.to_string()).or_default();
        match fx.map(|f| f.trim().to_ascii_lowercase()).filter(|f| f == "none" || crate::fx::Fx::parse(f).is_some()) {
            Some(f) => {
                map.insert(user_id.to_string(), f);
            }
            None => {
                map.remove(user_id);
            }
        }
        self.assign_dirty.store(true, Ordering::Relaxed);
    }

    /// Forgets a viewer completely (voice role and list entry).
    /// Forgets a viewer: gone from the list, a new random voice next time.
    pub fn forget_viewer(&self, profile: &str, user_id: &str) {
        self.set_role(profile, user_id, None);
        let mut a = self.assign.lock();
        if let Some(m) = a.viewers.get_mut(profile) {
            m.remove(user_id);
        }
        self.assign_dirty.store(true, Ordering::Relaxed);
    }

    /// The voice for a viewer, following the profile's mode.
    fn pick_voice(&self, p: &crate::profile::Profile, user_id: &str, login: &str, lang: &str) -> Option<Picked> {
        let vc = self.voices.read();
        let find = |key: &str| {
            let k = key.to_lowercase();
            vc.all.iter().find(|v| v.id.to_lowercase() == k || v.name.to_lowercase() == k).map(|v| Picked::from((v.id.clone(), v.name.clone())))
        };
        if let Some(v) = p.voices.users.get(login).and_then(|k| find(k)) {
            return Some(v);
        }
        let excluded = |v: &&ttv_vcp::Voice| !p.voices.exclude.iter().any(|x| x.eq_ignore_ascii_case(&v.id) || x.eq_ignore_ascii_case(&v.name));
        let role = |len: usize| -> usize {
            let mut a = self.assign.lock();
            let map = a.roles.entry(p.id.clone()).or_default();
            let idx = *map.entry(user_id.to_string()).or_insert_with(|| {
                self.assign_dirty.store(true, Ordering::Relaxed);
                fnv1a(user_id) % len.max(1) as u32
            });
            idx as usize % len.max(1)
        };
        match p.voices.mode {
            VoiceMode::Preset => {
                let fallback = p.voices.fallback_language.as_str();
                let entry = self.host
                    .with_preset(&p.voices.preset, |defs| {
                        let n = match p.voices.preset_voices {
                            0 => defs.len(),
                            k => (k as usize).min(defs.len()),
                        };
                        if n == 0 {
                            return None;
                        }
                        let r = role(n);
                        let hash = defs[r].hash();
                        let fx = |i: usize| defs[i].fx.as_deref().and_then(crate::fx::Fx::parse);
                        let get = |l: &str| self.host.library_entry(&format!("{hash}:{l}"));
                        if let Some(e) = get(lang).or_else(|| get(fallback)) {
                            return Some((e, fx(r)));
                        }
                        // This viewer's voice isn't computed yet: a computed one
                        // of the same preset stands in until it is.
                        for l in [lang, fallback] {
                            let ready: Vec<usize> = (0..n).filter(|&i| self.host.library_entry(&defs[i].key(l)).is_some()).collect();
                            if !ready.is_empty() {
                                let i = ready[r % ready.len()];
                                return self.host.library_entry(&defs[i].key(l)).map(|e| (e, fx(i)));
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

    async fn generator(self: Arc<Self>) {
        let (tx, rx) = mpsc::channel::<Prepared>(1);
        tokio::spawn(self.clone().play_loop(rx));
        loop {
            if self.stopped.load(Ordering::Relaxed) {
                return;
            }
            let next = {
                let paused = self.paused.load(Ordering::Relaxed);
                if paused { None } else { self.queue.lock().pop_front() }
            };
            let Some(mut item) = next else {
                tokio::select! {
                    _ = self.wake.notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                }
                continue;
            };
            self.publish();
            let Some(p) = self.store.read().profiles.iter().find(|x| x.id == item.profile).cloned() else { continue };
            let out = self.host.output(&p);
            if !out.listening() {
                // an OBS link nobody has open: don't make VCP work for nothing
                item.view.state = ItemState::Removed;
                item.view.reason = Some("nobody listens (OBS isn't connected)".into());
                self.record(item.view);
                continue;
            }
            if self.voices.read().loaded.is_none() {
                self.refresh_voices().await;
            }
            // Name announcement: when the speaker changes (or always / never).
            let recent_other = {
                let last = self.last.lock();
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
                let set = self.host.settings();
                let deepl = if tr.provider == "deepl" { self.host.deepl_key() } else { None };
                let host = self.host.vcp();
                let svc = crate::translate::Service { provider: &tr.provider, email: &set.translate_email, deepl_key: deepl.as_deref(), vcp: host.as_ref() };
                match self.host.translator().translate(&svc, &item.view.speech, &item.view.lang, into).await {
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
                *self.last.lock() = Some((item.view.login.clone(), Instant::now()));
            }
            let voice = if event { self.announcer(&p) } else { self.pick_voice(&p, &item.user_id, &item.view.login, &lang) };
            item.view.voice = voice.as_ref().map(|v| v.name.clone());
            let vcp_lang = {
                let vc = self.voices.read();
                langs::vcp_name(&lang, vc.languages.as_deref(), &p.language.unsupported)
            };
            // the translation after the original: the viewer's voice in that language
            let then = then.and_then(|t| {
                let v = self.pick_voice(&p, &item.user_id, &item.view.login, into)?;
                let vl = langs::vcp_name(into, self.voices.read().languages.as_deref(), &p.language.unsupported);
                Some((t, v, vl))
            });
            let fallback = p.voices.os_fallback;
            let (vcp, voice) = match (self.host.vcp(), voice) {
                (Some(c), Some(v)) => (c, v),
                (host, _) => {
                    let why = if host.is_none() { "no voice host" } else { "no voice found - set a default voice in VCP or create voices" };
                    if !fallback {
                        item.view.state = ItemState::Failed;
                        item.view.reason = Some(why.into());
                        self.record(item.view);
                        continue;
                    }
                    // the system's own voice reads instead
                    item.view.voice = Some("system voice".into());
                    item.view.reason = Some(format!("{why} – read by the system voice"));
                    self.inflight.lock().push((item.view.id, item.user_id.clone(), item.msg_id.clone(), item.view.login.clone(), item.cancel.clone()));
                    let (ctx, crx) = mpsc::channel(8);
                    let cancel = item.cancel.clone();
                    if tx.send(Prepared { item, out: out.clone(), clips: crx, gain: 1.0, fx: None, fell_back: Arc::default() }).await.is_err() {
                        return;
                    }
                    for piece in speech::split(&text) {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let r = self.host.os_speak(&piece.0, &lang).await;
                        let failed = r.is_err();
                        if ctx.send(r).await.is_err() || failed {
                            break;
                        }
                    }
                    continue;
                }
            };
            let Picked { id: voice_id, gain, fx: voice_fx, .. } = voice;
            self.inflight.lock().push((item.view.id, item.user_id.clone(), item.msg_id.clone(), item.view.login.clone(), item.cancel.clone()));
            let (ctx, crx) = mpsc::channel(8);
            let cancel = item.cancel.clone();
            let fx = if p.voices.effects { self.viewer_fx(&p.id, &item.user_id).unwrap_or(voice_fx) } else { None };
            let cue = item.cue.filter(|_| self.voices.read().emotional);
            let fell_back: Arc<Mutex<Option<String>>> = Arc::default();
            if tx.send(Prepared { item, out, clips: crx, gain, fx, fell_back: fell_back.clone() }).await.is_err() {
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
            let streaming = self.host.settings().streaming;
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
                    r = self.host.os_speak(&piece.0, &lang).await.map(|(mut s, rate)| {
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

    async fn play_loop(self: Arc<Self>, mut rx: mpsc::Receiver<Prepared>) {
        while let Some(Prepared { mut item, out: output, mut clips, gain, fx, fell_back }) = rx.recv().await {
            let max_secs = self.store.read().profiles.iter().find(|p| p.id == item.profile).map(|p| p.reading.max_seconds).unwrap_or(60) as f32;
            let t0 = Instant::now();
            item.view.state = ItemState::Speaking;
            self.status.write().current = Some(item.view.clone());
            *self.playing.lock() = Some(output.clone());
            self.publish();
            self.record(item.view.clone());
            let mut last_clip = 0;
            let mut secs = 0f32;
            let mut error = None;
            let mut chain: Option<crate::fx::Chain> = None;
            // what was played, for "play again"
            let mut played: Vec<Vec<f32>> = Vec::new();
            let mut played_rate = 0;
            let stage = self.host.stage().await;
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
                        let (out, out_rate) = self.host.stage_run(&stage, out, rate).await;
                        if played_rate == 0 || played_rate == out_rate {
                            played_rate = out_rate;
                            played.push(out.clone());
                        }
                        last_clip = output.play(out, out_rate);
                        if item.view.first_audio_ms.is_none() {
                            item.view.first_audio_ms = Some(t0.elapsed().as_millis() as u64);
                            self.status.write().current = Some(item.view.clone());
                            self.publish();
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
                    let (tail, tail_rate) = self.host.stage_run(&stage, tail, rate).await;
                    if played_rate == tail_rate {
                        played.push(tail.clone());
                    }
                    last_clip = output.play(tail, tail_rate);
                }
            }
            if !item.cancel.load(Ordering::Relaxed)
                && let Some((t, r)) = self.host.stage_tail(&stage).await
            {
                if played_rate == r {
                    played.push(t.clone());
                }
                last_clip = output.play(t, r);
            }
            if last_clip != 0 {
                crate::output::wait(&*output, last_clip).await;
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
            self.inflight.lock().retain(|(id, ..)| *id != item.view.id);
            if item.view.state == ItemState::Spoken && !played.is_empty() {
                let mut r = self.replays.lock();
                r.push_back((item.view.id, item.profile.clone(), played_rate, played));
                // the last 20 messages, at most 3 minutes of audio
                while r.len() > 20 || r.iter().map(|(_, _, rate, c)| c.iter().map(Vec::len).sum::<usize>() as f32 / *rate.max(&1) as f32).sum::<f32>() > 180.0 {
                    r.pop_front();
                }
            }
            self.status.write().current = None;
            *self.playing.lock() = None;
            self.record(item.view);
            self.publish();
            let gap = self.host.settings().gap_ms;
            tokio::time::sleep(Duration::from_millis(gap as u64)).await;
        }
    }

    async fn housekeeping(self: Arc<Self>) {
        // every 10 s for (re)connects, the rest every minute
        let mut tick = tokio::time::interval(Duration::from_secs(10));
        let mut was_connected = false;
        let mut round = 0u32;
        loop {
            if self.stopped.load(Ordering::Relaxed) {
                return;
            }
            tick.tick().await;
            round += 1;
            let connected = self.host.connected();
            if connected && !was_connected {
                self.refresh_voices().await;
            }
            was_connected = connected;
            if round % 6 != 1 {
                continue;
            }
            self.save_assignments();
            // Refresh voices every 10 min (and above when the host (re)connects).
            if connected && self.voices.read().loaded.is_none_or(|t| t.elapsed() > Duration::from_secs(600)) {
                self.refresh_voices().await;
            }
            for lm in self.lang.lock().values_mut() {
                lm.prune();
            }
            let cutoff = Duration::from_secs(3600);
            self.cooldown.lock().retain(|_, t| t.elapsed() < cutoff);
            let minutes: HashMap<String, u32> = self.store.read().profiles.iter().map(|p| (p.id.clone(), p.reading.approval_minutes.max(1))).collect();
            let now = chrono::Utc::now().timestamp_millis();
            let expired: Vec<Item> = {
                let mut p = self.pending.lock();
                let (old, keep): (Vec<Item>, Vec<Item>) = p.drain(..).partition(|i| now - i.view.at > *minutes.get(&i.profile).unwrap_or(&10) as i64 * 60_000);
                p.extend(keep);
                old
            };
            if !expired.is_empty() {
                for mut i in expired {
                    i.view.state = ItemState::Removed;
                    i.view.reason = Some("not approved in time".into());
                    self.record(i.view);
                }
                self.publish();
            }
        }
    }

    /// Saves pending data (call on exit).
    pub fn flush(&self) {
        self.save_assignments();
    }
}

// ── helpers ─────────────────────────────────────────────────────────────

fn has_role(m: &ChatMessage, roles: &[Role]) -> bool {
    roles.iter().any(|r| match r {
        Role::Broadcaster => m.roles.broadcaster,
        Role::Moderator => m.roles.moderator,
        Role::Vip => m.roles.vip,
        Role::Subscriber => m.roles.subscriber,
        Role::Everyone => true,
    })
}

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

fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    h
}

/// A role other than `current` among `n` (any role when n is unknown).
fn other_role(current: u32, n: u32, random: u32) -> u32 {
    if n > 1 { (current % n + 1 + random % (n - 1)) % n } else { random }
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

    #[test]
    fn a_new_random_voice_is_another_one() {
        for n in [2u32, 3, 30, 240] {
            for current in [0u32, 1, n - 1, fnv1a("viewer"), u32::MAX] {
                for r in [0u32, 1, 7, n, u32::MAX] {
                    let new = other_role(current, n, r);
                    assert_ne!(new % n, current % n, "n {n}, current {current}, random {r}");
                    assert!(new < n);
                }
            }
        }
        assert_eq!(other_role(5, 0, 42), 42, "unknown number of voices: just random");
    }

    // ── a host without VCP: the pipeline, the system voice and OBS links ──

    #[derive(Default)]
    struct FakeOut {
        deaf: AtomicBool,
        clips: Mutex<Vec<usize>>,
        skips: AtomicU64,
    }

    impl Output for FakeOut {
        fn play(&self, samples: Vec<f32>, _rate: u32) -> u64 {
            let mut c = self.clips.lock();
            c.push(samples.len());
            c.len() as u64
        }
        fn is_finished(&self, _id: u64) -> bool {
            true
        }
        fn skip_all(&self) {
            self.skips.fetch_add(1, Ordering::Relaxed);
        }
        fn listening(&self) -> bool {
            !self.deaf.load(Ordering::Relaxed)
        }
    }

    struct FakeHost {
        out: Arc<FakeOut>,
        translator: Translator,
    }

    impl Host for FakeHost {
        fn vcp(&self) -> Option<ttv_vcp::Client> {
            None
        }
        fn connected(&self) -> bool {
            false
        }
        fn library_entry(&self, _key: &str) -> Option<ttv_link::LibraryEntry> {
            None
        }
        fn with_preset<R>(&self, _id: &str, _f: impl FnOnce(&[ttv_link::VoiceDef]) -> R) -> Option<R> {
            None
        }
        fn output(&self, _profile: &Profile) -> Arc<dyn Output> {
            self.out.clone()
        }
        fn status_changed(&self, _status: &ReaderStatus) {}
        fn item_changed(&self, _item: &ItemView) {}
        fn settings(&self) -> Settings {
            Settings { gap_ms: 0, ..Default::default() }
        }
        fn deepl_key(&self) -> Option<String> {
            None
        }
        fn set_volume(&self, _volume: f32) {}
        fn translator(&self) -> &Translator {
            &self.translator
        }
        async fn os_speak(&self, text: &str, _lang: &str) -> Result<(Vec<f32>, u32), String> {
            // 10 ms of "speech" per letter
            Ok((vec![0.1; text.chars().count() * 240], 24_000))
        }
        type Stage = ();
        async fn stage(&self) {}
        async fn stage_run(&self, _stage: &(), samples: Vec<f32>, rate: u32) -> (Vec<f32>, u32) {
            (samples, rate)
        }
        async fn stage_tail(&self, _stage: &()) -> Option<(Vec<f32>, u32)> {
            None
        }
    }

    fn reader(fallback: bool) -> (Arc<Reader<FakeHost>>, Arc<FakeOut>) {
        let dir = std::env::temp_dir().join(format!("ttv-reader-test-{}", rand::random::<u32>()));
        let out = Arc::new(FakeOut::default());
        let r = Arc::new(Reader::new(FakeHost { out: out.clone(), translator: Translator::new(reqwest::Client::new()) }, &dir));
        {
            let mut s = r.store.write();
            let p = &mut s.profiles[0];
            p.channels = vec!["test".into()];
            p.voices.os_fallback = fallback;
            p.reading.user_cooldown_secs = 0;
            let id = p.id.clone();
            s.active = vec![id];
        }
        r.clone().run();
        (r, out)
    }

    async fn settled(r: &Reader<FakeHost>, n: usize) -> Vec<ItemView> {
        let t = Instant::now();
        loop {
            let h = r.history();
            if h.len() >= n && h.iter().all(|i| !matches!(i.state, ItemState::Queued | ItemState::Speaking)) || t.elapsed() > Duration::from_secs(10) {
                return h;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn without_a_voice_host_a_message_fails_or_the_system_voice_reads() {
        let (r, out) = reader(false);
        r.test_message("test", "Viewer", "Hello there, how are you?");
        let h = settled(&r, 1).await;
        assert_eq!(h[0].state, ItemState::Failed, "{h:?}");
        assert_eq!(h[0].reason.as_deref(), Some("no voice host"));
        assert!(out.clips.lock().is_empty());

        let (r, out) = reader(true);
        r.test_message("test", "Viewer", "Hello there, how are you?");
        let h = settled(&r, 1).await;
        assert_eq!(h[0].state, ItemState::Spoken, "{h:?}");
        assert_eq!(h[0].voice.as_deref(), Some("system voice"));
        assert!(!out.clips.lock().is_empty());
        assert!(r.replay(None).is_ok(), "a spoken message can be played again");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn nobody_listening_means_no_work() {
        let (r, out) = reader(true);
        out.deaf.store(true, Ordering::Relaxed);
        r.test_message("test", "Viewer", "Is anybody out there?");
        let h = settled(&r, 1).await;
        assert_eq!(h[0].state, ItemState::Removed, "{h:?}");
        assert!(h[0].reason.as_deref().is_some_and(|x| x.contains("nobody listens")));
        assert!(out.clips.lock().is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn filters_apply_before_the_queue() {
        let (r, _out) = reader(true);
        r.store.write().profiles[0].reading.ignore_users = vec!["nightbot".into()];
        r.test_message("test", "Nightbot", "Follow the rules!");
        r.test_message("test", "Viewer", "!points");
        let h = settled(&r, 2).await;
        assert_eq!(h.len(), 2);
        assert!(h.iter().all(|i| i.state == ItemState::Filtered), "{h:?}");
        assert_eq!(h[0].reason.as_deref(), Some("ignored user (bot)"));
        assert_eq!(h[1].reason.as_deref(), Some("command / prefix"));
    }
}
