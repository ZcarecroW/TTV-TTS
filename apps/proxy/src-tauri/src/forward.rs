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
    let url = format!("{}{}", vcp.base(), target_path(&parts.uri));
    // One job at a time at VCP, shared with the computing jobs; held until
    // the answer has been passed on.
    let turn = if ttv_vcp::is_work(&parts.method, parts.uri.path()) {
        let t0 = std::time::Instant::now();
        let t = ttv_vcp::turn().await;
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

    match rb.send().await {
        Ok(res) => {
            let mut builder = Response::builder().status(res.status());
            if let Some(h) = builder.headers_mut() {
                for (name, value) in res.headers() {
                    if !skip_response_header(name) {
                        h.append(name.clone(), value.clone());
                    }
                }
            }
            let stream = res.bytes_stream().map(move |chunk| {
                let _held = &turn;
                chunk
            });
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

#[cfg(test)]
mod tests {
    use super::*;

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
