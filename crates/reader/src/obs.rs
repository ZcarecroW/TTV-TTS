//! OBS links: a browser source in OBS that plays the voices and nothing else.
//!
//! The page is a few hundred bytes of script without anything visible: it
//! opens a WebSocket and plays the PCM clips it receives back to back with
//! Web Audio. OBS gets the sound as its own source (with "Control audio via
//! OBS"), so it has a fader and filters like any other.
//!
//! Wire format, per clip: a binary frame `[1, 0, 0, 0, rate u32 LE, PCM16 LE …]`
//! (mono). Text frames: `{"t":"stop"}` stops everything that's queued (skip).
//! A link is `/o/<token>`; the token is the only secret. Addresses outside the
//! local network are refused unless the link is meant for the internet, and
//! an address that guesses wrong tokens is locked out for a while.

use std::collections::VecDeque;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::{Extension, Router};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use bytes::Bytes;
use parking_lot::Mutex;
use tokio::sync::broadcast;

use crate::output::Output;
use ttv_net::Secure;

/// The browser source: no markup to render, no requests but the socket.
pub const PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>TTV-TTS</title><style>html,body{margin:0;background:transparent}</style><script>
(()=>{let x,t=0,w=500;const n=new Set,u=location.href.split(/[?#]/)[0].replace(/\/$/,"").replace(/^http/,"ws")+"/ws",
go=()=>{const s=new WebSocket(u);s.binaryType="arraybuffer";s.onopen=()=>w=500;s.onclose=()=>{setTimeout(go,w);w=Math.min(w*2,15e3)};
s.onmessage=e=>{if(typeof e.data=="string"){if(JSON.parse(e.data).t=="stop"){n.forEach(a=>{try{a.stop()}catch{}});n.clear();t=0}return}
x??=new AudioContext;x.state=="suspended"&&x.resume();const d=new DataView(e.data),p=new Int16Array(e.data,8),b=x.createBuffer(1,p.length,d.getUint32(4,1)),c=b.getChannelData(0);
for(let i=0;i<p.length;i++)c[i]=p[i]/32768;const a=x.createBufferSource();a.buffer=b;a.connect(x.destination);const at=Math.max(x.currentTime+.05,t);
a.start(at);t=at+b.duration;n.add(a);a.onended=()=>n.delete(a)}};onclick=()=>x?.resume();go()})()
</script>"#;

/// Listeners per link (OBS, its preview, a second PC …).
const MAX_LISTENERS: usize = 8;
/// What a clip takes to reach the listener and start (scheduling + network).
const LATENCY: Duration = Duration::from_millis(250);

/// A new link token: 16 characters (~94 bits).
pub fn new_token() -> String {
    crate::random_key()[..16].to_string()
}

/// One OBS link's audio. Clips go to every connected browser source.
pub struct ObsSink {
    tx: broadcast::Sender<Frame>,
    listeners: AtomicUsize,
    next: AtomicU64,
    clock: Mutex<Clock>,
}

#[derive(Clone)]
enum Frame {
    Clip(Bytes),
    Stop,
}

struct Clock {
    /// When the audio queued so far ends at the listener.
    playhead: Instant,
    /// (clip id, when it has played)
    ends: VecDeque<(u64, Instant)>,
    /// Clips up to this id were skipped.
    skipped: u64,
}

impl ObsSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            tx: broadcast::channel(64).0,
            listeners: AtomicUsize::new(0),
            next: AtomicU64::new(1),
            clock: Mutex::new(Clock { playhead: Instant::now(), ends: VecDeque::new(), skipped: 0 }),
        })
    }

    /// Browser sources connected right now.
    pub fn listeners(&self) -> usize {
        self.listeners.load(Ordering::Relaxed)
    }
}

