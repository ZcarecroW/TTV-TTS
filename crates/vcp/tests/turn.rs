//! Work requests of one process never reach VCP at the same time; lookups
//! (GET) still pass while VCP works.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Default)]
struct Seen {
    active_posts: AtomicUsize,
    max_posts: AtomicUsize,
    gets_during_post: AtomicUsize,
    /// bodies of the POSTs in the order VCP got them
    order: parking_lot::Mutex<Vec<String>>,
}

async fn serve(seen: Arc<Seen>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { return };
            let seen = seen.clone();
            tokio::spawn(async move {
                loop {
                    // read one request (headers + content-length body)
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let head_end = loop {
                        let Ok(n) = s.read(&mut chunk).await else { return };
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            break i + 4;
                        }
                    };
                    let head = String::from_utf8_lossy(&buf[..head_end]).to_ascii_lowercase();
                    let len: usize = head.lines().find_map(|l| l.strip_prefix("content-length:")).map(|v| v.trim().parse().unwrap_or(0)).unwrap_or(0);
                    while buf.len() < head_end + len {
                        let Ok(n) = s.read(&mut chunk).await else { return };
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                    }
                    if head.starts_with("post") {
                        seen.order.lock().push(String::from_utf8_lossy(&buf[head_end..]).to_string());
                        let now = seen.active_posts.fetch_add(1, Ordering::SeqCst) + 1;
                        seen.max_posts.fetch_max(now, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(120)).await;
                        seen.active_posts.fetch_sub(1, Ordering::SeqCst);
                    } else if seen.active_posts.load(Ordering::SeqCst) > 0 {
                        seen.gets_during_post.fetch_add(1, Ordering::SeqCst);
                    }
                    let body = if head.starts_with("post") { "{}" } else { "[]" };
                    let res = format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len());
                    if s.write_all(res.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn work_requests_take_turns() {
    let seen = Arc::new(Seen::default());
    let base = serve(seen.clone()).await;
    let t0 = Instant::now();
    let mut tasks = Vec::new();
    for i in 0..4 {
        // separate clients (= separate connections), like the engine and the forwarder
        let c = ttv_vcp::Client::new(&base, None);
        tasks.push(tokio::spawn(async move {
            let path = if i % 2 == 0 { "/api/v1/generation/clone?sync=true" } else { "/api/v1/voice-design/generate" };
            c.post::<_, serde_json::Value>(path, &serde_json::json!({ "n": i }), Duration::from_secs(10)).await.unwrap();
        }));
    }
    // lookups are not held up
    let c = ttv_vcp::Client::new(&base, None);
    tokio::time::sleep(Duration::from_millis(60)).await;
    let g = Instant::now();
    let _: serde_json::Value = c.get("/api/v1/voices", Duration::from_secs(5)).await.unwrap();
    let get_ms = g.elapsed().as_millis();
    for t in tasks {
        t.await.unwrap();
    }
    assert_eq!(seen.max_posts.load(Ordering::SeqCst), 1, "two work requests reached VCP at once");
    assert!(t0.elapsed() >= Duration::from_millis(4 * 120), "{:?}", t0.elapsed());
    assert!(get_ms < 100, "a lookup waited {get_ms} ms");
    assert!(seen.gets_during_post.load(Ordering::SeqCst) >= 1, "the lookup ran while VCP worked");
    assert!(ttv_vcp::is_work(&ttv_vcp::Method::POST, "/api/v1/generation/clone?sync=true"));
    assert!(!ttv_vcp::is_work(&ttv_vcp::Method::POST, "/api/v1/generation/stop"));
    assert!(!ttv_vcp::is_work(&ttv_vcp::Method::POST, "/ttv/v1/jobs"));
    assert!(!ttv_vcp::is_work(&ttv_vcp::Method::GET, "/api/v1/voices"));
    assert!(ttv_vcp::is_work(&ttv_vcp::Method::GET, "/api/v1/streaming/abc/audio?format=wav"));
    assert!(!ttv_vcp::is_work(&ttv_vcp::Method::GET, "/api/v1/streaming/abc"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chat_goes_before_background_work() {
    let seen = Arc::new(Seen::default());
    let base = serve(seen.clone()).await;
    let post = |name: &'static str, background: bool, delay: u64| {
        let c = ttv_vcp::Client::new(&base, None);
        let c = if background { c.background() } else { c };
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            c.post::<_, serde_json::Value>("/api/v1/generation/clone?sync=true", &serde_json::json!({ "n": name }), Duration::from_secs(10)).await.unwrap();
        })
    };
    // chat A works, a job queues up (B), then more chat arrives (C): C first
    let a = post("A", false, 0);
    let b = post("B", true, 30);
    let c = post("C", false, 60);
    for t in [a, b, c] {
        t.await.unwrap();
    }
    let order: Vec<String> = seen.order.lock().iter().map(|b| b.chars().filter(|c| c.is_ascii_uppercase()).collect()).collect();
    assert_eq!(order, ["A", "C", "B"], "chat must not wait behind a job");
}
