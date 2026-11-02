//! Optional Twitch login for what the anonymous chat connection can't see:
//! follows and channel-point rewards without text (EventSub, see
//! `eventsub`). Device code flow with a public client (no secret): the app
//! shows a code, the streamer confirms it on twitch.tv/activate. The tokens
//! live in the OS credential store.

use std::pin::Pin;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

/// Follows need a moderator scope (the streamer is their own moderator).
pub const SCOPES: &[&str] = &["moderator:read:followers", "channel:read:redemptions"];

// `SCRAMBLED`: the client ID of release builds, scrambled by build.rs.
include!(concat!(env!("OUT_DIR"), "/twitch_id.rs"));

/// The client ID built into this release (a streamer can also enter the one
/// of their own Twitch application). Unscrambled on first use only; never
/// logged or shown.
pub fn built_in_client_id() -> Option<String> {
    static ID: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        // black_box: the compiler must not work the ID out at build time
        let (blob, seed) = std::hint::black_box(SCRAMBLED)?;
        super::scramble::unscramble(std::hint::black_box(blob), std::hint::black_box(seed)).filter(|s| !s.trim().is_empty())
    })
    .clone()
}

pub const TOKEN_KEY: &str = "twitch-token";

#[derive(Debug, Clone)]
pub struct Urls {
    pub id: String,
    pub helix: String,
    pub eventsub: String,
}

impl Default for Urls {
    fn default() -> Self {
        Self { id: "https://id.twitch.tv".into(), helix: "https://api.twitch.tv/helix".into(), eventsub: "wss://eventsub.wss.twitch.tv/ws".into() }
    }
}

/// Twitch's answer to `POST /oauth2/device`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: i64,
    pub scopes: Vec<String>,
    pub user_id: String,
    pub login: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateInfo {
    pub login: String,
    pub user_id: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

pub enum Poll {
    Pending,
    SlowDown,
    Done { access_token: String, refresh_token: String, expires_at: i64, scopes: Vec<String> },
}

#[derive(Debug)]
pub enum AuthError {
    /// Twitch refused (bad client id, refresh token no longer valid, …).
    Twitch(u16, String),
    /// Twitch couldn't be reached.
    Network(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Twitch(s, m) => write!(f, "Twitch said {s}: {m}"),
            Self::Network(e) => write!(f, "Twitch isn't reachable ({e})"),
        }
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

async fn twitch_error(resp: reqwest::Response) -> AuthError {
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let msg = serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| v["message"].as_str().map(str::to_string)).unwrap_or(text);
    AuthError::Twitch(status, msg)
}

fn net(e: reqwest::Error) -> AuthError {
    AuthError::Network(e.to_string())
}

pub async fn start_device_flow(http: &reqwest::Client, urls: &Urls, client_id: &str) -> Result<DeviceCode, AuthError> {
    let resp = http.post(format!("{}/oauth2/device", urls.id)).form(&[("client_id", client_id), ("scopes", &SCOPES.join(" "))]).send().await.map_err(net)?;
    if !resp.status().is_success() {
        return Err(twitch_error(resp).await);
    }
    resp.json().await.map_err(net)
}

pub async fn poll_device_token(http: &reqwest::Client, urls: &Urls, client_id: &str, device_code: &str) -> Result<Poll, AuthError> {
    let resp = http
        .post(format!("{}/oauth2/token", urls.id))
        .form(&[("client_id", client_id), ("scopes", &SCOPES.join(" ")), ("device_code", device_code), ("grant_type", "urn:ietf:params:oauth:grant-type:device_code")])
        .send()
        .await
        .map_err(net)?;
    if resp.status().is_success() {
        let t: TokenResponse = resp.json().await.map_err(net)?;
        return Ok(Poll::Done { access_token: t.access_token, refresh_token: t.refresh_token.unwrap_or_default(), expires_at: now() + t.expires_in.unwrap_or(3600), scopes: t.scope });
    }
    match twitch_error(resp).await {
        AuthError::Twitch(_, m) if m == "authorization_pending" => Ok(Poll::Pending),
        AuthError::Twitch(_, m) if m == "slow_down" => Ok(Poll::SlowDown),
        e => Err(e),
    }
}

pub async fn refresh(http: &reqwest::Client, urls: &Urls, client_id: &str, old: &TokenSet) -> Result<TokenSet, AuthError> {
    let resp = http
        .post(format!("{}/oauth2/token", urls.id))
        .form(&[("client_id", client_id), ("grant_type", "refresh_token"), ("refresh_token", old.refresh_token.as_str())])
        .send()
        .await
        .map_err(net)?;
    if !resp.status().is_success() {
        return Err(twitch_error(resp).await);
    }
    let t: TokenResponse = resp.json().await.map_err(net)?;
    Ok(TokenSet {
        access_token: t.access_token,
        // refresh tokens of public clients are single-use: a new one comes back
        refresh_token: t.refresh_token.unwrap_or_else(|| old.refresh_token.clone()),
        expires_at: now() + t.expires_in.unwrap_or(3600),
        scopes: if t.scope.is_empty() { old.scopes.clone() } else { t.scope },
        user_id: old.user_id.clone(),
        login: old.login.clone(),
    })
}

/// Twitch wants apps to validate tokens on start and hourly. None = invalid.
pub async fn validate(http: &reqwest::Client, urls: &Urls, access_token: &str) -> Result<Option<ValidateInfo>, AuthError> {
    let resp = http.get(format!("{}/oauth2/validate", urls.id)).header("Authorization", format!("OAuth {access_token}")).send().await.map_err(net)?;
    if resp.status().as_u16() == 401 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(twitch_error(resp).await);
    }
    resp.json().await.map(Some).map_err(net)
}

