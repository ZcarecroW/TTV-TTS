//! Limits for servers that can be reached from the internet. Without them
//! anybody could open connections that never send a request and use up
//! every slot (a "slowloris" attack), or fill the memory with lists of
//! addresses that guessed wrong.
//!
//! - [`Connections`]: open connections in total and per address.
//! - [`Strikes`]: wrong guesses per address; locked out beyond a limit.
//! - [`http1`] / [`http2`]: a request's headers must arrive within
//!   [`HEADER_TIMEOUT`], and an idle keep-alive connection ends after it too.
//!
//! IPv6 addresses count by their /64 network: one home (or one rented
//! server) gets a whole /64, so counting single addresses would limit nothing.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hyper_util::rt::{TokioExecutor, TokioTimer};
use parking_lot::Mutex;

/// A request's headers must arrive within this; an idle keep-alive
/// connection is closed after it.
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(20);

/// An address in the local network (or this PC, or a VPN like Tailscale's
/// 100.64.0.0/10).
pub fn is_local(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_loopback() || v.is_private() || v.is_link_local() || (o[0] == 100 && (64..128).contains(&o[1]))
        }
        IpAddr::V6(v) => v.is_loopback() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// The address limits count by: IPv4 as is, IPv6 by its /64 network.
pub fn key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v) => {
            let s = v.segments();
            IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        v4 => v4,
    }
}

#[derive(Default)]
struct Counts {
    total: usize,
    per: HashMap<IpAddr, usize>,
}

/// Open connections in total and per address.
#[derive(Clone)]
pub struct Connections {
    counts: Arc<Mutex<Counts>>,
    max_total: usize,
    max_per: usize,
}

/// One open connection; frees its place when dropped.
pub struct Slot {
    counts: Arc<Mutex<Counts>>,
    key: Option<IpAddr>,
}

impl Connections {
    pub fn new(max_total: usize, max_per: usize) -> Self {
        Self { counts: Arc::default(), max_total, max_per }
    }

    /// A place for a new connection from `ip`, or `None` beyond the limits.
    /// `per_address`: false for addresses that only count towards the total
    /// (this PC and the local network).
    pub fn take(&self, ip: IpAddr, per_address: bool) -> Option<Slot> {
        let mut c = self.counts.lock();
        if c.total >= self.max_total {
            return None;
        }
        let key = per_address.then(|| key(ip));
        if let Some(k) = key {
            let n = c.per.get(&k).copied().unwrap_or(0);
            if n >= self.max_per {
                return None;
            }
            c.per.insert(k, n + 1);
        }
        c.total += 1;
        Some(Slot { counts: self.counts.clone(), key })
    }

    /// Open connections right now.
    pub fn open(&self) -> usize {
        self.counts.lock().total
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut c = self.counts.lock();
        c.total = c.total.saturating_sub(1);
        if let Some(k) = self.key
            && let Some(n) = c.per.get_mut(&k)
        {
            *n -= 1;
            if *n == 0 {
                c.per.remove(&k);
            }
        }
    }
}

/// Wrong guesses (keys, tokens, links) per address: after `limit` within
/// `window` the address is locked out until the window has passed. Memory
/// stays bounded under a flood from many addresses.
pub struct Strikes {
    map: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    limit: u32,
    window: Duration,
    cap: usize,
}

