//! Profiles: one per streamer / situation (own channel, a friend's channel,
//! co-streams). A profile says which channels are read and how. Defaults are
//! the tried settings of TTV-TTS 1.0.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReadMode {
    /// Every message.
    #[default]
    All,
    /// Only "!tts text".
    Command,
    /// Only viewers with one of `allowed_roles`.
    Privileged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Broadcaster,
    Moderator,
    Vip,
    Subscriber,
    Everyone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Reading {
    pub mode: ReadMode,
    pub command: String,
    pub allowed_roles: Vec<Role>,
    /// Only messages redeemed with this channel-points reward (empty = any).
    pub reward_id: String,
    pub ignore_broadcaster: bool,
    pub ignore_users: Vec<String>,
    pub ignore_prefixes: Vec<String>,
    /// Read the text viewers attach to subs/resubs.
    pub read_sub_messages: bool,
    pub max_chars: usize,
    pub max_seconds: u32,
    pub user_cooldown_secs: u32,
    pub max_queue: usize,
    /// Messages wait on the Live page until the streamer approves them.
    pub approval: bool,
    /// Not approved within this many minutes: dropped.
    pub approval_minutes: u32,
}

impl Default for Reading {
    fn default() -> Self {
        Self {
            mode: ReadMode::All,
            command: "!tts".into(),
            allowed_roles: vec![Role::Broadcaster, Role::Moderator, Role::Vip, Role::Subscriber],
            reward_id: String::new(),
            ignore_broadcaster: false,
            ignore_users: [
                "nightbot", "streamelements", "streamlabs", "moobot", "fossabot", "wizebot", "sery_bot", "soundalerts", "botrixoficial",
                "kofistreambot", "blerp", "pokemoncommunitygame", "tangiabot", "streamstickers", "lumiastream", "commanderroot",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            ignore_prefixes: vec!["!".into()],
            read_sub_messages: true,
            max_chars: 400,
            max_seconds: 60,
            user_cooldown_secs: 0,
            max_queue: 15,
            approval: false,
            approval_minutes: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Announce {
    /// "Max says: …" only when someone else spoke before.
    #[default]
    OnChange,
    Always,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Names {
    pub announce: Announce,
    /// After this much silence the name is announced again.
    pub reset_secs: u32,
    /// "max_mustermann123" -> "max mustermann"
    pub cleanup: bool,
    /// login -> how to pronounce it
    pub aliases: BTreeMap<String, String>,
}

impl Default for Names {
    fn default() -> Self {
        Self { announce: Announce::OnChange, reset_secs: 90, cleanup: true, aliases: BTreeMap::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LanguageRules {
    /// Used when a message's language isn't clear ("gg", "ok").
    pub default: String,
    /// Detected but not spoken by the model: "auto" (VCP decides) or a code.
    pub unsupported: String,
    pub remember_minutes: u32,
}

impl Default for LanguageRules {
    fn default() -> Self {
        Self { default: "en".into(), unsupported: "auto".into(), remember_minutes: 15 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmoteMode {
    /// Every emote once ("KEKW KEKW KEKW" -> "kek").
    #[default]
    Once,
    Remove,
    Keep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Emotes {
    pub mode: EmoteMode,
    pub max_per_message: usize,
    /// "PepeLaugh" -> "Pepe Laugh"
    pub split_words: bool,
    pub skip_emote_only: bool,
    pub bttv: bool,
    pub ffz: bool,
    pub seventv: bool,
    /// emote -> pronunciation ("" = silent), case-insensitive
    pub aliases: BTreeMap<String, String>,
}

impl Default for Emotes {
    fn default() -> Self {
        let aliases: &[(&str, &str)] = &[
            ("KEKW", "kek"), ("OMEGALUL", "omega lul"), ("LUL", "lul"), ("LULW", "lul"), ("Pog", "pog"), ("PogU", "pog"), ("POGGERS", "poggers"),
            ("PogChamp", "pog champ"), ("monkaS", "monka"), ("monkaW", "monka"), ("Sadge", "sadge"), ("4Head", "for head"), ("5Head", "five head"),
            ("EZ", "easy"), ("Kappa", "kappa"), ("xD", "x d"), (":)", ""), (":(", ""), (":D", ""), (":P", ""), (";)", ""), (";P", ""), (":O", ""),
            (":/", ""), (":|", ""), (":\\", ""), ("B)", ""), ("O_o", ""), ("o_O", ""), ("R)", ""), (">(", ""), ("<3", ""),
        ];
        Self {
            mode: EmoteMode::Once,
            max_per_message: 3,
            split_words: true,
            skip_emote_only: false,
            bttv: true,
            ffz: true,
            seventv: true,
            aliases: aliases.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmojiMode {
    /// Known emojis once as a word ("😂😂" -> "laughing").
    #[default]
    Describe,
    Remove,
    Keep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Emojis {
    pub mode: EmojiMode,
    pub max_per_message: usize,
}

impl Default for Emojis {
    fn default() -> Self {
        Self { mode: EmojiMode::Describe, max_per_message: 2 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LinkMode {
    /// Say "link".
    #[default]
    Replace,
    Remove,
    /// Ignore the whole message.
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MentionMode {
    #[default]
    Name,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockedMode {
    /// Drop the message.
    #[default]
    Skip,
    /// Replace the word with "beep".
    Beep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TextRules {
    pub links: LinkMode,
    pub mentions: MentionMode,
    pub collapse_repeats: bool,
    pub fix_shouting: bool,
    /// language ("all" or code) -> whole word -> replacement
    pub replacements: BTreeMap<String, BTreeMap<String, String>>,
    pub blocked_words: Vec<String>,
    pub blocked_mode: BlockedMode,
}

impl Default for TextRules {
    fn default() -> Self {
        let mk = |pairs: &[(&str, &str)]| pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<BTreeMap<_, _>>();
        let mut r = BTreeMap::new();
        r.insert("all".into(), mk(&[("w/", "with")]));
        r.insert(
            "de".into(),
            mk(&[("gg", "gut gespielt"), ("wp", "well played"), ("brb", "bin gleich zurück"), ("afk", "a f k"), ("vllt", "vielleicht"), ("iwie", "irgendwie"), ("kp", "keine Ahnung"), ("mmn", "meiner Meinung nach")]),
        );
        r.insert(
            "en".into(),
            mk(&[("gg", "good game"), ("wp", "well played"), ("brb", "be right back"), ("idk", "I don't know"), ("imo", "in my opinion"), ("tbh", "to be honest"), ("ty", "thank you"), ("np", "no problem")]),
        );
        r.insert("ru".into(), mk(&[("спс", "спасибо"), ("пж", "пожалуйста"), ("хз", "не знаю")]));
        Self { links: LinkMode::Replace, mentions: MentionMode::Name, collapse_repeats: true, fix_shouting: true, replacements: r, blocked_words: Vec::new(), blocked_mode: BlockedMode::Skip }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModCommands {
    pub enabled: bool,
    pub roles: Vec<Role>,
    pub skip: String,
    pub clear: String,
    pub pause: String,
    pub resume: String,
    pub volume: String,
    pub mute: String,
    pub unmute: String,
    /// Timeouts, bans and deleted messages leave the queue.
    pub skip_on_timeout: bool,
}

impl Default for ModCommands {
    fn default() -> Self {
        Self {
            enabled: true,
            roles: vec![Role::Broadcaster, Role::Moderator],
            skip: "!skip".into(),
            clear: "!ttsclear".into(),
            pause: "!ttspause".into(),
            resume: "!ttsresume".into(),
            volume: "!ttsvol".into(),
            mute: "!ttsmute".into(),
            unmute: "!ttsunmute".into(),
            skip_on_timeout: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMode {
    /// Every viewer keeps a voice of a preset, spoken in the detected
    /// language by that voice's computed version for the language.
    #[default]
    Preset,
    /// Every viewer keeps a role; it's spoken by that role's voice of the
    /// detected language (voices named "TTS-de 3 …", "TTS-en 3 …").
    PerUserLanguage,
    /// Every viewer keeps one voice from the whole pool.
    PerUser,
    /// The voice depends on the language only.
    PerLanguage,
    /// Always the same voice.
    Single,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceRules {
    pub mode: VoiceMode,
    /// Preset whose voices viewers get (mode Preset).
    pub preset: String,
    /// How many of the preset's voices are handed out (its first N; 0 = all).
    pub preset_voices: u32,
    /// Voices whose name starts like this form a language pool ({lang} = code).
    pub prefix: String,
    /// Languages without an own pool use this pool.
    pub fallback_language: String,
    /// Never assign these voices (name or id).
    pub exclude: Vec<String>,
    /// Fallback voice (name or id); None = VCP's default agent voice.
    pub default_voice: Option<String>,
    /// language code -> voice (mode PerLanguage)
    pub per_language: BTreeMap<String, String>,
    /// login -> voice (name or id), always wins
    pub users: BTreeMap<String, String>,
    /// Speak with emotions picked from emotes, emojis and punctuation.
    pub emotions: bool,
    /// At most this strong: "mild", "normal" or "strong".
    pub emotion_strength: String,
    /// Play voice effects (radio, robot, …) from the voice definitions and
    /// the viewer list.
    pub effects: bool,
    /// When the voice host can't speak, the system's own voice reads.
    pub os_fallback: bool,
}

impl Default for VoiceRules {
    fn default() -> Self {
        Self {
            mode: VoiceMode::Preset,
            preset: "cinematic".into(),
            preset_voices: 30,
            prefix: "TTS-{lang} ".into(),
            fallback_language: "en".into(),
            exclude: Vec::new(),
            default_voice: None,
            per_language: BTreeMap::new(),
            users: BTreeMap::new(),
            emotions: true,
            emotion_strength: "normal".into(),
            effects: true,
            os_fallback: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Channel logins (lower case, without #).
    pub channels: Vec<String>,
    pub reading: Reading,
    pub names: Names,
    pub language: LanguageRules,
    pub emotes: Emotes,
    pub emojis: Emojis,
    pub text: TextRules,
    pub mod_commands: ModCommands,
    pub voices: VoiceRules,
    pub translate: TranslateRules,
    /// Event announcements (subs, gifts, raids, cheers).
    pub events: Announcements,
    /// Viewers muted with !ttsmute or in the app.
    pub muted: Vec<String>,
    /// Where the voices play.
    pub output: OutputRules,
}

/// Where a profile's voices play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    /// The audio device chosen in the app's settings.
    #[default]
    Device,
    /// An OBS browser source that the app serves on this PC.
    Obs,
    /// An OBS browser source served by the voice host PC (TTV-TTS Proxy),
    /// which then reads this profile's chat itself.
    HostObs,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputRules {
    pub mode: OutputMode,
    /// The secret part of the OBS link.
    pub token: String,
    /// The OBS link also works from the internet (else only in this network).
    pub online: bool,
}

/// Event announcements, read by an announcer voice (off by default – chat
/// bots often announce already).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Announcements {
    pub enabled: bool,
    pub subs: bool,
    pub gifts: bool,
    pub raids: bool,
    pub bits: bool,
    /// Needs the Twitch login (off by default: follow bots).
    pub follows: bool,
    /// Channel-point rewards without text (needs the Twitch login).
    pub redemptions: bool,
    /// Cheers with fewer bits aren't announced.
    pub min_bits: u32,
    /// Raids with fewer viewers aren't announced.
    pub min_raid: u32,
    /// VCP voice (name or id) of the announcer; "" = the profile's fallback voice.
    pub voice: String,
    /// Own sentences per event kind (sub, resub, subgift, submysterygift,
    /// raid, bits, follow, redeem) with {user}, {months}, {recipient}, {count},
    /// {viewers}, {bits}, {reward}.
    pub texts: BTreeMap<String, String>,
}

impl Default for Announcements {
    fn default() -> Self {
        Self { enabled: false, subs: true, gifts: true, raids: true, bits: true, follows: false, redemptions: true, min_bits: 100, min_raid: 1, voice: String::new(), texts: BTreeMap::new() }
    }
}

/// "Translate for me": chat in languages the streamer doesn't speak.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TranslateRules {
    pub enabled: bool,
    /// Into this language ("" = the profile's default language).
    pub into: String,
    /// Languages the streamer understands: never translated.
    pub mine: Vec<String>,
    /// "translation" (read only it), "both" (the original, then the
    /// translation) or "show" (read the original, show the translation).
    pub read: String,
    /// "mymemory", "deepl" or "vcp".
    pub provider: String,
}

impl Default for TranslateRules {
    fn default() -> Self {
        Self { enabled: false, into: String::new(), mine: Vec::new(), read: "translation".into(), provider: "mymemory".into() }
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "My channel".into(),
            channels: Vec::new(),
            reading: Reading::default(),
            names: Names::default(),
            language: LanguageRules::default(),
            emotes: Emotes::default(),
            emojis: Emojis::default(),
            text: TextRules::default(),
            mod_commands: ModCommands::default(),
            voices: VoiceRules::default(),
            translate: TranslateRules::default(),
            events: Announcements::default(),
            muted: Vec::new(),
            output: OutputRules::default(),
        }
    }
}

impl Profile {
    pub fn normalize(&mut self) {
        if self.id.is_empty() {
            self.id = crate::random_key()[..10].to_ascii_lowercase();
        }
        let mut chans: Vec<String> = self
            .channels
            .iter()
            .map(|c| c.trim().trim_start_matches('#').trim_start_matches("https://www.twitch.tv/").trim_start_matches("twitch.tv/").to_ascii_lowercase())
            .filter(|c| !c.is_empty() && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'))
            .collect();
        chans.dedup();
        self.channels = chans;
        self.reading.max_chars = self.reading.max_chars.clamp(20, 2000);
        self.reading.max_queue = self.reading.max_queue.clamp(1, 200);
        self.reading.max_seconds = self.reading.max_seconds.clamp(5, 300);
        self.muted.iter_mut().for_each(|m| *m = m.to_ascii_lowercase());
        if self.output.token.len() < 12 {
            self.output.token = crate::obs::new_token();
        }
    }
}
