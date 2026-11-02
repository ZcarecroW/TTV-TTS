//! EventSub over WebSocket for the logged-in streamer's own channel: follows
//! and channel-point rewards without text (rewards with text arrive in chat).
//! The events become chat notices ("follow", "redeem") and take the same way
//! as subs and raids (see `announce`).

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::Message;

use super::account::{Tokens, Urls};
use super::irc::{ChatEvent, ChatMessage, Kind};

/// Follow bots add hundreds of follows at once: one announcement per window.
const FOLLOW_WINDOW: Duration = Duration::from_secs(10);

pub struct Config {
    pub http: reqwest::Client,
    pub urls: Urls,
    pub client_id: String,
    /// The streamer (broadcaster and their own moderator).
    pub user_id: String,
    pub tokens: Arc<dyn Tokens>,
}

/// What the settings show about the connection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    pub connected: bool,
    pub subscribed: Vec<String>,
    pub error: Option<String>,
}

pub type OnLive = Arc<dyn Fn(Live) + Send + Sync>;

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

enum End {
    Shutdown,
    Reconnect(String),
    Failed(String),
}

/// Runs until `shutdown` turns true; reconnects with backoff.
pub async fn run(cfg: Config, tx: mpsc::Sender<ChatEvent>, mut shutdown: watch::Receiver<bool>, on_live: OnLive) {
    let mut backoff = 1u64;
    let mut url = cfg.urls.eventsub.clone();
    // after Twitch's "reconnect" the subscriptions carry over
    let mut subscribe = true;
    let mut subscribed = Vec::new();
    loop {
        if *shutdown.borrow() {
            return;
        }
        let end = match tokio_tungstenite::connect_async(url.as_str()).await {
            Ok((ws, _)) => session(&cfg, ws, &tx, &mut shutdown, subscribe, &mut subscribed, &on_live).await,
            Err(e) => End::Failed(format!("EventSub connection failed: {e}")),
        };
        match end {
            End::Shutdown => return,
            End::Reconnect(u) => {
                url = u;
                subscribe = false;
                backoff = 1;
                continue;
            }
            End::Failed(reason) => {
                log::info!("EventSub: {reason}");
                on_live(Live { connected: false, subscribed: Vec::new(), error: Some(reason) });
                url = cfg.urls.eventsub.clone();
                subscribe = true;
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(backoff)) => {}
            _ = shutdown.changed() => return,
        }
        backoff = (backoff * 2).min(60);
    }
}

