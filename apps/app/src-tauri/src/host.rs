//! The voice host: a paired TTV-TTS Proxy (encrypted, certificate pinned) or
//! Voice Creator Pro on this PC. Both speak VCP's API, so the rest of the app
//! only sees a `ttv_vcp::Client`.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::ClientConfig;
use serde::Serialize;
use ttv_link::pin::{LearningVerifier, PinnedVerifier};
use ttv_link::{PairRequest, PairResponse, ProxyInfo, discovery};

use crate::config::HostConfig;

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn tls_with(verifier: Arc<dyn rustls::client::danger::ServerCertVerifier>) -> ClientConfig {
    let mut cfg = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    cfg
}

/// HTTP client that only accepts the proxy's pinned certificate.
pub fn pinned_http(fingerprint: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls_with(PinnedVerifier::new(fingerprint, &provider())))
        .connect_timeout(Duration::from_secs(4))
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_nodelay(true)
        .no_proxy()
        .build()
        .expect("pinned client")
}

/// TLS config for WebSockets to the proxy (events).
pub fn pinned_tls(fingerprint: &str) -> Arc<ClientConfig> {
    let mut cfg = tls_with(PinnedVerifier::new(fingerprint, &provider()));
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(cfg)
}

/// A VCP client for the configured host, or None if none is set up.
pub fn client(host: &HostConfig, secret: impl Fn(&str) -> Option<String>) -> Option<ttv_vcp::Client> {
    match host {
        HostConfig::None => None,
        HostConfig::Proxy { id, address, fingerprint, .. } => {
            let token = secret(&crate::secrets::proxy_token(id));
            Some(ttv_vcp::Client::with_http(pinned_http(fingerprint), &format!("https://{address}"), token))
        }
        HostConfig::Direct { url } => Some(ttv_vcp::Client::new(url, secret(crate::secrets::VCP_KEY))),
    }
}

// ── discovery ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Found {
    /// "192.168.1.25:8110"
    pub address: String,
    pub info: ProxyInfo,
}

/// Broadcast addresses of all IPv4 interfaces, plus the limited broadcast.
fn broadcast_targets(port: u16) -> Vec<SocketAddr> {
    let mut out = vec![SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, port))];
    if let Ok(ifs) = if_addrs::get_if_addrs() {
        for i in ifs {
            if let if_addrs::IfAddr::V4(v4) = i.addr
                && !v4.ip.is_loopback()
                && let Some(b) = v4.broadcast
            {
                out.push(SocketAddr::V4(SocketAddrV4::new(b, port)));
            }
        }
    }
    out.dedup();
    out
}

/// Asks the network for proxies (UDP broadcast) and collects answers for
/// `wait`. Also asks `extra` addresses directly (e.g. a remembered one).
pub async fn discover(wait: Duration, extra: &[SocketAddr]) -> Vec<Found> {
    let port = ttv_link::DEFAULT_PORT;
    let socket = match (|| -> std::io::Result<tokio::net::UdpSocket> {
        let s = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
        s.set_broadcast(true)?;
        s.set_nonblocking(true)?;
        s.bind(&SocketAddr::from(([0, 0, 0, 0], 0)).into())?;
        tokio::net::UdpSocket::from_std(s.into())
    })() {
        Ok(s) => s,
        Err(e) => {
            log::warn!("discovery socket: {e}");
            return Vec::new();
        }
    };
    let targets: Vec<SocketAddr> = broadcast_targets(port).into_iter().chain(extra.iter().copied()).collect();
    // Two rounds: a single UDP packet may get lost on Wi-Fi.
    for _ in 0..2 {
        for t in &targets {
            let _ = socket.send_to(discovery::REQUEST, t).await;
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
    let mut found: Vec<Found> = Vec::new();
    let deadline = Instant::now() + wait;
    let mut buf = [0u8; discovery::MAX_PACKET];
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match tokio::time::timeout(left, socket.recv_from(&mut buf)).await {
            Ok(Ok((n, from))) => {
                if let Some(info) = discovery::decode_reply(&buf[..n]) {
                    let address = format!("{}:{}", from.ip(), info.port);
                    if !found.iter().any(|f| f.info.id == info.id) {
                        found.push(Found { address, info });
                    }
                }
            }
            _ => break,
        }
    }
    found
}

// ── pairing ─────────────────────────────────────────────────────────────

/// What the app learns about a proxy before pairing: its info and the
/// certificate it actually presents (shown to the user for comparison).
#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    pub address: String,
    pub info: ProxyInfo,
    /// Fingerprint of the certificate seen in the TLS handshake.
    pub fingerprint: String,
    pub fingerprint_short: String,
}

