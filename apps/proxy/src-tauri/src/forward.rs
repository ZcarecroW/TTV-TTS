//! Forwards every request outside `/ttv/v1` to Voice Creator Pro: same
//! method, path, query and body, with VCP's API key attached. Bodies stream
//! both ways without buffering, so speech audio reaches the app while VCP is
//! still generating it, and large uploads don't sit in memory.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, StatusCode, header};
use axum::response::Response;
use futures_util::StreamExt;

use crate::server::api_error;
use crate::state::AppState;

/// Hop-by-hop headers (RFC 9110 §7.6.1) and the client's credentials.
fn skip_request_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host" | "connection" | "keep-alive" | "te" | "trailer" | "transfer-encoding" | "upgrade"
            | "proxy-connection" | "proxy-authorization" | "authorization" | "x-api-key"
    )
}

fn skip_response_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection" | "keep-alive" | "te" | "trailer" | "transfer-encoding" | "upgrade" | "proxy-connection" | "proxy-authenticate"
    )
}

/// Path and query without the client's `api_key` parameter.
fn target_path(uri: &axum::http::Uri) -> String {
    let path = uri.path();
    match uri.query() {
        None => path.to_string(),
        Some(q) if !q.contains("api_key") => format!("{path}?{q}"),
        Some(q) => {
            let kept: Vec<&str> = q.split('&').filter(|p| !p.starts_with("api_key=") && *p != "api_key").collect();
            if kept.is_empty() { path.to_string() } else { format!("{path}?{}", kept.join("&")) }
        }
    }
}

/// Last request of the host's own apps (LAN), unix milliseconds: guests
/// over the internet wait for a pause in it.
static LAST_LAN: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Last time anyone made VCP work through this proxy (chat, previews,
/// guests), unix milliseconds. Computing jobs wait for a pause in it; plain
/// lookups (voice lists) don't count.
static LAST_WORK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

pub fn touch_work() {
    LAST_WORK.store(chrono::Utc::now().timestamp_millis(), std::sync::atomic::Ordering::Relaxed);
}

/// When VCP last worked for someone other than the computing jobs.
pub fn last_work() -> Option<std::time::Instant> {
    let last = LAST_WORK.load(std::sync::atomic::Ordering::Relaxed);
    if last <= 0 {
        return None;
    }
    let ago = (chrono::Utc::now().timestamp_millis() - last).max(0) as u64;
    std::time::Instant::now().checked_sub(std::time::Duration::from_millis(ago))
}

/// The host's apps used VCP within `quiet`.
pub fn lan_busy(quiet: std::time::Duration) -> bool {
    let last = LAST_LAN.load(std::sync::atomic::Ordering::Relaxed);
    last > 0 && chrono::Utc::now().timestamp_millis() - last < quiet.as_millis() as i64
}

pub async fn handler(State(state): State<Arc<AppState>>, req: Request) -> Response {
    crate::updater::touch();
    LAST_LAN.store(chrono::Utc::now().timestamp_millis(), std::sync::atomic::Ordering::Relaxed);
    if req.headers().contains_key(header::UPGRADE) {
        return api_error(StatusCode::NOT_IMPLEMENTED, "bad_request", "WebSocket connections to VCP are not forwarded.");
    }
    let vcp = state.vcp();
    let (parts, body) = req.into_parts();
    if ttv_vcp::exclusive() && ttv_vcp::is_work(&parts.method, parts.uri.path()) {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "benchmark", "Voice Creator Pro is being measured. It's free again when the benchmark ends.");
    }
    let url = format!("{}{}", vcp.base(), target_path(&parts.uri));
    // One job at a time at VCP, shared with the computing jobs; held until
    // the answer has been passed on.
    let turn = if ttv_vcp::is_work(&parts.method, parts.uri.path()) {
        touch_work();
        let t0 = std::time::Instant::now();
        let t = ttv_vcp::turn(parts.uri.path()).await;
        if t0.elapsed() > std::time::Duration::from_secs(5) {
            log::info!("{} waited {:.1} s for VCP", parts.uri.path(), t0.elapsed().as_secs_f32());
        }
        Some(t)
    } else {
        None
    };

    let mut rb = vcp.http().request(parts.method.clone(), url);
    for (name, value) in &parts.headers {
        if !skip_request_header(name) {
            rb = rb.header(name, value);
        }
    }
    if let Some(k) = vcp.key() {
        rb = rb.header("x-api-key", k);
    }
    let has_body = parts.headers.get(header::CONTENT_LENGTH).is_some_and(|v| v.as_bytes() != b"0")
        || parts.headers.contains_key(header::TRANSFER_ENCODING);
    if has_body {
        rb = rb.body(reqwest::Body::wrap_stream(body.into_data_stream()));
    }

    // speaking with a saved voice: how fast VCP is (see ttv_vcp::perf)
    let meter = (turn.as_ref().is_some_and(ttv_vcp::Turn::measures_speed) && !parts.uri.path().contains("stream")).then(|| Arc::new(parking_lot::Mutex::new(Meter::new())));
    let sent = rb.send().await;
    if turn.is_some() {
        // the quiet gap counts from the end of a long generation
        touch_work();
    }
    match sent {
        Ok(res) => {
            let mut builder = Response::builder().status(res.status());
            if let Some(h) = builder.headers_mut() {
                for (name, value) in res.headers() {
                    if !skip_response_header(name) {
                        h.append(name.clone(), value.clone());
                    }
                }
            }
            let fed = meter.clone();
            let stream = res
                .bytes_stream()
                .map(move |chunk| {
                    let _held = &turn;
                    if let (Some(m), Ok(b)) = (&fed, &chunk) {
                        m.lock().feed(b);
                    }
                    chunk
                })
                .chain(futures_util::stream::once(async move {
                    // the whole answer went through
                    if let Some(m) = meter {
                        m.lock().done();
                    }
                    Ok(bytes::Bytes::new())
                }));
            builder.body(Body::from_stream(stream)).unwrap_or_else(|e| {
                api_error(StatusCode::BAD_GATEWAY, "internal", format!("bad answer from VCP: {e}"))
            })
        }
        Err(e) => {
            let msg = if e.is_connect() {
                "Voice Creator Pro is not reachable on the proxy PC (not running, or its HTTP API is off).".to_string()
            } else if e.is_timeout() {
                "Voice Creator Pro did not answer in time.".to_string()
            } else {
                format!("Forwarding to Voice Creator Pro failed: {e}")
            };
            api_error(StatusCode::BAD_GATEWAY, "vcp_unreachable", msg)
        }
    }
}