async fn session(cfg: &Config, mut ws: Ws, tx: &mpsc::Sender<ChatEvent>, shutdown: &mut watch::Receiver<bool>, subscribe: bool, subscribed: &mut Vec<String>, on_live: &OnLive) -> End {
    let mut keepalive = Duration::from_secs(30);
    let mut deadline = Instant::now() + Duration::from_secs(15);
    let mut welcomed = false;
    let mut last_follow: Option<Instant> = None;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        let msg = tokio::select! {
            _ = shutdown.changed() => {
                let _ = ws.close(None).await;
                return End::Shutdown;
            }
            m = timeout(wait, ws.next()) => m,
        };
        let msg = match msg {
            Err(_) => return End::Failed("no keepalive from Twitch".into()),
            Ok(None) => return End::Failed("connection closed".into()),
            Ok(Some(Err(e))) => return End::Failed(e.to_string()),
            Ok(Some(Ok(m))) => m,
        };
        deadline = Instant::now() + keepalive + Duration::from_secs(10);
        let text = match msg {
            Message::Text(t) => t,
            Message::Ping(p) => {
                let _ = ws.send(Message::Pong(p)).await;
                continue;
            }
            Message::Close(f) => return End::Failed(format!("Twitch closed the connection {}", f.map(|f| format!("{} {}", f.code, f.reason)).unwrap_or_default())),
            _ => continue,
        };
        let Ok(frame) = serde_json::from_str::<Value>(&text) else { continue };
        let meta = &frame["metadata"];
        match meta["message_type"].as_str().unwrap_or_default() {
            "session_welcome" => {
                let s = &frame["payload"]["session"];
                if let Some(k) = s["keepalive_timeout_seconds"].as_u64() {
                    keepalive = Duration::from_secs(k.max(10));
                    deadline = Instant::now() + keepalive + Duration::from_secs(10);
                }
                if !welcomed {
                    welcomed = true;
                    let mut error = None;
                    if subscribe {
                        let (ok, err) = subscribe_all(cfg, s["id"].as_str().unwrap_or_default()).await;
                        *subscribed = ok;
                        error = err;
                    }
                    if subscribed.is_empty() {
                        return End::Failed(error.unwrap_or_else(|| "no events available".into()));
                    }
                    on_live(Live { connected: true, subscribed: subscribed.clone(), error });
                }
            }
            "session_reconnect" => {
                if let Some(u) = frame["payload"]["session"]["reconnect_url"].as_str() {
                    return End::Reconnect(u.to_string());
                }
            }
            "revocation" => {
                let kind = frame["payload"]["subscription"]["type"].as_str().unwrap_or_default();
                let status = frame["payload"]["subscription"]["status"].as_str().unwrap_or_default();
                log::warn!("EventSub: {kind} ended ({status})");
                subscribed.retain(|k| k != kind);
                on_live(Live { connected: !subscribed.is_empty(), subscribed: subscribed.clone(), error: Some(format!("{kind}: {status}")) });
            }
            "notification" => {
                let kind = meta["subscription_type"].as_str().unwrap_or_default();
                let Some(m) = parse_event(kind, &frame["payload"]["event"]) else { continue };
                if m.notice_type.as_deref() == Some("follow") {
                    if last_follow.is_some_and(|t| t.elapsed() < FOLLOW_WINDOW) {
                        log::debug!("EventSub: follow of {} not announced (many follows at once)", m.login);
                        continue;
                    }
                    last_follow = Some(Instant::now());
                }
                if tx.send(ChatEvent::Message(Box::new(m))).await.is_err() {
                    return End::Shutdown;
                }
            }
            _ => {}
        }
    }
}

/// Subscribes to what the account may see: (kinds subscribed, why some failed).
async fn subscribe_all(cfg: &Config, session_id: &str) -> (Vec<String>, Option<String>) {
    let b = cfg.user_id.as_str();
    let subs = [
        ("channel.follow", "2", json!({ "broadcaster_user_id": b, "moderator_user_id": b })),
        ("channel.channel_points_custom_reward_redemption.add", "1", json!({ "broadcaster_user_id": b })),
    ];
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for (kind, version, condition) in subs {
        let body = json!({ "type": kind, "version": version, "condition": condition, "transport": { "method": "websocket", "session_id": session_id } });
        match create(cfg, &body).await {
            Ok(()) => ok.push(kind.to_string()),
            Err(e) => {
                log::info!("EventSub: {kind} unavailable: {e}");
                // channel points exist for Affiliates and Partners only
                errors.push(format!("{kind}: {e}"));
            }
        }
    }
    (ok, (!errors.is_empty()).then(|| errors.join("; ")))
}