impl Strikes {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self::with_cap(limit, window, 4096)
    }

    pub fn with_cap(limit: u32, window: Duration, cap: usize) -> Self {
        Self { map: Mutex::new(HashMap::new()), limit, window, cap }
    }

    pub fn blocked(&self, ip: IpAddr) -> bool {
        let k = key(ip);
        let mut m = self.map.lock();
        match m.get(&k) {
            Some((n, since)) if since.elapsed() < self.window => *n >= self.limit,
            Some(_) => {
                m.remove(&k);
                false
            }
            None => false,
        }
    }

    /// Counts a wrong guess; returns how many the address made in the window.
    pub fn strike(&self, ip: IpAddr) -> u32 {
        let k = key(ip);
        let mut m = self.map.lock();
        if m.len() >= self.cap && !m.contains_key(&k) {
            let window = self.window;
            m.retain(|_, (_, since)| since.elapsed() < window);
            // still full: keep the addresses that are locked out
            if m.len() >= self.cap {
                let limit = self.limit;
                m.retain(|_, (n, _)| *n >= limit);
            }
            if m.len() >= self.cap {
                m.clear();
            }
        }
        let e = m.entry(k).or_insert((0, Instant::now()));
        if e.1.elapsed() >= self.window {
            *e = (0, Instant::now());
        }
        e.0 = e.0.saturating_add(1);
        e.0
    }

    /// Addresses tracked right now (for tests).
    pub fn len(&self) -> usize {
        self.map.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// HTTP/1.1 for connections from outside: the header deadline, idle
/// keep-alive connections end after it.
pub fn http1() -> hyper::server::conn::http1::Builder {
    let mut b = hyper::server::conn::http1::Builder::new();
    b.timer(TokioTimer::new()).header_read_timeout(HEADER_TIMEOUT);
    b
}

/// HTTP/2 (the proxy's own API, when TLS agreed on it): connections are
/// pinged, so dead ones end, and carry at most 128 requests at once. The
/// protocol is picked from TLS (ALPN), never guessed from the first bytes:
/// hyper-util's auto builder waits for those forever (even with
/// `http1_only`, its upgradeable connection reads them first).
pub fn http2() -> hyper::server::conn::http2::Builder<TokioExecutor> {
    let mut b = hyper::server::conn::http2::Builder::new(TokioExecutor::new());
    b.timer(TokioTimer::new())
        .keep_alive_interval(Some(Duration::from_secs(30)))
        .keep_alive_timeout(Duration::from_secs(20))
        .max_concurrent_streams(128);
    b
}

/// A connection that must send its first byte within [`HEADER_TIMEOUT`]:
/// HTTP/2 waits for the client's preface without a deadline of its own, so
/// a client that agreed on HTTP/2 and then stayed silent kept its place.
pub struct FirstByte<I> {
    io: I,
    deadline: Option<std::pin::Pin<Box<tokio::time::Sleep>>>,
}

impl<I> FirstByte<I> {
    pub fn new(io: I) -> Self {
        Self::within(io, HEADER_TIMEOUT)
    }

    pub fn within(io: I, limit: Duration) -> Self {
        Self { io, deadline: Some(Box::pin(tokio::time::sleep(limit))) }
    }
}

impl<I: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for FirstByte<I> {
    fn poll_read(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> std::task::Poll<std::io::Result<()>> {
        use std::future::Future;
        use std::task::Poll;
        let this = self.get_mut();
        let before = buf.filled().len();
        match std::pin::Pin::new(&mut this.io).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                if buf.filled().len() > before {
                    this.deadline = None;
                }
                Poll::Ready(Ok(()))
            }
            Poll::Pending => {
                let expired = this.deadline.as_mut().is_some_and(|d| d.as_mut().poll(cx).is_ready());
                if expired { Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "the client sent nothing"))) } else { Poll::Pending }
            }
            r => r,
        }
    }
}

