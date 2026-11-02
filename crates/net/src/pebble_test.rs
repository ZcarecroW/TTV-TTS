//! The whole certificate flow against Pebble, Let's Encrypt's own ACME test
//! server: account, order, DNS-01 through a fake DuckDNS (HTTP) and a fake
//! name server (UDP) that Pebble asks, issuing, serving HTTPS with the new
//! certificate, renewing with "replaces", and a wrong token.
//!
//! Run with the folder holding `pebble.exe`, `pebble.minica.pem`,
//! `cert.pem` and `key.pem` (from the Pebble release and repository):
//! `PEBBLE_DIR=… cargo test -p ttv-net pebble -- --ignored --nocapture`

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get;
use parking_lot::Mutex;

use super::*;

type Txt = Arc<Mutex<HashMap<String, String>>>;

/// DuckDNS's update API: the TXT record applies to the name and everything below it.
async fn duck(State(txt): State<Txt>, Query(q): Query<HashMap<String, String>>) -> String {
    if q.get("token").map(String::as_str) != Some("test-token") {
        return "KO".into();
    }
    let name = format!("{}.duckdns.org", q.get("domains").cloned().unwrap_or_default());
    if q.contains_key("clear") {
        txt.lock().remove(&name);
        return "OK\n\nUPDATED".into();
    }
    match q.get("txt") {
        Some(v) => {
            txt.lock().insert(name, v.clone());
            format!("OK\n{v}\nUPDATED")
        }
        None => "OK\n203.0.113.7\n\nUPDATED".into(),
    }
}

/// Answers TXT questions from the map (a name or anything below it).
async fn name_server(sock: tokio::net::UdpSocket, txt: Txt) {
    let mut buf = [0u8; 1500];
    loop {
        let Ok((n, from)) = sock.recv_from(&mut buf).await else { return };
        let q = &buf[..n];
        if n < 17 {
            continue;
        }
        // the question name
        let (mut i, mut labels) = (12, Vec::new());
        while i < n && q[i] != 0 {
            let l = q[i] as usize;
            labels.push(String::from_utf8_lossy(&q[i + 1..i + 1 + l]).to_ascii_lowercase());
            i += 1 + l;
        }
        let qtype = u16::from_be_bytes([q[i + 1], q[i + 2]]);
        let end = i + 5;
        let name = labels.join(".");
        let value = txt.lock().iter().find(|(k, _)| name == **k || name.ends_with(&format!(".{k}"))).map(|(_, v)| v.clone());
        let mut r = q[..end].to_vec();
        r[2] = 0x84;
        r[3] = 0;
        r[6..12].copy_from_slice(&[0, 0, 0, 0, 0, 0]);
        if let (16, Some(v)) = (qtype, value) {
            r[7] = 1;
            r.extend([0xc0, 12, 0, 16, 0, 1, 0, 0, 0, 0]);
            r.extend(((v.len() + 1) as u16).to_be_bytes());
            r.push(v.len() as u8);
            r.extend(v.as_bytes());
        }
        let _ = sock.send_to(&r, from).await;
    }
}

