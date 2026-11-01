//! VCP's speed is measured only on sentences spoken right after speaking:
//! after a design (or recognition, translation) VCP switches models first.

use std::time::Duration;

use base64::Engine as _;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn wav(secs: f32) -> Vec<u8> {
    let n = (secs * 24_000.0) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    for v in [16u32.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 24_000u32.to_le_bytes().to_vec(), 48_000u32.to_le_bytes().to_vec(), 2u16.to_le_bytes().to_vec(), 16u16.to_le_bytes().to_vec()] {
        b.extend_from_slice(&v);
    }
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 2).to_le_bytes());
    b.resize(44 + n as usize * 2, 0);
    b
}

async fn serve() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let speech = format!("{{\"success\":true,\"status\":\"completed\",\"audio_base64\":\"{}\"}}", base64::engine::general_purpose::STANDARD.encode(wav(2.0)));
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else { return };
            let speech = speech.clone();
            tokio::spawn(async move {
                loop {
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
                    let body = if head.contains("/generation/clone") { speech.clone() } else { "{}".to_string() };
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

#[tokio::test]
async fn only_speaking_after_speaking_is_measured() {
    let base = serve().await;
    let chat = ttv_vcp::Client::new(&base, None);
    let job = chat.background();
    let speak = async |c: &ttv_vcp::Client| {
        let _: serde_json::Value = c.post("/api/v1/generation/clone?sync=true", &serde_json::json!({}), Duration::from_secs(5)).await.unwrap();
    };
    let design = async |c: &ttv_vcp::Client| {
        let _: serde_json::Value = c.post("/api/v1/generation/design?sync=true", &serde_json::json!({}), Duration::from_secs(5)).await.unwrap();
    };
    ttv_vcp::perf::set_model("speed-test");
    speak(&chat).await; // first one: nothing known before
    assert_eq!(ttv_vcp::perf::speed().recent, 0);
    speak(&chat).await; // after speaking: measured
    assert_eq!(ttv_vcp::perf::speed().recent, 1);
    design(&job).await;
    speak(&chat).await; // after a design: VCP switched models
    assert_eq!(ttv_vcp::perf::speed().recent, 1);
    speak(&job).await; // computing jobs don't measure
    assert_eq!(ttv_vcp::perf::speed().recent, 1);
    speak(&chat).await;
    let s = ttv_vcp::perf::speed();
    assert_eq!(s.recent, 2);
    // 2 s of audio from a local fake: far faster than real time
    assert!(s.now.unwrap() < 0.5, "{s:?}");
}