pub async fn revoke(http: &reqwest::Client, urls: &Urls, client_id: &str, access_token: &str) {
    let _ = http.post(format!("{}/oauth2/revoke", urls.id)).form(&[("client_id", client_id), ("token", access_token)]).send().await;
}

pub type BoxFut<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Where EventSub gets its access token (the app's account, or a test's).
pub trait Tokens: Send + Sync {
    /// A token valid for a few more minutes.
    fn token(&self) -> BoxFut<'_, Result<String, String>>;
    /// Twitch refused `stale`: a fresh one.
    fn refreshed(&self, stale: String) -> BoxFut<'_, Result<String, String>>;
}

/// What the settings show about the Twitch account.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct TwitchStatus {
    /// A client ID is built in or entered (else login isn't possible).
    pub available: bool,
    pub own_client_id: bool,
    pub login: Option<String>,
    /// EventSub runs and the subscriptions below are active.
    pub connected: bool,
    pub subscribed: Vec<String>,
    pub error: Option<String>,
    /// A login waits for the streamer to confirm the code.
    pub pending: Option<PendingLogin>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PendingLogin {
    pub user_code: String,
    pub verification_uri: String,
    /// Unix seconds.
    pub expires_at: i64,
}

/// The account at runtime: tokens (cached from the credential store),
/// status, the running EventSub connection.
pub struct Account {
    pub http: reqwest::Client,
    pub urls: Urls,
    pub status: RwLock<TwitchStatus>,
    tokens: Mutex<Option<TokenSet>>,
    loaded: std::sync::atomic::AtomicBool,
    refresh_lock: tokio::sync::Mutex<()>,
    /// Ends the running EventSub connection when dropped or set.
    pub stop: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    /// Bumped to cancel a waiting login.
    pub login_gen: std::sync::atomic::AtomicU64,
}

impl Account {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            urls: Urls::default(),
            status: RwLock::new(TwitchStatus::default()),
            tokens: Mutex::new(None),
            loaded: Default::default(),
            refresh_lock: tokio::sync::Mutex::new(()),
            stop: Mutex::new(None),
            login_gen: Default::default(),
        }
    }

    pub fn tokens(&self, secrets: &crate::secrets::Secrets) -> Option<TokenSet> {
        use std::sync::atomic::Ordering;
        if !self.loaded.swap(true, Ordering::Relaxed) {
            *self.tokens.lock() = secrets.get(TOKEN_KEY).and_then(|s| serde_json::from_str(&s).ok());
        }
        self.tokens.lock().clone()
    }

    pub fn store(&self, secrets: &crate::secrets::Secrets, t: Option<&TokenSet>) -> Result<(), String> {
        let raw = t.map(|t| serde_json::to_string(t).unwrap_or_default());
        secrets.set(TOKEN_KEY, raw.as_deref()).map_err(|e| e.to_string())?;
        *self.tokens.lock() = t.cloned();
        self.loaded.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// A token valid for at least five more minutes (refreshed if needed;
    /// refreshes are serialized: each refresh token works once).
    pub async fn access_token(&self, secrets: &crate::secrets::Secrets, client_id: &str, stale: Option<&str>) -> Result<String, String> {
        let _g = self.refresh_lock.lock().await;
        let t = self.tokens(secrets).ok_or("Not logged in to Twitch.")?;
        let still_good = t.expires_at - now() > 300 && stale.is_none_or(|s| s != t.access_token);
        if still_good {
            return Ok(t.access_token);
        }
        match refresh(&self.http, &self.urls, client_id, &t).await {
            Ok(fresh) => {
                self.store(secrets, Some(&fresh))?;
                Ok(fresh.access_token)
            }
            Err(e @ AuthError::Twitch(..)) => {
                // revoked, password changed, unused for 30 days: log in again
                let _ = self.store(secrets, None);
                Err(format!("The Twitch login has expired ({e}). Log in again in the settings."))
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

/// The app's account as EventSub's token source.
pub struct AppTokens(pub Arc<crate::state::AppState>);

impl Tokens for AppTokens {
    fn token(&self) -> BoxFut<'_, Result<String, String>> {
        Box::pin(async move {
            let id = crate::twitch_cmds::client_id(&self.0).ok_or("no Twitch client ID")?;
            self.0.twitch.access_token(&self.0.secrets, &id, None).await
        })
    }
    fn refreshed(&self, stale: String) -> BoxFut<'_, Result<String, String>> {
        Box::pin(async move {
            let id = crate::twitch_cmds::client_id(&self.0).ok_or("no Twitch client ID")?;
            self.0.twitch.access_token(&self.0.secrets, &id, Some(&stale)).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_twitch_answers() {
        let dc: DeviceCode = serde_json::from_str(r#"{"device_code":"abc","expires_in":1800,"interval":5,"user_code":"QBQLNSHC","verification_uri":"https://www.twitch.tv/activate?device-code=QBQLNSHC"}"#).unwrap();
        assert_eq!((dc.user_code.as_str(), dc.interval), ("QBQLNSHC", 5));
        let t: TokenResponse = serde_json::from_str(r#"{"access_token":"a","expires_in":14400,"refresh_token":"r","scope":["moderator:read:followers"],"token_type":"bearer"}"#).unwrap();
        assert_eq!((t.access_token.as_str(), t.refresh_token.as_deref(), t.scope.len()), ("a", Some("r"), 1));
        let v: ValidateInfo = serde_json::from_str(r#"{"client_id":"x","login":"zcarecrow","scopes":["channel:read:redemptions"],"user_id":"71498656","expires_in":5520838}"#).unwrap();
        assert_eq!(v.user_id, "71498656");
    }
}
