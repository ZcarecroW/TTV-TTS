//! Third-party emote names (BTTV, FFZ, 7TV) per channel, so "KEKW" is read
//! as an emote only where it is one. Twitch's own emotes come with every
//! message. Global sets are shared; lists are refreshed every 10 minutes.
//! Only the names are kept (a big channel's 7TV answer is ~2.5 MB).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::Deserialize;

const REFRESH: Duration = Duration::from_secs(600);

#[derive(Deserialize)]
struct BttvEmote {
    code: String,
    #[serde(default)]
    modifier: bool,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct BttvUser {
    #[serde(rename = "channelEmotes")]
    channel: Vec<BttvEmote>,
    #[serde(rename = "sharedEmotes")]
    shared: Vec<BttvEmote>,
}
#[derive(Deserialize)]
struct SevenEmote {
    name: String,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct SevenSet {
    emotes: Vec<SevenEmote>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct SevenUser {
    emote_set: Option<SevenSet>,
}

#[derive(Clone, Copy)]
pub struct Providers {
    pub bttv: bool,
    pub ffz: bool,
    pub seventv: bool,
}

#[derive(Default)]
struct Entry {
    names: Arc<HashSet<String>>,
    loaded: Option<Instant>,
}

pub struct Emotes {
    http: reqwest::Client,
    global: RwLock<Entry>,
    rooms: RwLock<HashMap<String, Entry>>,
}

impl Emotes {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(concat!("TTV-TTS/", env!("CARGO_PKG_VERSION"), " (+https://github.com/ZcarecroW/TTV-TTS)"))
            .build()
            .expect("emote client");
        Self { http, global: RwLock::new(Entry::default()), rooms: RwLock::new(HashMap::new()) }
    }

    pub fn is_emote(&self, room_id: &str, word: &str) -> bool {
        self.global.read().names.contains(word) || self.rooms.read().get(room_id).is_some_and(|e| e.names.contains(word))
    }

    /// Loads (or refreshes, if older than 10 min) the lists for `room_id`.
    pub async fn ensure(&self, room_id: &str, p: Providers) {
        let global_old = self.global.read().loaded.is_none_or(|t| t.elapsed() > REFRESH);
        if global_old {
            let names = self.load_global(p).await;
            *self.global.write() = Entry { names: Arc::new(names), loaded: Some(Instant::now()) };
        }
        let room_old = self.rooms.read().get(room_id).is_none_or(|e| e.loaded.is_none_or(|t| t.elapsed() > REFRESH));
        if room_old && !room_id.is_empty() {
            let names = self.load_room(room_id, p).await;
            log::info!("emotes for room {room_id}: {} channel + {} global", names.len(), self.global.read().names.len());
            self.rooms.write().insert(room_id.to_string(), Entry { names: Arc::new(names), loaded: Some(Instant::now()) });
        }
    }

    async fn get<T: for<'de> Deserialize<'de> + Default>(&self, url: &str) -> T {
        match self.http.get(url).send().await {
            Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
            // 404 = the channel doesn't use that service.
            Ok(r) if r.status().as_u16() == 404 => T::default(),
            Ok(r) => {
                log::debug!("{url}: HTTP {}", r.status());
                T::default()
            }
            Err(e) => {
                log::debug!("{url}: {e}");
                T::default()
            }
        }
    }

    async fn load_global(&self, p: Providers) -> HashSet<String> {
        let mut set = HashSet::new();
        let (bttv, ffz, seven) = tokio::join!(
            async { if p.bttv { self.get::<Vec<BttvEmote>>("https://api.betterttv.net/3/cached/emotes/global").await } else { Vec::new() } },
            async { if p.ffz { self.get::<Vec<BttvEmote>>("https://api.betterttv.net/3/cached/frankerfacez/emotes/global").await } else { Vec::new() } },
            async { if p.seventv { self.get::<SevenSet>("https://7tv.io/v3/emote-sets/global").await } else { SevenSet::default() } },
        );
        set.extend(bttv.into_iter().chain(ffz).filter(|e| !e.modifier).map(|e| e.code));
        set.extend(seven.emotes.into_iter().map(|e| e.name));
        set
    }

    async fn load_room(&self, id: &str, p: Providers) -> HashSet<String> {
        let mut set = HashSet::new();
        let (bttv, ffz, seven) = tokio::join!(
            async { if p.bttv { self.get::<BttvUser>(&format!("https://api.betterttv.net/3/cached/users/twitch/{id}")).await } else { BttvUser::default() } },
            async { if p.ffz { self.get::<Vec<BttvEmote>>(&format!("https://api.betterttv.net/3/cached/frankerfacez/users/twitch/{id}")).await } else { Vec::new() } },
            async { if p.seventv { self.get::<SevenUser>(&format!("https://7tv.io/v3/users/twitch/{id}")).await } else { SevenUser::default() } },
        );
        set.extend(bttv.channel.into_iter().chain(bttv.shared).chain(ffz).filter(|e| !e.modifier).map(|e| e.code));
        set.extend(seven.emote_set.unwrap_or_default().emotes.into_iter().map(|e| e.name));
        set
    }
}