/// Mono samples as a clip frame (PCM16).
fn encode(samples: &[f32], rate: u32) -> Bytes {
    let mut b = Vec::with_capacity(8 + samples.len() * 2);
    b.extend_from_slice(&[1, 0, 0, 0]);
    b.extend_from_slice(&rate.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    Bytes::from(b)
}

impl Output for ObsSink {
    fn play(&self, samples: Vec<f32>, rate: u32) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let secs = samples.len() as f32 / rate.max(1) as f32;
        let _ = self.tx.send(Frame::Clip(encode(&samples, rate)));
        let mut c = self.clock.lock();
        let now = Instant::now();
        c.playhead = c.playhead.max(now) + Duration::from_secs_f32(secs);
        let end = c.playhead + LATENCY;
        c.ends.push_back((id, end));
        while c.ends.front().is_some_and(|(_, t)| *t + Duration::from_secs(60) < now) {
            c.ends.pop_front();
        }
        id
    }

    fn is_finished(&self, id: u64) -> bool {
        let c = self.clock.lock();
        id <= c.skipped || c.ends.iter().find(|(i, _)| *i == id).is_none_or(|(_, t)| Instant::now() >= *t)
    }

    fn skip_all(&self) {
        let _ = self.tx.send(Frame::Stop);
        let mut c = self.clock.lock();
        c.skipped = self.next.load(Ordering::Relaxed).saturating_sub(1);
        c.playhead = Instant::now();
        c.ends.clear();
    }

    fn listening(&self) -> bool {
        self.listeners() > 0
    }
}

/// Why a link can't be opened.
pub enum Refusal {
    /// No such link (counts as a guess).
    Unknown,
    /// The link works only in the local network.
    NotOnline,
    /// From the internet, the link must use HTTPS (this server has a certificate).
    NeedsHttps,
}

/// The links a server knows: token -> audio, and who may open it.
pub trait Links: Send + Sync + 'static {
    /// `secure`: the request came over HTTPS.
    fn sink(&self, token: &str, from: IpAddr, secure: bool) -> Result<Arc<ObsSink>, Refusal>;
}

/// An address in the local network (or this PC, or a VPN like Tailscale).
pub use ttv_net::limits::is_local;

/// Addresses that guessed wrong tokens: locked out for 15 minutes after 20
/// (IPv6 by /64, bounded memory under a flood).
struct Guesses(ttv_net::limits::Strikes);

impl Default for Guesses {
    fn default() -> Self {
        Self(ttv_net::limits::Strikes::new(Self::LIMIT, Self::WINDOW))
    }
}

impl Guesses {
    const LIMIT: u32 = 20;
    const WINDOW: Duration = Duration::from_secs(900);

    fn blocked(&self, ip: IpAddr) -> bool {
        self.0.blocked(ip)
    }

    fn wrong(&self, ip: IpAddr) {
        self.0.strike(ip);
    }
}

struct Served<L> {
    links: Arc<L>,
    guesses: Guesses,
}

type S<L> = State<Arc<Served<L>>>;

/// The routes of OBS links: `/o/<token>` (the page) and `/o/<token>/ws`.
/// Serve with `into_make_service_with_connect_info::<SocketAddr>()`.
pub fn router<L: Links>(links: Arc<L>) -> Router {
    let served = Arc::new(Served { links, guesses: Guesses::default() });
    Router::new().route("/o/{token}", get(page::<L>)).route("/o/{token}/ws", get(socket::<L>)).with_state(served)
}

fn refuse(status: StatusCode, text: &'static str) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store")], text).into_response()
}

fn check<L: Links>(s: &Served<L>, token: &str, from: IpAddr, secure: bool) -> Result<Arc<ObsSink>, (StatusCode, &'static str)> {
    let from = from.to_canonical();
    if s.guesses.blocked(from) {
        return Err((StatusCode::TOO_MANY_REQUESTS, "Too many wrong links from this address. Try again later."));
    }
    match s.links.sink(token, from, secure) {
        Ok(sink) => Ok(sink),
        Err(Refusal::Unknown) => {
            s.guesses.wrong(from);
            Err((StatusCode::NOT_FOUND, "This TTV-TTS link doesn't exist (any more)."))
        }
        Err(Refusal::NotOnline) => Err((StatusCode::FORBIDDEN, "This TTV-TTS link only works in the local network.")),
        Err(Refusal::NeedsHttps) => Err((StatusCode::FORBIDDEN, "This TTV-TTS link needs https:// now. Copy the new link from TTV-TTS.")),
    }
}