async fn create(cfg: &Config, body: &Value) -> Result<(), String> {
    let mut token = cfg.tokens.token().await?;
    for attempt in 0..2 {
        let resp = cfg
            .http
            .post(format!("{}/eventsub/subscriptions", cfg.urls.helix))
            .header("Client-Id", &cfg.client_id)
            .bearer_auth(&token)
            .json(body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        if resp.status().is_success() {
            return Ok(());
        }
        if status == 401 && attempt == 0 {
            token = cfg.tokens.refreshed(token).await?;
            continue;
        }
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["message"].as_str().map(str::to_string)).unwrap_or(text);
        return Err(format!("{status} {msg}"));
    }
    Err("unauthorized".into())
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// A follow or a reward without text as a chat notice; None for anything
/// else (a reward with text is read from chat).
pub fn parse_event(kind: &str, e: &Value) -> Option<ChatMessage> {
    let mut params = std::collections::BTreeMap::new();
    let (notice, id) = match kind {
        "channel.follow" => ("follow", format!("follow-{}-{}", s(&e["user_id"]), s(&e["followed_at"]))),
        "channel.channel_points_custom_reward_redemption.add" => {
            if !s(&e["user_input"]).trim().is_empty() {
                return None;
            }
            params.insert("reward".to_string(), s(&e["reward"]["title"]));
            params.insert("reward-id".to_string(), s(&e["reward"]["id"]));
            params.insert("cost".to_string(), e["reward"]["cost"].as_u64().unwrap_or(0).to_string());
            ("redeem", format!("redeem-{}", s(&e["id"])))
        }
        _ => return None,
    };
    let login = s(&e["user_login"]);
    Some(ChatMessage {
        channel: s(&e["broadcaster_user_login"]).to_lowercase(),
        room_id: s(&e["broadcaster_user_id"]),
        id,
        display_name: Some(s(&e["user_name"])).filter(|n| !n.is_empty()).unwrap_or_else(|| login.clone()),
        login,
        user_id: s(&e["user_id"]),
        text: String::new(),
        action: false,
        roles: Default::default(),
        emotes: String::new(),
        bits: 0,
        reward_id: None,
        first_message: false,
        kind: Kind::Notice,
        notice_type: Some(notice.into()),
        system_msg: None,
        color: None,
        notice_params: params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::twitch::account::BoxFut;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestTokens(AtomicUsize);
    impl Tokens for TestTokens {
        fn token(&self) -> BoxFut<'_, Result<String, String>> {
            Box::pin(async { Ok("stale".to_string()) })
        }
        fn refreshed(&self, stale: String) -> BoxFut<'_, Result<String, String>> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Box::pin(async move {
                assert_eq!(stale, "stale");
                Ok("fresh".to_string())
            })
        }
    }

    fn follow(user: &str) -> Value {
        json!({ "metadata": { "message_type": "notification", "subscription_type": "channel.follow" }, "payload": { "event": {
            "user_id": format!("id-{user}"), "user_login": user, "user_name": user.to_uppercase(),
            "broadcaster_user_id": "71498656", "broadcaster_user_login": "zcarecrow", "broadcaster_user_name": "ZcarecroW",
            "followed_at": "2026-11-02T01:02:03Z" } } })
    }

    fn redeem(user: &str, input: &str) -> Value {
        json!({ "metadata": { "message_type": "notification", "subscription_type": "channel.channel_points_custom_reward_redemption.add" }, "payload": { "event": {
            "id": format!("r-{user}"), "user_id": "9", "user_login": user, "user_name": user, "user_input": input, "status": "unfulfilled",
            "broadcaster_user_id": "71498656", "broadcaster_user_login": "zcarecrow", "broadcaster_user_name": "ZcarecroW",
            "reward": { "id": "rw1", "title": "Hydrate!", "cost": 300, "prompt": "" }, "redeemed_at": "2026-11-02T01:02:03Z" } } })
    }

    fn welcome(id: &str) -> Value {
        json!({ "metadata": { "message_type": "session_welcome" }, "payload": { "session": { "id": id, "keepalive_timeout_seconds": 10 } } })
    }

    /// A fake Twitch: helix that refuses the stale token, two EventSub
    /// servers (the second one is where Twitch sends us to reconnect).
    #[tokio::test]
    async fn subscribes_announces_and_follows_a_reconnect() {
        use axum::{Router, extract::State, http::HeaderMap, routing::post};
        type Subs = Arc<parking_lot::Mutex<Vec<(String, String)>>>;
        let subs: Subs = Default::default();
        let app = Router::new()
            .route(
                "/helix/eventsub/subscriptions",
                post(|State(subs): State<Subs>, h: HeaderMap, body: String| async move {
                    let auth = h.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
                    if auth != "Bearer fresh" {
                        return (axum::http::StatusCode::UNAUTHORIZED, r#"{"message":"Invalid OAuth token"}"#.to_string());
                    }
                    let v: Value = serde_json::from_str(&body).unwrap();
                    assert_eq!(h.get("client-id").unwrap(), "test-client");
                    subs.lock().push((s(&v["type"]), s(&v["transport"]["session_id"])));
                    (axum::http::StatusCode::ACCEPTED, "{}".to_string())
                }),
            )
            .with_state(subs.clone());
        let helix = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let helix_addr = helix.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(helix, app).await.unwrap() });

        let ws1 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws2 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws2_url = format!("ws://{}", ws2.local_addr().unwrap());
        let ws1_url = format!("ws://{}", ws1.local_addr().unwrap());
        let subs1 = subs.clone();
        tokio::spawn(async move {
            let (tcp, _) = ws1.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let send = |v: Value| Message::Text(v.to_string().into());
            ws.send(send(welcome("s1"))).await.unwrap();
            for _ in 0..50 {
                if subs1.lock().len() == 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            ws.send(send(follow("alice"))).await.unwrap();
            ws.send(send(follow("bot1"))).await.unwrap(); // within the window: dropped
            ws.send(send(redeem("bob", ""))).await.unwrap();
            ws.send(send(redeem("carol", "with text"))).await.unwrap(); // read from chat instead
            ws.send(send(json!({ "metadata": { "message_type": "session_reconnect" }, "payload": { "session": { "reconnect_url": ws2_url } } }))).await.unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
        });
        tokio::spawn(async move {
            let (tcp, _) = ws2.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let send = |v: Value| Message::Text(v.to_string().into());
            ws.send(send(welcome("s2"))).await.unwrap();
            ws.send(send(follow("dave"))).await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
        });

        let tokens = Arc::new(TestTokens(AtomicUsize::new(0)));
        let cfg = Config {
            http: reqwest::Client::new(),
            urls: Urls { id: String::new(), helix: format!("http://{helix_addr}/helix"), eventsub: ws1_url },
            client_id: "test-client".into(),
            user_id: "71498656".into(),
            tokens: tokens.clone(),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let (stop_tx, stop_rx) = watch::channel(false);
        let lives: Arc<parking_lot::Mutex<Vec<Live>>> = Default::default();
        let l2 = lives.clone();
        let task = tokio::spawn(run(cfg, tx, stop_rx, Arc::new(move |l| l2.lock().push(l))));

        let mut got = Vec::new();
        while got.len() < 3 {
            let ev = timeout(Duration::from_secs(5), rx.recv()).await.expect("events").unwrap();
            if let ChatEvent::Message(m) = ev {
                got.push((m.notice_type.clone().unwrap(), m.display_name.clone(), m.channel.clone(), m.notice_params.get("reward").cloned()));
            }
        }
        assert_eq!(
            got,
            vec![
                ("follow".into(), "ALICE".into(), "zcarecrow".into(), None),
                ("redeem".into(), "bob".into(), "zcarecrow".into(), Some("Hydrate!".into())),
                ("follow".into(), "DAVE".into(), "zcarecrow".into(), None),
            ]
        );
        // both subscriptions on the first session only, after one refresh per subscription at most
        let s = subs.lock().clone();
        assert_eq!(s.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["channel.follow", "channel.channel_points_custom_reward_redemption.add"]);
        assert!(s.iter().all(|x| x.1 == "s1"));
        assert!(tokens.0.load(Ordering::Relaxed) >= 1);
        assert!(lives.lock().first().is_some_and(|l| l.connected && l.subscribed.len() == 2 && l.error.is_none()), "{:?}", lives.lock());
        stop_tx.send(true).unwrap();
        timeout(Duration::from_secs(3), task).await.expect("stops").unwrap();
    }

    #[test]
    fn other_events_are_ignored() {
        assert!(parse_event("stream.online", &json!({})).is_none());
        assert!(parse_event("channel.channel_points_custom_reward_redemption.add", &redeem("x", "  hi ")["payload"]["event"]).is_none());
    }
}
