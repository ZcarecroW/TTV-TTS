//! Opens ports in the home router by itself (UPnP IGD), so friends and OBS
//! links can reach this PC from the internet without router settings.
//!
//! - Only the ports of features switched on for the internet are mapped,
//!   to this PC's address in the network, with a lease of an hour that is
//!   renewed every 25 minutes (routers that only know permanent mappings
//!   get one); they're removed again when not needed or on exit.
//! - The router is searched from every network adapter in turn: with a VPN,
//!   the default route (and the search) would otherwise go into the tunnel.
//! - The router's internet address tells whether port forwarding can work at
//!   all: a private or carrier-NAT address (DS-Lite, CGNAT) can't be reached.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use igd_next::aio::tokio::{Tokio, search_gateway};
use igd_next::{AddPortError, PortMappingProtocol, SearchOptions};
use parking_lot::RwLock;
use serde::Serialize;
use tokio::sync::Notify;

type Gateway = igd_next::aio::Gateway<Tokio>;

/// A lease on the router; renewed well before it runs out.
const LEASE: u32 = 3600;
const RENEW: Duration = Duration::from_secs(25 * 60);

/// What the window shows.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpnpStatus {
    /// The router's address in the network.
    pub router: Option<String>,
    /// The router's address on the internet.
    pub external_ip: Option<String>,
    /// That address can't be reached from the internet (DS-Lite, carrier NAT).
    pub no_public_ip: bool,
    /// Ports open in the router right now.
    pub mapped: Vec<u16>,
    /// The ports that should be open.
    pub wanted: Vec<u16>,
    pub error: Option<String>,
    /// Unix seconds of the last attempt.
    pub at: Option<i64>,
}

pub struct Upnp {
    status: RwLock<UpnpStatus>,
    wake: Notify,
    /// The router and what we mapped there (to remove it again).
    held: tokio::sync::Mutex<Option<(Gateway, Vec<u16>)>>,
    name: String,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// An address the internet can reach (not private, not carrier NAT).
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            !(v.is_private() || v.is_loopback() || v.is_link_local() || v.is_unspecified() || v.is_broadcast() || (o[0] == 100 && (64..128).contains(&o[1])) || o[0] == 0)
        }
        IpAddr::V6(v) => (v.segments()[0] & 0xe000) == 0x2000,
    }
}

/// This PC's IPv4 addresses in local networks (each one is tried for the search).
fn local_addresses() -> Vec<Ipv4Addr> {
    let mut v: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            IpAddr::V4(a) if a.is_private() => Some(a),
            _ => None,
        })
        .collect();
    // home networks first (192.168.x), VPN adapters often use 10.x
    v.sort_by_key(|a| (a.octets()[0] != 192, a.octets()[0] != 172));
    v.dedup();
    v
}

fn add_error(e: &AddPortError) -> String {
    match e {
        AddPortError::ActionNotAuthorized => {
            "The router doesn't let this PC open ports. FRITZ!Box: Internet → Permit Access → Port Sharing → this PC → “Permit independent port sharing for this device”. Other routers: switch on UPnP.".into()
        }
        AddPortError::PortInUse => "The router already forwards this port to another device.".into(),
        e => format!("The router refused: {e}"),
    }
}

impl Upnp {
    /// `name` goes into the router's list ("TTV-TTS", "TTV-TTS Proxy").
    pub fn new(name: &str) -> Arc<Self> {
        Arc::new(Self { status: RwLock::default(), wake: Notify::new(), held: tokio::sync::Mutex::new(None), name: name.to_string() })
    }

    pub fn status(&self) -> UpnpStatus {
        self.status.read().clone()
    }

    /// The router's public address, when it has one (for DuckDNS).
    pub fn public_ip(&self) -> Option<String> {
        let s = self.status.read();
        s.external_ip.clone().filter(|_| !s.no_public_ip)
    }