/// Keeps the start and end of a speech answer and its length, to measure
/// the audio in it without holding it.
struct Meter {
    t0: std::time::Instant,
    head: Vec<u8>,
    tail: Vec<u8>,
    total: usize,
}

impl Meter {
    fn new() -> Self {
        Self { t0: std::time::Instant::now(), head: Vec::with_capacity(1024), tail: Vec::with_capacity(512), total: 0 }
    }

    fn feed(&mut self, b: &[u8]) {
        self.total += b.len();
        if self.head.len() < 1024 {
            let n = (1024 - self.head.len()).min(b.len());
            self.head.extend_from_slice(&b[..n]);
        }
        if b.len() >= 512 {
            self.tail.clear();
            self.tail.extend_from_slice(&b[b.len() - 512..]);
        } else {
            self.tail.extend_from_slice(b);
            let extra = self.tail.len().saturating_sub(512);
            self.tail.drain(..extra);
        }
    }

    fn done(&self) {
        if let Some(audio) = ttv_vcp::perf::audio_secs(&self.head, &self.tail, self.total) {
            ttv_vcp::perf::record(self.t0.elapsed().as_secs_f32(), audio);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_reads_the_audio_length_across_chunks() {
        use base64::Engine as _;
        // 2.5 s of 24 kHz 16-bit mono
        let n: u32 = 60_000;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + n * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 1u16.to_le_bytes().to_vec(), 24_000u32.to_le_bytes().to_vec(), 48_000u32.to_le_bytes().to_vec(), 2u16.to_le_bytes().to_vec(), 16u16.to_le_bytes().to_vec()] {
            wav.extend_from_slice(&v);
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(n * 2).to_le_bytes());
        wav.resize(44 + n as usize * 2, 7);
        let json = format!("{{\"success\":true,\"status\":\"completed\",\"audio_base64\":\"{}\",\"error_code\":null}}", base64::engine::general_purpose::STANDARD.encode(&wav));
        for chunk in [7usize, 300, 4096, 65_536] {
            let mut m = Meter::new();
            for c in json.as_bytes().chunks(chunk) {
                m.feed(c);
            }
            let secs = ttv_vcp::perf::audio_secs(&m.head, &m.tail, m.total).unwrap();
            assert!((secs - 2.5).abs() < 0.01, "chunks of {chunk}: {secs}");
        }
    }

    #[test]
    fn strips_api_key() {
        let u: axum::http::Uri = "/api/v1/streaming/abc/audio?format=wav&api_key=secret".parse().unwrap();
        assert_eq!(target_path(&u), "/api/v1/streaming/abc/audio?format=wav");
        let u: axum::http::Uri = "/x?api_key=secret".parse().unwrap();
        assert_eq!(target_path(&u), "/x");
        let u: axum::http::Uri = "/x?a=1".parse().unwrap();
        assert_eq!(target_path(&u), "/x?a=1");
    }
}
