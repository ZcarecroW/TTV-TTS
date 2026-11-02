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
    token: String,
    sink: Arc<ObsSink>,
    online: bool,
}

impl Links for OneLink {
    fn sink(&self, token: &str, from: IpAddr) -> Result<Arc<ObsSink>, Refusal> {
        if token != self.token {
            return Err(Refusal::Unknown);
        }
        if !self.online && !obs::is_local(from) {
            return Err(Refusal::NotOnline);
        }
        Ok(self.sink.clone())
    }
}

async fn serve(links: Arc<OneLink>) -> String {
    let app = obs::router(links);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move { axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap() });
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
    let base = serve(Arc::new(OneLink { token: token.clone(), sink: sink.clone(), online: false })).await;

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
    let base = serve(Arc::new(OneLink { token: token.clone(), sink: ObsSink::new(), online: false })).await;
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
}