impl<I: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for FirstByte<I> {
    fn poll_write(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &[u8]) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().io).poll_write(cx, buf)
    }
    fn poll_write_vectored(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, bufs: &[std::io::IoSlice<'_>]) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().io).poll_write_vectored(cx, bufs)
    }
    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }
    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().io).poll_flush(cx)
    }
    fn poll_shutdown(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().io).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv6_counts_by_network() {
        let a: IpAddr = "2a02:810d:1:2:aaaa:bbbb:cccc:dddd".parse().unwrap();
        let b: IpAddr = "2a02:810d:1:2::1".parse().unwrap();
        let c: IpAddr = "2a02:810d:1:3::1".parse().unwrap();
        assert_eq!(key(a), key(b));
        assert_ne!(key(a), key(c));
        assert_eq!(key("::ffff:1.2.3.4".parse().unwrap()), "1.2.3.4".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn connections_are_capped_and_freed() {
        let c = Connections::new(3, 2);
        let ip: IpAddr = "203.0.113.5".parse().unwrap();
        let a = c.take(ip, true).unwrap();
        let _b = c.take(ip, true).unwrap();
        assert!(c.take(ip, true).is_none(), "two per address");
        let other = c.take("203.0.113.6".parse().unwrap(), true).unwrap();
        assert!(c.take("203.0.113.7".parse().unwrap(), true).is_none(), "three in total");
        drop(a);
        assert!(c.take(ip, true).is_some(), "freed");
        drop(other);
        // refused addresses leave nothing behind
        let c = Connections::new(1, 1);
        let _one = c.take(ip, true).unwrap();
        for i in 0..100u8 {
            assert!(c.take(IpAddr::from([198, 51, 100, i]), true).is_none());
        }
        assert_eq!(c.counts.lock().per.len(), 1);
        // local addresses only count towards the total
        let c = Connections::new(10, 1);
        let lan: IpAddr = "192.168.1.2".parse().unwrap();
        let _x: Vec<Slot> = (0..5).map(|_| c.take(lan, false).unwrap()).collect();
        assert_eq!(c.open(), 5);
    }

    #[test]
    fn strikes_lock_out_and_stay_small() {
        let s = Strikes::with_cap(3, Duration::from_secs(60), 100);
        let ip: IpAddr = "203.0.113.9".parse().unwrap();
        for _ in 0..3 {
            assert!(!s.blocked(ip));
            s.strike(ip);
        }
        assert!(s.blocked(ip));
        // the same /64 is the same address
        let v6: IpAddr = "2001:db8:1:2::1".parse().unwrap();
        for i in 0..3u16 {
            s.strike(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, 2, 9, 9, 9, i)));
        }
        assert!(s.blocked(v6));
        // a flood from many addresses: bounded, the locked-out ones stay
        for i in 0..10_000u32 {
            s.strike(IpAddr::from(i.to_be_bytes()));
        }
        assert!(s.len() <= 100, "{}", s.len());
        assert!(s.blocked(ip) && s.blocked(v6));
    }

    /// The deadline really closes connections: a request that never ends its
    /// headers, and a keep-alive connection that stays idle after a request.
    #[tokio::test]
    async fn silent_http2_connections_are_closed() {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = axum::Router::new().route("/", axum::routing::get(|| async { "hi" }));
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let svc = hyper_util::service::TowerToHyperService::new(router);
            let io = hyper_util::rt::TokioIo::new(FirstByte::within(stream, Duration::from_millis(300)));
            let _ = http2().serve_connection(io, svc).await;
        });
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut buf = [0u8; 256];
        let t = Instant::now();
        let closed = loop {
            match tokio::time::timeout(Duration::from_secs(3), s.read(&mut buf)).await {
                Ok(Ok(0)) | Ok(Err(_)) => break true,
                Ok(Ok(_)) => continue, // the server's SETTINGS
                Err(_) => break false,
            }
        };
        assert!(closed && t.elapsed() < Duration::from_secs(2), "silent HTTP/2 client: {:?}", t.elapsed());
    }

    /// Normal HTTP/2 traffic passes, also long after the first-byte deadline.
    #[tokio::test]
    async fn http2_requests_pass_the_first_byte_deadline() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = axum::Router::new().route("/", axum::routing::get(|| async { "hi" }));
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let svc = hyper_util::service::TowerToHyperService::new(router.clone());
                tokio::spawn(async move {
                    let io = hyper_util::rt::TokioIo::new(FirstByte::within(stream, Duration::from_millis(200)));
                    let _ = http2().serve_connection(io, svc).await;
                });
            }
        });
        let client = reqwest::Client::builder().http2_prior_knowledge().build().unwrap();
        let url = format!("http://{addr}/");
        for round in 0..3 {
            let r = client.get(&url).send().await.unwrap();
            assert_eq!(r.version(), reqwest::Version::HTTP_2);
            assert_eq!(r.text().await.unwrap(), "hi", "round {round}");
            // the same connection, idle past the deadline
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
    }

    #[tokio::test]
    async fn slow_and_idle_connections_are_closed() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = axum::Router::new().route("/", axum::routing::get(|| async { "hi" }));
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let svc = hyper_util::service::TowerToHyperService::new(router.clone());
                tokio::spawn(async move {
                    let mut b = http1();
                    b.header_read_timeout(Duration::from_millis(300));
                    let _ = b.serve_connection(hyper_util::rt::TokioIo::new(stream), svc).with_upgrades().await;
                });
            }
        });
        let closed_within = |mut s: tokio::net::TcpStream, limit: Duration| async move {
            let mut buf = [0u8; 1024];
            let t = Instant::now();
            loop {
                match tokio::time::timeout(limit, s.read(&mut buf)).await {
                    Ok(Ok(0)) | Ok(Err(_)) => return Some(t.elapsed()),
                    Ok(Ok(_)) => continue,
                    Err(_) => return None,
                }
            }
        };
        // headers that never end
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET / HTTP/1.1
Host: x
X-Slow: ").await.unwrap();
        let t = closed_within(s, Duration::from_secs(3)).await.expect("slow headers: closed");
        assert!(t < Duration::from_secs(2), "{t:?}");
        // a request, then idle keep-alive
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET / HTTP/1.1
Host: x

").await.unwrap();
        let mut buf = [0u8; 256];
        let n = s.read(&mut buf).await.unwrap();
        assert!(std::str::from_utf8(&buf[..n]).unwrap().ends_with("hi"));
        let t = closed_within(s, Duration::from_secs(3)).await.expect("idle keep-alive: closed");
        assert!(t < Duration::from_secs(2), "{t:?}");
        // a connection that never sends a byte (auto would wait for the first ones forever)
        let s = tokio::net::TcpStream::connect(addr).await.unwrap();
        let t = closed_within(s, Duration::from_secs(3)).await.expect("silent: closed");
        assert!(t < Duration::from_secs(2), "{t:?}");
    }

}