#[tokio::test]
#[ignore]
async fn pebble() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let dir = std::path::PathBuf::from(std::env::var("PEBBLE_DIR").expect("PEBBLE_DIR"));
    let txt: Txt = Arc::default();

    let dns = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dns_addr = dns.local_addr().unwrap();
    tokio::spawn(name_server(dns, txt.clone()));
    let web = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let web_addr = web.local_addr().unwrap();
    let app = Router::new().route("/update", get(duck)).with_state(txt.clone());
    tokio::spawn(async move { axum::serve(web, app).await.unwrap() });

    let cfg = serde_json::json!({ "pebble": {
        "listenAddress": "127.0.0.1:14000", "managementListenAddress": "127.0.0.1:15000",
        "certificate": dir.join("cert.pem"), "privateKey": dir.join("key.pem"),
        "httpPort": 5002, "tlsPort": 5001, "ocspResponderURL": "", "externalAccountBindingRequired": false,
        "retryAfter": { "authz": 1, "order": 1 }, "keyAlgorithm": "ecdsa",
        "profiles": { "default": { "description": "default", "validityPeriod": 7_776_000 } }
    }});
    let cfg_path = dir.join("ttv-pebble.json");
    std::fs::write(&cfg_path, cfg.to_string()).unwrap();
    let mut pebble = tokio::process::Command::new(dir.join("pebble.exe"))
        .args(["-config", cfg_path.to_str().unwrap(), "-dnsserver", &dns_addr.to_string()])
        .env("PEBBLE_VA_NOSLEEP", "1")
        .env("PEBBLE_WFE_NONCEREJECT", "0")
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("pebble");
    for _ in 0..50 {
        if tokio::net::TcpStream::connect("127.0.0.1:14000").await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // SAFETY: set before anything reads them; no other test uses these
    unsafe {
        std::env::set_var("TTV_DUCKDNS_URL", format!("http://{web_addr}"));
        std::env::set_var("TTV_DNS_SERVER", dns_addr.to_string());
        std::env::set_var("TTV_ACME_URL", "https://localhost:14000/dir");
        std::env::set_var("TTV_ACME_ROOT", dir.join("pebble.minica.pem"));
    }

    let data = std::env::temp_dir().join(format!("ttv-net-pebble-{}", rand::random::<u32>()));
    let certs = Certs::new(&data, || {});
    let fqdn = "mystream.duckdns.org";

    // a wrong token fails with DuckDNS's answer and waits before trying again
    let bad = Want { domain: "mystream".into(), token: "wrong".into() };
    let e = certs.obtain(&bad).await.unwrap_err();
    assert!(e.contains("KO"), "{e}");

    let t0 = std::time::Instant::now();
    let want = Want { domain: "mystream".into(), token: "test-token".into() };
    let w = want.clone();
    let runner = tokio::spawn(certs.clone().run(move || Some(w.clone())));
    while !certs.valid_for(fqdn) {
        assert!(t0.elapsed() < Duration::from_secs(60), "no certificate: {:?}", certs.status());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!("certificate in {} ms: {:?}", t0.elapsed().as_millis(), certs.status());
    assert!(txt.lock().is_empty(), "the TXT record was removed again");
    assert!(data.join("cert.pem").exists() && data.join("key.pem").exists() && data.join("account.json").exists());
    let s = certs.status();
    assert_eq!(s.domain.as_deref(), Some(fqdn));
    assert!(s.error.is_none() && s.step.is_none());

    // HTTPS with it, trusted through Pebble's (random) root
    let root = reqwest::Client::builder().danger_accept_invalid_certs(true).build().unwrap().get("https://localhost:15000/roots/0").send().await.unwrap().text().await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(crate::serve::serve(listener, Router::new().route("/x", get(|| async { "hello" })), Some(certs.server_config()), async {
        let _ = stop_rx.await;
    }));
    let client = |root: &str| {
        let mut roots = rustls::RootCertStore::empty();
        for c in rustls::pki_types::CertificateDer::pem_slice_iter(root.as_bytes()) {
            roots.add(c.unwrap()).unwrap();
        }
        let tls = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
        reqwest::Client::builder().tls_backend_preconfigured(tls).resolve(fqdn, SocketAddr::from(([127, 0, 0, 1], port))).build().unwrap()
    };
    let body = client(&root).get(format!("https://{fqdn}:{port}/x")).send().await.unwrap().text().await.unwrap();
    assert_eq!(body, "hello");
    let first_serial = certs.current.read().as_ref().unwrap().facts.serial.clone();

    // renewing replaces the certificate while the server keeps running
    certs.obtain(&want).await.unwrap();
    let second_serial = certs.current.read().as_ref().unwrap().facts.serial.clone();
    assert_ne!(first_serial, second_serial);
    let body = client(&root).get(format!("https://{fqdn}:{port}/x")).send().await.unwrap().text().await.unwrap();
    assert_eq!(body, "hello");

    // a restart loads the kept certificate and account
    let again = Certs::new(&data, || {});
    assert!(again.valid_for(fqdn));
    assert!(again.due(fqdn).await.is_some(), "a fresh certificate isn't due: {:?}", again.status());
    println!("renewal at {:?} (valid {:?}..{:?})", again.status().renew_at, again.status().valid_from, again.status().valid_until);

    let _ = stop_tx.send(());
    let _ = server.await;
    runner.abort();
    let _ = pebble.kill().await;
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_file(&cfg_path);
}
