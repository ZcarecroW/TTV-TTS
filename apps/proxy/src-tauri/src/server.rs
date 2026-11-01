//! The network side: an HTTPS listener (and optionally a plain-HTTP one for
//! older tools) that serves the proxy's own API under `/ttv/v1` and forwards
//! everything else to Voice Creator Pro.
//!
//! One task per connection: a slow TLS handshake never blocks others, and
//! streams (speech audio) run as long as they need.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::Router;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt;

use crate::state::AppState;

/// Paths that work without a key: the proxy's public info and pairing, and
/// the two endpoints VCP itself leaves open.
fn is_public(path: &str) -> bool {
    matches!(path, "/ttv/v1/info" | "/ttv/v1/pair" | "/api/v1/health" | "/api/v1/system/version")
}

pub fn api_error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (status, axum::Json(ttv_link::ApiError { error: message.into(), code: code.into() })).into_response()
}

/// The key a request presents, VCP style: `X-API-Key`, `Authorization: Bearer`
/// or `?api_key=` (the last one works for `<audio src>` URLs).
pub fn presented_key(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(v) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        return Some(v.trim().to_string());
    }
    if let Some(v) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok())
        && let Some(t) = v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer "))
    {
        return Some(t.trim().to_string());
    }
    let q = query?;
    url::form_urlencoded::parse(q.as_bytes()).find(|(k, _)| k == "api_key").map(|(_, v)| v.trim().to_string())
}

async fn auth(State(state): State<Arc<AppState>>, ConnectInfo(addr): ConnectInfo<SocketAddr>, req: Request, next: Next) -> Response {
    if is_public(req.uri().path()) {
        return next.run(req).await;
    }
    let ip: IpAddr = addr.ip();
    if state.access.locked_out(ip) {
        return api_error(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "Too many wrong keys from this address - try again in a minute.");
    }
    let key = presented_key(req.headers(), req.uri().query());
    let ok = key.as_deref().and_then(|k| state.access.check(k, state.access_key().as_deref(), ip)).is_some();
    if ok {
        return next.run(req).await;
    }
    if key.is_some() {
        state.access.failed(ip);
        log::warn!("{ip} used a wrong key for {}", req.uri().path());
    }
    api_error(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "Missing or invalid API key. Send X-API-Key, Authorization: Bearer, or ?api_key= (the proxy's key or a paired device token).",
    )
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/ttv/v1", crate::api::routes())
        .fallback(crate::forward::handler)
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

/// Counts open connections.
struct ConnGuard(Arc<AppState>);
impl ConnGuard {
    fn new(state: &Arc<AppState>) -> Self {
        state.connections.fetch_add(1, Ordering::Relaxed);
        Self(state.clone())
    }
}
impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::Relaxed);
    }
}

async fn accept_loop(listener: TcpListener, tls: Option<TlsAcceptor>, router: Router, state: Arc<AppState>) {
    loop {
        let (stream, addr) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                // e.g. too many open files: back off briefly instead of spinning.
                log::debug!("accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        let router = router.clone();
        let tls = tls.clone();
        let state = state.clone();
        tokio::spawn(async move {
            let _guard = ConnGuard::new(&state);
            let svc = router.map_request(move |mut req: hyper::Request<Incoming>| {
                req.extensions_mut().insert(ConnectInfo(addr));
                req
            });
            let svc = TowerToHyperService::new(svc);
            let builder = auto::Builder::new(TokioExecutor::new());
            let result = match tls {
                Some(acceptor) => match tokio::time::timeout(Duration::from_secs(10), acceptor.accept(stream)).await {
                    Ok(Ok(tls_stream)) => builder.serve_connection_with_upgrades(TokioIo::new(tls_stream), svc).await,
                    Ok(Err(e)) => {
                        log::debug!("TLS handshake with {addr} failed: {e}");
                        return;
                    }
                    Err(_) => return,
                },
                None => builder.serve_connection_with_upgrades(TokioIo::new(stream), svc).await,
            };
            if let Err(e) = result {
                log::debug!("connection {addr}: {e}");
            }
        });
    }
}

/// All IPv4 interfaces. A debug build may use `TTV_PROXY_BIND` (e.g. 127.0.0.1)
/// so local tests don't trigger the firewall prompt.
pub fn bind_ip() -> [u8; 4] {
    if cfg!(debug_assertions)
        && let Ok(ip) = std::env::var("TTV_PROXY_BIND").unwrap_or_default().parse::<std::net::Ipv4Addr>()
    {
        return ip.octets();
    }
    [0, 0, 0, 0]
}

async fn bind(port: u16) -> std::io::Result<TcpListener> {
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::STREAM, Some(socket2::Protocol::TCP))?;
    #[cfg(not(windows))]
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from((bind_ip(), port)).into())?;
    socket.listen(256)?;
    TcpListener::from_std(socket.into())
}

/// Runs the listeners; restarts them when the port or certificate changes.
pub async fn run(state: Arc<AppState>) {
    loop {
        let (port, http_port) = {
            let c = state.cfg.read();
            (c.port, c.http_port)
        };
        let router = router(state.clone());
        let https = match bind(port).await {
            Ok(l) => {
                *state.listen_error.write() = None;
                log::info!("listening on port {port} (HTTPS)");
                l
            }
            Err(e) => {
                let msg = format!("port {port} cannot be used: {e}");
                log::error!("{msg}");
                *state.listen_error.write() = Some(msg);
                state.publish();
                tokio::select! {
                    _ = state.rebind.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(15)) => {}
                }
                continue;
            }
        };
        let http = if http_port != 0 && http_port != port {
            match bind(http_port).await {
                Ok(l) => {
                    log::info!("also listening on port {http_port} (plain HTTP)");
                    Some(l)
                }
                Err(e) => {
                    log::warn!("plain-HTTP port {http_port} cannot be used: {e}");
                    None
                }
            }
        } else {
            None
        };
        let acceptor = TlsAcceptor::from(state.tls.read().config.clone());
        let plain = async {
            match http {
                Some(l) => accept_loop(l, None, router.clone(), state.clone()).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = accept_loop(https, Some(acceptor), router.clone(), state.clone()) => {}
            _ = plain => {}
            _ = state.rebind.notified() => log::info!("restarting the listeners"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn keys_from_all_places() {
        let mut h = HeaderMap::new();
        assert_eq!(presented_key(&h, Some("a=1&api_key=q%2Bk")), Some("q+k".into()));
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer tok"));
        assert_eq!(presented_key(&h, None), Some("tok".into()));
        h.insert("x-api-key", HeaderValue::from_static("hdr"));
        assert_eq!(presented_key(&h, Some("api_key=q")), Some("hdr".into()));
        assert_eq!(presented_key(&HeaderMap::new(), None), None);
    }

    #[test]
    fn public_paths() {
        assert!(is_public("/ttv/v1/info"));
        assert!(is_public("/api/v1/health"));
        assert!(!is_public("/api/v1/voices"));
        assert!(!is_public("/ttv/v1/status"));
    }
}
