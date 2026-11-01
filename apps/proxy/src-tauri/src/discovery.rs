//! Answers LAN discovery requests from TTV-TTS (UDP, same port as HTTPS).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use ttv_link::discovery;

use crate::state::AppState;

fn bind(port: u16) -> std::io::Result<UdpSocket> {
    let s = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
    s.set_reuse_address(true)?;
    s.set_broadcast(true)?;
    s.set_nonblocking(true)?;
    s.bind(&SocketAddr::from((crate::server::bind_ip(), port)).into())?;
    UdpSocket::from_std(s.into())
}

pub async fn run(state: Arc<AppState>) {
    loop {
        let (port, enabled) = {
            let c = state.cfg.read();
            (c.port, c.discovery)
        };
        if !enabled {
            state.rebind.notified().await;
            continue;
        }
        let socket = match bind(port) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("LAN discovery unavailable on UDP {port}: {e}");
                tokio::select! {
                    _ = state.rebind.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(30)) => {}
                }
                continue;
            }
        };
        let mut buf = [0u8; 256];
        // At most 20 answers per second, whatever arrives.
        let mut window = Instant::now();
        let mut answered = 0u32;
        loop {
            tokio::select! {
                r = socket.recv_from(&mut buf) => {
                    let Ok((n, from)) = r else { continue };
                    if !discovery::is_request(&buf[..n]) { continue }
                    if window.elapsed() > Duration::from_secs(1) { window = Instant::now(); answered = 0; }
                    answered += 1;
                    if answered > 20 { continue }
                    let reply = discovery::encode_reply(&state.info());
                    let _ = socket.send_to(&reply, from).await;
                }
                _ = state.rebind.notified() => break,
            }
        }
    }
}
