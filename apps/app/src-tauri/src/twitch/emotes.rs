//! Third-party emote names (BTTV, FFZ, 7TV) per channel, so "KEKW" is read
//! as an emote only where it is one. Twitch's own emotes come with every
//! message. Global sets are shared; lists are refreshed every 10 minutes,
//! and a channel's 7TV set follows 7TV's EventAPI live (emotes added,
//! removed or renamed during the stream count at once). Only the names are
//! kept (a big channel's 7TV answer is ~2.5 MB).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
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
#[derive(Deserialize, Default)]
#[serde(default)]
struct SevenEmote {
    name: String,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct SevenSet {
    id: String,
    emotes: Vec<SevenEmote>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct SevenAccount {
    id: String,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct SevenUser {
    emote_set: Option<SevenSet>,
    user: Option<SevenAccount>,
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

/// A channel's 7TV set, kept current by the EventAPI.
#[derive(Default)]
struct Live {
    set_id: String,
    user_id: String,
    names: HashSet<String>,
    /// The EventAPI connection for this set runs.
    watching: bool,
}

type LiveMap = Arc<RwLock<HashMap<String, Live>>>;

pub struct Emotes {
    http: reqwest::Client,
    global: RwLock<Entry>,
    /// BTTV + FFZ per room.
    rooms: RwLock<HashMap<String, Entry>>,
    /// 7TV per room.
    seven: LiveMap,
}

impl Emotes {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .user_agent(concat!("TTV-TTS/", env!("CARGO_PKG_VERSION"), " (+https://github.com/ZcarecroW/TTV-TTS)"))
            .build()
            .expect("emote client");
        Self { http, global: RwLock::new(Entry::default()), rooms: RwLock::new(HashMap::new()), seven: Arc::default() }
    }

    pub fn is_emote(&self, room_id: &str, word: &str) -> bool {
        self.global.read().names.contains(word)
            || self.rooms.read().get(room_id).is_some_and(|e| e.names.contains(word))
            || self.seven.read().get(room_id).is_some_and(|l| l.names.contains(word))
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
            let seven = self.seven.read().get(room_id).map_or(0, |l| l.names.len());
            log::info!("emotes for room {room_id}: {} channel + {seven} 7TV + {} global", names.len(), self.global.read().names.len());
            self.rooms.write().insert(room_id.to_string(), Entry { names: Arc::new(names), loaded: Some(Instant::now()) });
        }
    }

    async fn get<T: for<'de> Deserialize<'de> + Default>(&self, url: &str) -> T {
        get(&self.http, url).await
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
        let (bttv, ffz) = tokio::join!(
            async { if p.bttv { self.get::<BttvUser>(&format!("https://api.betterttv.net/3/cached/users/twitch/{id}")).await } else { BttvUser::default() } },
            async { if p.ffz { self.get::<Vec<BttvEmote>>(&format!("https://api.betterttv.net/3/cached/frankerfacez/users/twitch/{id}")).await } else { Vec::new() } },
        );
        set.extend(bttv.channel.into_iter().chain(bttv.shared).chain(ffz).filter(|e| !e.modifier).map(|e| e.code));
        if p.seventv {
            load_seven(&self.http, &self.seven, id).await;
            let watch = self.seven.write().get_mut(id).is_some_and(|l| !l.set_id.is_empty() && !std::mem::replace(&mut l.watching, true));
            if watch {
                tokio::spawn(watch_seven(self.http.clone(), self.seven.clone(), id.to_string()));
            }
        }
        set
    }
}

async fn get<T: for<'de> Deserialize<'de> + Default>(http: &reqwest::Client, url: &str) -> T {
    match http.get(url).timeout(Duration::from_secs(15)).send().await {
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

/// (Re)loads a room's 7TV set; keeps the watching flag.
async fn load_seven(http: &reqwest::Client, map: &LiveMap, room: &str) {
    let u: SevenUser = get(http, &format!("https://7tv.io/v3/users/twitch/{room}")).await;
    let set = u.emote_set.unwrap_or_default();
    let mut m = map.write();
    let l = m.entry(room.to_string()).or_default();
    l.set_id = set.id;
    l.user_id = u.user.map(|a| a.id).unwrap_or_default();
    l.names = set.emotes.into_iter().map(|e| e.name).collect();
}

/// Follows a room's 7TV set over the EventAPI (Server-Sent Events) for as
/// long as the app runs; reconnects with backoff.
async fn watch_seven(http: reqwest::Client, map: LiveMap, room: String) {
    let mut wait = Duration::from_secs(2);
    loop {
        let (set, user) = {
            let m = map.read();
            let Some(l) = m.get(&room) else { return };
            (l.set_id.clone(), l.user_id.clone())
        };
        if set.is_empty() {
            if let Some(l) = map.write().get_mut(&room) {
                l.watching = false;
            }
            return;
        }
        let mut subs = format!("emote_set.update<object_id={set}>");
        if !user.is_empty() {
            subs.push_str(&format!(",user.update<object_id={user}>"));
        }
        let url = format!("https://events.7tv.io/v3@{}", url_encode(&subs));
        match follow(&http, &url, &map, &room).await {
            Ok(Follow::SetChanged) => {
                log::info!("7TV: room {room} switched its emote set");
                load_seven(&http, &map, &room).await;
                wait = Duration::from_secs(2);
                continue;
            }
            Ok(Follow::Ended) => wait = Duration::from_secs(2),
            Err(e) => {
                log::debug!("7TV events for {room}: {e}");
                wait = (wait * 2).min(Duration::from_secs(300));
            }
        }
        tokio::time::sleep(wait).await;
    }
}

enum Follow {
    /// The connection ended normally (reconnect, end of stream).
    Ended,
    /// The channel uses another emote set now.
    SetChanged,
}

async fn follow(http: &reqwest::Client, url: &str, map: &LiveMap, room: &str) -> Result<Follow, String> {
    let r = http.get(url).header("Accept", "text/event-stream").send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    let mut stream = r.bytes_stream();
    let mut buf = String::new();
    // no data for three heartbeats (~47 s each) = a dead connection
    let mut heartbeat = Duration::from_secs(50);
    loop {
        let chunk = match tokio::time::timeout(heartbeat * 3, stream.next()).await {
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(e))) => return Err(e.to_string()),
            Ok(None) => return Ok(Follow::Ended),
            Err(_) => return Err("no heartbeat".into()),
        };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buf.find("\n\n") {
            let event: String = buf.drain(..end + 2).collect();
            let (mut kind, mut data) = ("", String::new());
            for line in event.lines() {
                if let Some(v) = line.strip_prefix("event:") {
                    kind = v.trim();
                } else if let Some(v) = line.strip_prefix("data:") {
                    data.push_str(v.trim());
                }
            }
            match kind {
                "hello" => {
                    if let Some(ms) = serde_json::from_str::<serde_json::Value>(&data).ok().and_then(|v| v["heartbeat_interval"].as_u64()) {
                        heartbeat = Duration::from_millis(ms.max(5_000));
                    }
                }
                "dispatch" => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                        match apply(&v, map, room) {
                            Applied::SetChanged => return Ok(Follow::SetChanged),
                            Applied::Changed(n) => log::info!("7TV: room {room} emotes changed ({n})"),
                            Applied::Nothing => {}
                        }
                    }
                }
                "reconnect" | "end_of_stream" => return Ok(Follow::Ended),
                _ => {}
            }
        }
    }
}