async fn page<L: Links>(State(s): S<L>, Path(token): Path<String>, ConnectInfo(addr): ConnectInfo<SocketAddr>, secure: Option<Extension<Secure>>) -> Response {
    if let Err((code, why)) = check(&s, &token, addr.ip(), secure.is_some_and(|Extension(Secure(x))| x)) {
        return refuse(code, why);
    }
    let mut r = PAGE.into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert("x-robots-tag", HeaderValue::from_static("noindex"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    r
}

async fn socket<L: Links>(State(s): S<L>, Path(token): Path<String>, ConnectInfo(addr): ConnectInfo<SocketAddr>, secure: Option<Extension<Secure>>, ws: WebSocketUpgrade) -> Response {
    let sink = match check(&s, &token, addr.ip(), secure.is_some_and(|Extension(Secure(x))| x)) {
        Ok(sink) => sink,
        Err((code, why)) => return refuse(code, why),
    };
    if sink.listeners() >= MAX_LISTENERS {
        return refuse(StatusCode::TOO_MANY_REQUESTS, "Too many listeners on this link.");
    }
    // a listener only answers pings: tiny messages are plenty
    ws.max_message_size(4096).max_frame_size(4096).on_upgrade(move |socket| listen(sink, socket))
}

/// Counts a listener while it's connected.
struct Listener(Arc<ObsSink>);

impl Drop for Listener {
    fn drop(&mut self) {
        self.0.listeners.fetch_sub(1, Ordering::Relaxed);
    }
}

async fn listen(sink: Arc<ObsSink>, mut socket: WebSocket) {
    let mut rx = sink.tx.subscribe();
    sink.listeners.fetch_add(1, Ordering::Relaxed);
    let _count = Listener(sink.clone());
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    ping.tick().await;
    loop {
        tokio::select! {
            f = rx.recv() => {
                let msg = match f {
                    Ok(Frame::Clip(b)) => Message::Binary(b),
                    Ok(Frame::Stop) => Message::Text(r#"{"t":"stop"}"#.into()),
                    // too slow to keep up: drop what it missed
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if socket.send(msg).await.is_err() {
                    break;
                }
            }
            m = socket.recv() => match m {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
            _ = ping.tick() => {
                if socket.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_addresses() {
        for a in ["127.0.0.1", "192.168.178.25", "10.1.2.3", "172.20.0.5", "100.101.102.103", "::1", "fd00::1", "fe80::1", "::ffff:192.168.1.2"] {
            assert!(is_local(a.parse().unwrap()), "{a}");
        }
        for a in ["8.8.8.8", "84.150.1.2", "2a02:810d::1", "::ffff:8.8.8.8", "100.128.0.1"] {
            assert!(!is_local(a.parse().unwrap()), "{a}");
        }
    }

    #[test]
    fn clips_are_pcm16_with_their_rate() {
        let f = encode(&[0.0, 1.0, -1.0, 2.0], 24_000);
        assert_eq!(&f[..4], &[1, 0, 0, 0]);
        assert_eq!(u32::from_le_bytes(f[4..8].try_into().unwrap()), 24_000);
        let pcm: Vec<i16> = f[8..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(pcm, vec![0, 32767, -32767, 32767]);
    }

    #[test]
    fn the_clock_follows_the_queued_audio() {
        let s = ObsSink::new();
        let a = s.play(vec![0.0; 2400], 24_000); // 0.1 s
        let b = s.play(vec![0.0; 2400], 24_000);
        assert!(!s.is_finished(a) && !s.is_finished(b));
        std::thread::sleep(LATENCY + Duration::from_millis(120));
        assert!(s.is_finished(a) && !s.is_finished(b), "b starts after a");
        s.skip_all();
        assert!(s.is_finished(b), "skipped");
        assert!(!s.listening(), "nobody connected");
    }

    #[test]
    fn wrong_guesses_lock_an_address_out() {
        let g = Guesses::default();
        let ip: IpAddr = "84.150.1.2".parse().unwrap();
        for _ in 0..Guesses::LIMIT {
            assert!(!g.blocked(ip));
            g.wrong(ip);
        }
        assert!(g.blocked(ip));
        assert!(!g.blocked("84.150.1.3".parse().unwrap()));
    }
}
