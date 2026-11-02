//! OBS links over real sockets: the page, the audio socket, skips, refusals
//! and the lock-out after wrong guesses.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message;
use ttv_reader::obs::{self, Links, ObsSink, Refusal};
use ttv_reader::output::Output;

struct OneLink {
    token: std::sync::Mutex<String>,
    /// Every address counts as a stranger's for lock-outs.
    strict: bool,
    sink: Arc<ObsSink>,
    online: bool,
    /// Treat every address as one from the internet, with a certificate on the server.
    internet_https: bool,
}

impl Links for OneLink {
    fn sink(&self, token: &str, from: IpAddr, secure: bool) -> Result<Arc<ObsSink>, Refusal> {
        if token != *self.token.lock().unwrap() {
            return Err(Refusal::Unknown);
        }
        if self.internet_https {
            return if secure { Ok(self.sink.clone()) } else { Err(Refusal::NeedsHttps) };
        }
        if !self.online && !obs::is_local(from) {
            return Err(Refusal::NotOnline);
        }
        Ok(self.sink.clone())
    }

    fn strikes(&self, from: IpAddr) -> bool {
        self.strict || self.internet_https || !obs::is_local(from)
    }
}

fn link(token: &str, sink: Arc<ObsSink>, online: bool, internet_https: bool, strict: bool) -> Arc<OneLink> {
    Arc::new(OneLink { token: std::sync::Mutex::new(token.to_string()), strict, sink, online, internet_https })
}

async fn serve(links: Arc<OneLink>) -> String {
    serve_tls(links, None).await
}

/// Like the app and the proxy: HTTP and (with a certificate) HTTPS on one port.
async fn serve_tls(links: Arc<OneLink>, tls: Option<Arc<rustls::ServerConfig>>) -> String {
    let app = obs::router(links);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(ttv_net::serve::serve(listener, app, tls, std::future::pending()));
    base
}

async fn until(what: &str, f: impl Fn() -> bool) {
    let t = std::time::Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(5), "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_browser_source_gets_the_audio_and_the_skips() {
    let token = obs::new_token();
    let sink = ObsSink::new();
    let base = serve(link(&token, sink.clone(), false, false, false)).await;

    let page = reqwest::get(format!("http://{base}/o/{token}")).await.unwrap();
    assert_eq!(page.status(), 200);
    assert!(page.headers()["content-type"].to_str().unwrap().starts_with("text/html"));
    let html = page.text().await.unwrap();
    assert!(html.contains("AudioContext") && html.len() < 1200, "{} bytes", html.len());

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{base}/o/{token}/ws")).await.unwrap();
    until("the listener", || sink.listening()).await;
    let clip: Vec<f32> = (0..2400).map(|i| (i as f32 / 10.0).sin() * 0.5).collect();
    sink.play(clip, 24_000);
    let f = ws.next().await.unwrap().unwrap();
    let Message::Binary(b) = f else { panic!("{f:?}") };
    assert_eq!(b.len(), 8 + 2400 * 2);
    assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 24_000);
    sink.skip_all();
    let f = ws.next().await.unwrap().unwrap();
    assert_eq!(f, Message::Text(r#"{"t":"stop"}"#.into()));
    drop(ws);
    until("the listener to leave", || !sink.listening()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_links_are_refused_and_guessers_locked_out() {
    let token = obs::new_token();
    let base = serve(link(&token, ObsSink::new(), false, false, true)).await;
    let get = |path: String| {
        let url = format!("http://{base}{path}");
        async move { reqwest::get(url).await.unwrap().status().as_u16() }
    };
    assert_eq!(get("/o/not-a-real-link".into()).await, 404);
    assert!(tokio_tungstenite::connect_async(format!("ws://{base}/o/not-a-real-link/ws")).await.is_err());
    for _ in 0..20 {
        get("/o/guess".into()).await;
    }
    // locked out now, even with the right token
    assert_eq!(get(format!("/o/{token}")).await, 429);

    // this PC and the local network don't lock themselves out (an OBS with an old link)
    let token = obs::new_token();
    let base = serve(link(&token, ObsSink::new(), false, false, false)).await;
    for _ in 0..30 {
        assert_eq!(reqwest::get(format!("http://{base}/o/old-link")).await.unwrap().status(), 404);
    }
    assert_eq!(reqwest::get(format!("http://{base}/o/{token}")).await.unwrap().status(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_link_ends_the_old_one_at_once() {
    let token = obs::new_token();
    let sink = ObsSink::new();
    let links = link(&token, sink.clone(), false, false, false);
    let base = serve(links.clone()).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{base}/o/{token}/ws")).await.unwrap();
    until("the listener", || sink.listening()).await;
    // "New link": the next clip doesn't reach the old one
    *links.token.lock().unwrap() = obs::new_token();
    sink.play(vec![0.25; 480], 24_000);
    let f = ws.next().await.unwrap().unwrap();
    assert_eq!(f, Message::Text(r#"{"t":"gone"}"#.into()));
    assert!(matches!(ws.next().await, Some(Ok(Message::Close(_))) | None), "closed");
    until("the listener to leave", || !sink.listening()).await;
    // and the page stops reconnecting on "gone"
    let html = reqwest::get(format!("http://{base}/o/{}", links.token.lock().unwrap())).await.unwrap().text().await.unwrap();
    assert!(html.contains(r#"m=="gone""#) && html.contains("g||setTimeout(go"), "{html}");
}

#[tokio::test(flavor = "multi_thread")]
async fn from_the_internet_only_https_works_once_there_is_a_certificate() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec!["mystream.duckdns.org".to_string()]).unwrap().self_signed(&key).unwrap();
    let mut cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()], rustls::pki_types::PrivateKeyDer::Pkcs8(key.serialize_der().into()))
        .unwrap();
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    let token = obs::new_token();
    let sink = ObsSink::new();
    let base = serve_tls(link(&token, sink.clone(), true, true, false), Some(Arc::new(cfg))).await;
    let port: u16 = base.rsplit(':').next().unwrap().parse().unwrap();

    let plain = reqwest::get(format!("http://{base}/o/{token}")).await.unwrap();
    assert_eq!(plain.status(), 403);
    assert!(plain.text().await.unwrap().contains("https://"));

    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.der().clone()).unwrap();
    let client_tls = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    let client = reqwest::Client::builder().tls_backend_preconfigured(client_tls.clone()).resolve("mystream.duckdns.org", SocketAddr::from(([127, 0, 0, 1], port))).build().unwrap();
    let page = client.get(format!("https://mystream.duckdns.org:{port}/o/{token}")).send().await.unwrap();
    assert_eq!(page.status(), 200);

    // the audio socket over TLS (wss://), as the page opens it
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let connector = tokio_tungstenite::Connector::Rustls(Arc::new(client_tls));
    let (mut ws, _) = tokio_tungstenite::client_async_tls_with_config(format!("wss://mystream.duckdns.org:{port}/o/{token}/ws"), tcp, None, Some(connector)).await.unwrap();
    until("the listener", || sink.listening()).await;
    sink.play(vec![0.25; 480], 24_000);
    let f = ws.next().await.unwrap().unwrap();
    assert!(matches!(f, Message::Binary(ref b) if b.len() == 8 + 960), "{f:?}");
}
