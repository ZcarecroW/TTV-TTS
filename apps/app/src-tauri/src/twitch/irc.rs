//! Anonymous Twitch chat (IRC over WebSocket, `justinfan` login): reads any
//! channel without an account. One connection carries all channels; the set
//! of channels can change at any time (JOIN/PART), and the connection comes
//! back by itself after drops (backoff 1 s … 30 s).

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

const URL: &str = "wss://irc-ws.chat.twitch.tv:443";

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Roles {
    pub broadcaster: bool,
    pub moderator: bool,
    pub vip: bool,
    pub subscriber: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Chat,
    /// sub, resub, subgift, raid … (USERNOTICE)
    Notice,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub channel: String,
    pub room_id: String,
    pub id: String,
    pub login: String,
    pub display_name: String,
    pub user_id: String,
    pub text: String,
    /// "/me …"
    pub action: bool,
    pub roles: Roles,
    /// Twitch's `emotes` tag (code-point positions).
    pub emotes: String,
    pub bits: u32,
    pub reward_id: Option<String>,
    pub first_message: bool,
    pub kind: Kind,
    /// USERNOTICE type: sub, resub, subgift, submysterygift, raid, …
    pub notice_type: Option<String>,
    /// Twitch's own description of the event ("X subscribed for 12 months").
    pub system_msg: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ChatEvent {
    Message(Box<ChatMessage>),
    /// The channel's room id (needed for emote lists) – once per join.
    Joined { channel: String, room_id: String },
    /// A viewer was timed out or banned (None = whole chat cleared).
    ClearUser { channel: String, user_id: Option<String> },
    ClearMessage { channel: String, msg_id: String },
    /// Connection state changed.
    Connection { connected: bool, error: Option<String> },
}

/// Handle to the chat connection. Dropping it ends the connection.
pub struct Chat {
    channels: watch::Sender<BTreeSet<String>>,
}

impl Chat {
    pub fn start(events: mpsc::Sender<ChatEvent>) -> Self {
        let (tx, rx) = watch::channel(BTreeSet::new());
        tokio::spawn(run(rx, events));
        Self { channels: tx }
    }

    /// The channels to read (lower-case logins). Empty = disconnect.
    pub fn set_channels(&self, channels: BTreeSet<String>) {
        self.channels.send_if_modified(|c| {
            if *c == channels {
                false
            } else {
                *c = channels;
                true
            }
        });
    }
}

async fn run(mut want: watch::Receiver<BTreeSet<String>>, events: mpsc::Sender<ChatEvent>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if want.borrow().is_empty() {
            let _ = events.send(ChatEvent::Connection { connected: false, error: None }).await;
            if want.changed().await.is_err() {
                return;
            }
            continue;
        }
        match session(&mut want, &events).await {
            Ok(()) => {
                // Planned: channel list emptied or Twitch asked to reconnect.
                backoff = Duration::from_secs(1);
            }
            Err(e) => {
                log::warn!("Twitch chat: {e} - reconnecting in {} s", backoff.as_secs());
                let _ = events.send(ChatEvent::Connection { connected: false, error: Some(e) }).await;
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    r = want.changed() => if r.is_err() { return },
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
        if events.is_closed() {
            return;
        }
    }
}

async fn session(want: &mut watch::Receiver<BTreeSet<String>>, events: &mpsc::Sender<ChatEvent>) -> Result<(), String> {
    let (ws, _) = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(URL))
        .await
        .map_err(|_| "connecting timed out".to_string())?
        .map_err(|e| e.to_string())?;
    let (mut sink, mut stream) = ws.split();
    let nick = format!("justinfan{}", 10_000 + rand::random::<u32>() % 80_000);
    for line in ["CAP REQ :twitch.tv/tags twitch.tv/commands".to_string(), "PASS SCHMOOPIIE".into(), format!("NICK {nick}")] {
        sink.send(Message::Text(line.into())).await.map_err(|e| e.to_string())?;
    }
    let mut joined: BTreeSet<String> = BTreeSet::new();
    let mut rooms: HashMap<String, String> = HashMap::new();
    let mut announced = false;
    // Twitch drops idle connections after ~5 min without PING; we send one too.
    let mut ping = tokio::time::interval(Duration::from_secs(240));
    ping.tick().await;
    loop {
        // Join/leave to match the wanted set (also right after connecting).
        let wanted = want.borrow().clone();
        if wanted.is_empty() {
            return Ok(());
        }
        let add: Vec<&String> = wanted.difference(&joined).collect();
        let remove: Vec<&String> = joined.difference(&wanted).collect();
        if !add.is_empty() {
            let list = add.iter().map(|c| format!("#{c}")).collect::<Vec<_>>().join(",");
            sink.send(Message::Text(format!("JOIN {list}").into())).await.map_err(|e| e.to_string())?;
        }
        if !remove.is_empty() {
            let list = remove.iter().map(|c| format!("#{c}")).collect::<Vec<_>>().join(",");
            sink.send(Message::Text(format!("PART {list}").into())).await.map_err(|e| e.to_string())?;
            for c in &remove {
                rooms.remove(*c);
            }
        }
        joined = wanted;

        tokio::select! {
            msg = stream.next() => {
                let text = match msg {
                    Some(Ok(Message::Text(t))) => t,
                    Some(Ok(Message::Ping(p))) => { let _ = sink.send(Message::Pong(p)).await; continue }
                    Some(Ok(Message::Close(_))) | None => return Err("Twitch closed the connection".into()),
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return Err(e.to_string()),
                };
                for line in text.split("\r\n").filter(|l| !l.is_empty()) {
                    let m = parse(line);
                    match m.command.as_str() {
                        "PING" => {
                            let arg = m.params.first().cloned().unwrap_or_else(|| "tmi.twitch.tv".into());
                            sink.send(Message::Text(format!("PONG :{arg}").into())).await.map_err(|e| e.to_string())?;
                        }
                        "RECONNECT" => return Err("Twitch asked to reconnect".into()),
                        "001" => {
                            if !announced {
                                announced = true;
                                let _ = events.send(ChatEvent::Connection { connected: true, error: None }).await;
                            }
                        }
                        "ROOMSTATE" => {
                            let chan = m.channel();
                            if let Some(id) = m.tags.get("room-id")
                                && rooms.get(&chan) != Some(id)
                            {
                                rooms.insert(chan.clone(), id.clone());
                                let _ = events.send(ChatEvent::Joined { channel: chan, room_id: id.clone() }).await;
                            }
                        }
                        "PRIVMSG" | "USERNOTICE" => {
                            if let Some(msg) = to_message(&m, &rooms) {
                                let _ = events.send(ChatEvent::Message(Box::new(msg))).await;
                            }
                        }
                        "CLEARCHAT" => {
                            let _ = events.send(ChatEvent::ClearUser { channel: m.channel(), user_id: m.tags.get("target-user-id").cloned() }).await;
                        }
                        "CLEARMSG" => {
                            if let Some(id) = m.tags.get("target-msg-id") {
                                let _ = events.send(ChatEvent::ClearMessage { channel: m.channel(), msg_id: id.clone() }).await;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ = want.changed() => {}
            _ = ping.tick() => {
                sink.send(Message::Text("PING :ttv-tts".into())).await.map_err(|e| e.to_string())?;
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct IrcLine {
    pub tags: HashMap<String, String>,
    pub prefix: String,
    pub command: String,
    pub params: Vec<String>,
}

impl IrcLine {
    fn channel(&self) -> String {
        self.params.first().map(|p| p.trim_start_matches('#').to_string()).unwrap_or_default()
    }
}

fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('s') => out.push(' '),
                Some(':') => out.push(';'),
                Some('\\') => out.push('\\'),
                Some('r') => out.push('\r'),
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parses one IRC line with IRCv3 tags.
pub fn parse(line: &str) -> IrcLine {
    let mut m = IrcLine::default();
    let mut rest = line;
    if let Some(r) = rest.strip_prefix('@') {
        let (tags, after) = r.split_once(' ').unwrap_or((r, ""));
        for part in tags.split(';') {
            let (k, v) = part.split_once('=').unwrap_or((part, ""));
            m.tags.insert(k.to_string(), unescape(v));
        }
        rest = after;
    }
    if let Some(r) = rest.strip_prefix(':') {
        let (p, after) = r.split_once(' ').unwrap_or((r, ""));
        m.prefix = p.to_string();
        rest = after;
    }
    let (head, trailing) = match rest.find(" :") {
        Some(i) => (&rest[..i], Some(&rest[i + 2..])),
        None => (rest, None),
    };
    let mut parts = head.split(' ').filter(|p| !p.is_empty());
    m.command = parts.next().unwrap_or("").to_string();
    m.params = parts.map(str::to_string).collect();
    if let Some(t) = trailing {
        m.params.push(t.to_string());
    }
    m
}

fn to_message(m: &IrcLine, rooms: &HashMap<String, String>) -> Option<ChatMessage> {
    let channel = m.channel();
    let notice = m.command == "USERNOTICE";
    let raw = m.params.get(1).cloned().unwrap_or_default();
    // Notices without attached text are still useful (event announcements).
    if raw.is_empty() && !notice {
        return None;
    }
    let (text, action) = match raw.strip_prefix("\u{1}ACTION ").and_then(|t| t.strip_suffix('\u{1}')) {
        Some(t) => (t.to_string(), true),
        None => (raw, false),
    };
    let login = m.tags.get("login").cloned().unwrap_or_else(|| m.prefix.split('!').next().unwrap_or("").to_string());
    let badges: Vec<&str> = m.tags.get("badges").map(|b| b.split(',').map(|x| x.split('/').next().unwrap_or("")).collect()).unwrap_or_default();
    let roles = Roles {
        broadcaster: badges.contains(&"broadcaster") || login.eq_ignore_ascii_case(&channel),
        moderator: m.tags.get("mod").is_some_and(|v| v == "1") || badges.contains(&"moderator"),
        vip: m.tags.get("vip").is_some_and(|v| v == "1") || badges.contains(&"vip"),
        subscriber: m.tags.get("subscriber").is_some_and(|v| v == "1") || badges.contains(&"subscriber") || badges.contains(&"founder"),
    };
    let tag = |k: &str| m.tags.get(k).filter(|v| !v.is_empty()).cloned();
    Some(ChatMessage {
        room_id: tag("room-id").or_else(|| rooms.get(&channel).cloned()).unwrap_or_default(),
        id: tag("id").unwrap_or_default(),
        display_name: tag("display-name").unwrap_or_else(|| login.clone()),
        user_id: tag("user-id").unwrap_or_else(|| login.clone()),
        login: login.to_ascii_lowercase(),
        text,
        action,
        roles,
        emotes: tag("emotes").unwrap_or_default(),
        bits: tag("bits").and_then(|b| b.parse().ok()).unwrap_or(0),
        reward_id: tag("custom-reward-id"),
        first_message: m.tags.get("first-msg").is_some_and(|v| v == "1"),
        kind: if notice { Kind::Notice } else { Kind::Chat },
        notice_type: if notice { tag("msg-id") } else { None },
        system_msg: if notice { tag("system-msg") } else { None },
        color: tag("color"),
        channel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_privmsg_with_tags() {
        let line = r"@badge-info=;badges=moderator/1,subscriber/12;color=#FF0000;display-name=Max_M;emotes=25:0-4;first-msg=0;id=abc-123;mod=1;room-id=1234;subscriber=1;user-id=999 :max_m!max_m@max_m.tmi.twitch.tv PRIVMSG #zcarecrow :Kappa hello\sworld";
        let m = parse(line);
        assert_eq!(m.command, "PRIVMSG");
        assert_eq!(m.params, vec!["#zcarecrow", r"Kappa hello\sworld"]);
        let msg = to_message(&m, &HashMap::new()).unwrap();
        assert_eq!(msg.channel, "zcarecrow");
        assert_eq!(msg.login, "max_m");
        assert_eq!(msg.display_name, "Max_M");
        assert!(msg.roles.moderator && msg.roles.subscriber && !msg.roles.broadcaster);
        assert_eq!(msg.emotes, "25:0-4");
        assert_eq!(msg.room_id, "1234");
    }

    #[test]
    fn action_and_unescape() {
        let m = parse("@display-name=Bob;system-msg=Bob\\ssubscribed\\sfor\\s3\\smonths! :tmi.twitch.tv USERNOTICE #chan :\u{1}ACTION dances\u{1}");
        assert_eq!(m.tags["system-msg"], "Bob subscribed for 3 months!");
        let msg = to_message(&m, &HashMap::new()).unwrap();
        assert!(msg.action);
        assert_eq!(msg.text, "dances");
        assert_eq!(msg.kind, Kind::Notice);
    }

    #[test]
    fn notices_without_text_are_kept_chat_without_text_is_not() {
        let n = parse("@msg-id=raid;login=raider :tmi.twitch.tv USERNOTICE #chan");
        assert!(to_message(&n, &HashMap::new()).is_some());
        let c = parse(":a!a@a.tmi.twitch.tv PRIVMSG #chan :");
        assert!(to_message(&c, &HashMap::new()).is_none());
    }

    #[test]
    fn ping_and_numeric() {
        let p = parse("PING :tmi.twitch.tv");
        assert_eq!(p.command, "PING");
        assert_eq!(p.params, vec!["tmi.twitch.tv"]);
        assert_eq!(parse(":tmi.twitch.tv 001 justinfan123 :Welcome, GLHF!").command, "001");
    }
}

/// Live test (network): `cargo test -p ttv-tts live_twitch -- --ignored --nocapture`
/// TTV_LIVE_CHANNEL picks the channel (default: a busy one).
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_twitch_anonymous_read() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let channel = std::env::var("TTV_LIVE_CHANNEL").unwrap_or_else(|_| "summit1g".into());
        let (tx, mut rx) = mpsc::channel(1024);
        let chat = Chat::start(tx);
        chat.set_channels(channel.split(',').map(|c| c.trim().to_string()).collect());
        let t0 = std::time::Instant::now();
        let (mut msgs, mut joined, mut connected) = (0, false, false);
        let mut per: HashMap<String, u32> = HashMap::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        while let Ok(Some(ev)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            match ev {
                ChatEvent::Connection { connected: c, error } => {
                    connected = c;
                    println!("{:>5} ms connection: {c} {error:?}", t0.elapsed().as_millis());
                }
                ChatEvent::Joined { channel, room_id } => {
                    joined = true;
                    println!("{:>5} ms joined #{channel} room {room_id}", t0.elapsed().as_millis());
                }
                ChatEvent::Message(m) => {
                    msgs += 1;
                    *per.entry(m.channel.clone()).or_default() += 1;
                    if msgs <= 5 {
                        println!("{:>5} ms {}: {}", t0.elapsed().as_millis(), m.display_name, m.text.chars().take(60).collect::<String>());
                    }
                }
                _ => {}
            }
        }
        println!("{msgs} messages in 15 s: {per:?}");
        assert!(connected && joined, "connected and joined");
    }
}