enum Applied {
    Nothing,
    Changed(usize),
    SetChanged,
}

/// Applies one EventAPI dispatch (`{type, body}`) to a room's 7TV names.
fn apply(v: &serde_json::Value, map: &LiveMap, room: &str) -> Applied {
    let body = &v["body"];
    match v["type"].as_str() {
        Some("emote_set.update") => {
            let mut m = map.write();
            let Some(l) = m.get_mut(room) else { return Applied::Nothing };
            let mut n = 0;
            let list = |k: &str| body[k].as_array().cloned().unwrap_or_default();
            for p in list("pulled").iter().chain(list("updated").iter()) {
                if p["key"] == "emotes"
                    && let Some(name) = p["old_value"]["name"].as_str()
                {
                    n += usize::from(l.names.remove(name));
                }
            }
            for p in list("pushed").iter().chain(list("updated").iter()) {
                if p["key"] == "emotes"
                    && let Some(name) = p["value"]["name"].as_str()
                {
                    n += usize::from(l.names.insert(name.to_string()));
                }
            }
            if n > 0 { Applied::Changed(n) } else { Applied::Nothing }
        }
        Some("user.update") => {
            // the active emote set was switched: nested under "connections"
            let switched = body["updated"].as_array().is_some_and(|a| {
                a.iter().any(|u| {
                    u["key"] == "connections"
                        && u["value"].as_array().is_some_and(|vals| vals.iter().any(|x| matches!(x["key"].as_str(), Some("emote_set" | "emote_set_id"))))
                })
            });
            if switched { Applied::SetChanged } else { Applied::Nothing }
        }
        _ => Applied::Nothing,
    }
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'@' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_with(names: &[&str]) -> LiveMap {
        let map: LiveMap = Arc::default();
        map.write().insert("123".into(), Live { set_id: "set".into(), names: names.iter().map(|s| s.to_string()).collect(), ..Default::default() });
        map
    }

    #[test]
    fn applies_added_removed_and_renamed_emotes() {
        let map = map_with(&["KEKW", "OldOne", "Renamed"]);
        let v = serde_json::json!({ "type": "emote_set.update", "body": {
            "id": "set",
            "pushed": [{ "key": "emotes", "value": { "id": "1", "name": "NewEmote" } }],
            "pulled": [{ "key": "emotes", "old_value": { "id": "2", "name": "OldOne" } }],
            "updated": [{ "key": "emotes", "old_value": { "id": "3", "name": "Renamed" }, "value": { "id": "3", "name": "RenamedNow" } }],
        }});
        assert!(matches!(apply(&v, &map, "123"), Applied::Changed(4)));
        let names = &map.read()["123"].names;
        assert!(names.contains("NewEmote") && names.contains("RenamedNow") && names.contains("KEKW"));
        assert!(!names.contains("OldOne") && !names.contains("Renamed"));
    }

    #[test]
    fn notices_a_switched_emote_set() {
        let map = map_with(&[]);
        let v = serde_json::json!({ "type": "user.update", "body": { "updated": [{ "key": "connections", "value": [{ "key": "emote_set", "value": { "id": "new" } }] }] } });
        assert!(matches!(apply(&v, &map, "123"), Applied::SetChanged));
        let other = serde_json::json!({ "type": "user.update", "body": { "updated": [{ "key": "username" }] } });
        assert!(matches!(apply(&other, &map, "123"), Applied::Nothing));
    }

    #[test]
    fn encodes_the_subscription() {
        assert_eq!(url_encode("emote_set.update<object_id=ab>,user.update<object_id=cd>"), "emote_set.update%3Cobject_id%3Dab%3E%2Cuser.update%3Cobject_id%3Dcd%3E");
    }

    /// Live: a big channel's 7TV set and its EventAPI hello.
    #[tokio::test]
    #[ignore]
    async fn seven_live() {
        let http = reqwest::Client::builder().user_agent("TTV-TTS test").build().unwrap();
        let map: LiveMap = Arc::default();
        // xQc (Twitch id 71092938) uses 7TV
        load_seven(&http, &map, "71092938").await;
        let (set, user, n) = { let m = map.read(); let l = &m["71092938"]; (l.set_id.clone(), l.user_id.clone(), l.names.len()) };
        println!("set {set} user {user}: {n} emotes");
        assert!(n > 50 && !set.is_empty());
        let url = format!("https://events.7tv.io/v3@{}", url_encode(&format!("emote_set.update<object_id={set}>")));
        let r = http.get(&url).header("Accept", "text/event-stream").send().await.unwrap();
        assert!(r.status().is_success());
        let mut s = r.bytes_stream();
        let first = tokio::time::timeout(Duration::from_secs(10), s.next()).await.unwrap().unwrap().unwrap();
        let text = String::from_utf8_lossy(&first);
        println!("{text}");
        assert!(text.contains("hello"));
    }
}