    /// Look again now (a port or a switch changed).
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// Keeps the ports `want` returns open in the router (none: closes ours).
    /// With `want_ip()`, the router's internet address is looked up even
    /// without ports (DuckDNS needs it behind a VPN). Runs forever;
    /// `on_change` is called when the status changes.
    pub async fn run(
        self: Arc<Self>,
        want: impl Fn() -> Vec<(u16, &'static str)> + Send + Sync + 'static,
        want_ip: impl Fn() -> bool + Send + Sync + 'static,
        on_change: impl Fn() + Send + Sync + 'static,
    ) {
        loop {
            let ports = want();
            let wait = if ports.is_empty() {
                self.close().await;
                let ip = want_ip();
                if ip && let Ok((g, _)) = self.find().await {
                    self.learn(&g).await;
                }
                on_change();
                ip.then_some(RENEW)
            } else {
                let r = self.apply(&ports).await;
                {
                    let mut s = self.status.write();
                    s.at = Some(now());
                    s.wanted = ports.iter().map(|p| p.0).collect();
                    s.error = r.err();
                }
                on_change();
                Some(if self.status.read().error.is_some() { Duration::from_secs(300) } else { RENEW })
            };
            match wait {
                None => self.wake.notified().await,
                Some(d) => {
                    tokio::select! {
                        _ = self.wake.notified() => {}
                        _ = tokio::time::sleep(d) => {}
                    }
                }
            }
        }
    }

    async fn find(&self) -> Result<(Gateway, Ipv4Addr), String> {
        let mut last = "No router answered (is UPnP switched on in the router?).".to_string();
        for local in local_addresses() {
            let mut opts = SearchOptions::default();
            opts.bind_addr = SocketAddr::new(IpAddr::V4(local), 0);
            opts.timeout = Some(Duration::from_secs(3));
            opts.single_search_timeout = Some(Duration::from_secs(2));
            match search_gateway(opts).await {
                Ok(g) => {
                    // the router must be in the same network as this address
                    if let IpAddr::V4(r) = g.addr.ip()
                        && r.octets()[..3] != local.octets()[..3]
                        && !r.is_private()
                    {
                        continue;
                    }
                    return Ok((g, local));
                }
                Err(e) => last = format!("No router answered on {local}: {e}"),
            }
        }
        Err(last)
    }

    async fn apply(&self, ports: &[(u16, &'static str)]) -> Result<(), String> {
        let (g, local) = self.find().await?;
        self.learn(&g).await;
        let mut held = self.held.lock().await;
        // ports we opened before that aren't wanted any more
        if let Some((old, mapped)) = held.as_ref() {
            for p in mapped.iter().filter(|p| !ports.iter().any(|w| w.0 == **p)) {
                let _ = old.remove_port(PortMappingProtocol::TCP, *p).await;
            }
        }
        let mut mapped = Vec::new();
        let mut errors = Vec::new();
        for (port, what) in ports {
            // routers show spaces badly (FRITZ!Box: dots)
            let desc = format!("{}-{what}", self.name).replace(' ', "-");
            let to = SocketAddr::new(IpAddr::V4(local), *port);
            let r = match g.add_port(PortMappingProtocol::TCP, *port, to, LEASE, &desc).await {
                Err(AddPortError::OnlyPermanentLeasesSupported) => g.add_port(PortMappingProtocol::TCP, *port, to, 0, &desc).await,
                r => r,
            };
            match r {
                Ok(()) => mapped.push(*port),
                Err(e) => errors.push(format!("port {port}: {}", add_error(&e))),
            }
        }
        if !mapped.is_empty() {
            log::info!("UPnP: ports {mapped:?} open on the router {} to {local}", g.addr.ip());
        }
        self.status.write().mapped = mapped.clone();
        *held = Some((g, mapped));
        if errors.is_empty() { Ok(()) } else { Err(errors.join(" ")) }
    }

    /// The router's addresses.
    async fn learn(&self, g: &Gateway) {
        let ext = g.get_external_ip().await.ok();
        let mut s = self.status.write();
        s.router = Some(g.addr.ip().to_string());
        s.external_ip = ext.map(|i| i.to_string());
        s.no_public_ip = ext.is_some_and(|i| !is_public(i));
    }

    /// Removes the ports we opened (when not needed any more, and on exit).
    pub async fn close(&self) {
        let mut held = self.held.lock().await;
        if let Some((g, mapped)) = held.take() {
            for p in &mapped {
                match tokio::time::timeout(Duration::from_secs(2), g.remove_port(PortMappingProtocol::TCP, *p)).await {
                    Ok(Ok(())) => log::info!("UPnP: port {p} closed on the router"),
                    Ok(Err(e)) => log::info!("UPnP: port {p} not closed: {e}"),
                    Err(_) => log::info!("UPnP: port {p} not closed (no answer)"),
                }
            }
        }
        let mut s = self.status.write();
        s.mapped.clear();
        s.wanted.clear();
        s.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_addresses() {
        for a in ["84.150.1.2", "8.8.8.8", "2a02:810d::1"] {
            assert!(is_public(a.parse().unwrap()), "{a}");
        }
        for a in ["192.168.178.1", "10.0.0.1", "172.20.1.1", "100.64.0.1", "100.127.255.1", "127.0.0.1", "0.0.0.0", "fd00::1", "fe80::1"] {
            assert!(!is_public(a.parse().unwrap()), "{a}");
        }
        assert!(is_public("100.128.0.1".parse().unwrap()));
    }

    /// Live, on the real router: opens a test port to this PC, checks it in
    /// the router's list and closes it again.
    /// `cargo test -p ttv-net upnp_live -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn upnp_live() {
        let u = Upnp::new("TTV-TTS test");
        let port = 47_000 + (rand::random::<u16>() % 1000);
        let t0 = std::time::Instant::now();
        let r = u.apply(&[(port, "test")]).await;
        let s = u.status();
        println!("{} ms: {r:?} {s:?}", t0.elapsed().as_millis());
        r.unwrap();
        assert_eq!(s.mapped, vec![port]);
        // the router lists it
        let held = u.held.lock().await;
        let g = &held.as_ref().unwrap().0;
        let mut found = false;
        for i in 0..64 {
            match g.get_generic_port_mapping_entry(i).await {
                Ok(e) if e.external_port == port => {
                    println!("router entry: {} -> {}:{} ({})", e.external_port, e.internal_client, e.internal_port, e.port_mapping_description);
                    found = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        drop(held);
        assert!(found, "the router lists the mapping");
        u.close().await;
        assert!(u.status().mapped.is_empty());
    }
}