/// Contacts a proxy at `address` (host:port) without pinning yet.
pub async fn probe(address: &str) -> Result<Probe, String> {
    let address = normalize_address(address)?;
    let verifier = LearningVerifier::new(&provider());
    let http = reqwest::Client::builder()
        .tls_backend_preconfigured(tls_with(verifier.clone()))
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(8))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let info: ProxyInfo = http
        .get(format!("https://{address}/ttv/v1/info"))
        .send()
        .await
        .map_err(|e| unreachable_text(&address, &e))?
        .json()
        .await
        .map_err(|_| format!("{address} answered, but it is not a TTV-TTS Proxy."))?;
    let fingerprint = verifier.seen().ok_or("no certificate seen")?;
    if !info.fingerprint.is_empty() && info.fingerprint != fingerprint {
        return Err("The certificate doesn't match what the proxy announces - something in the network is intercepting the connection.".into());
    }
    Ok(Probe { address, fingerprint_short: ttv_link::pin::short(&fingerprint), fingerprint, info })
}

/// Pairs with the 6-digit code; returns the device token.
pub async fn pair(probe: &Probe, code: &str, device_id: &str, device_name: &str) -> Result<PairResponse, String> {
    let http = pinned_http(&probe.fingerprint);
    let res = http
        .post(format!("https://{}/ttv/v1/pair", probe.address))
        .json(&PairRequest { code: code.trim().to_string(), device_name: device_name.to_string(), device_id: device_id.to_string() })
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| unreachable_text(&probe.address, &e))?;
    if res.status().is_success() {
        return res.json().await.map_err(|e| e.to_string());
    }
    let err: ttv_link::ApiError = res.json().await.unwrap_or_default();
    Err(match err.code.as_str() {
        "bad_code" => "Wrong code - check the code shown in TTV-TTS Proxy.".into(),
        "pairing_closed" => "No code is open on the proxy. Click “Pair a device” in TTV-TTS Proxy first.".into(),
        "rate_limited" => "Too many attempts - wait a minute.".into(),
        _ => err.error,
    })
}

fn normalize_address(input: &str) -> Result<String, String> {
    let s = input.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
    let s = s.split('/').next().unwrap_or("");
    if s.is_empty() {
        return Err("Please enter the proxy's address, e.g. 192.168.1.25".into());
    }
    Ok(if s.contains(':') { s.to_string() } else { format!("{s}:{}", ttv_link::DEFAULT_PORT) })
}

fn unreachable_text(address: &str, e: &reqwest::Error) -> String {
    if e.is_timeout() || e.is_connect() {
        format!("No answer from {address}. Is TTV-TTS Proxy running there, and does the firewall on that PC let it through?")
    } else {
        format!("Connecting to {address} failed: {e}")
    }
}

// ── Wake-on-LAN ─────────────────────────────────────────────────────────

/// Sends a magic packet for `mac` ("aa:bb:cc:dd:ee:ff") to all broadcast addresses.
pub async fn wake(mac: &str) -> Result<(), String> {
    let bytes: Vec<u8> = mac.split([':', '-']).filter_map(|h| u8::from_str_radix(h, 16).ok()).collect();
    if bytes.len() != 6 {
        return Err("invalid MAC address".into());
    }
    let mut packet = vec![0xffu8; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&bytes);
    }
    let s = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP)).map_err(|e| e.to_string())?;
    s.set_broadcast(true).map_err(|e| e.to_string())?;
    let socket: std::net::UdpSocket = s.into();
    for target in broadcast_targets(9) {
        let _ = socket.send_to(&packet, target);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        assert_eq!(normalize_address("192.168.1.25").unwrap(), "192.168.1.25:8110");
        assert_eq!(normalize_address("https://192.168.1.25:9000/ttv/v1/info").unwrap(), "192.168.1.25:9000");
        assert!(normalize_address("  ").is_err());
    }
}
