//! HTTP and HTTPS on the same port. The first byte tells them apart: a TLS
//! connection starts with a handshake record (0x16), HTTP with a method
//! name. So one forwarded port serves the local network (plain HTTP, where
//! no public certificate can exist for a LAN address) and the internet
//! (HTTPS with the DuckDNS certificate).
//!
//! Every request carries `ConnectInfo<SocketAddr>` and [`Secure`].

use std::future::Future;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::ConnectInfo;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use hyper_util::service::TowerToHyperService;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tower::ServiceExt;

/// Whether the request came over HTTPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Secure(pub bool);

/// Open connections at most (each costs a task and a few KB).
const MAX_CONNECTIONS: usize = 512;

struct Count(Arc<AtomicUsize>);

impl Drop for Count {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Serves `router` on `listener` until `stop` finishes: plain HTTP, and
/// HTTPS too when `tls` is given.
pub async fn serve(listener: TcpListener, router: Router, tls: Option<Arc<ServerConfig>>, stop: impl Future<Output = ()>) {
    let acceptor = tls.map(TlsAcceptor::from);
    let open = Arc::new(AtomicUsize::new(0));
    let (stopped_tx, stopped) = tokio::sync::watch::channel(false);
    tokio::pin!(stop);
    loop {
        let (stream, addr) = tokio::select! {
            _ = &mut stop => break,
            r = listener.accept() => match r {
                Ok(x) => x,
                Err(e) => {
                    log::debug!("accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
        };
        if open.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
            continue; // dropped
        }
        open.fetch_add(1, Ordering::Relaxed);
        let count = Count(open.clone());
        let _ = stream.set_nodelay(true);
        let (router, acceptor, mut stopped) = (router.clone(), acceptor.clone(), stopped.clone());
        tokio::spawn(async move {
            let _count = count;
            let mut first = [0u8; 1];
            // a connection that sends nothing is closed after a while
            let tls = match tokio::time::timeout(Duration::from_secs(15), stream.peek(&mut first)).await {
                Ok(Ok(1)) => first[0] == 0x16,
                _ => return,
            };
            let svc = router.map_request(move |mut req: hyper::Request<Incoming>| {
                req.extensions_mut().insert(ConnectInfo(addr));
                req.extensions_mut().insert(Secure(tls));
                req
            });
            let svc = TowerToHyperService::new(svc);
            let builder = auto::Builder::new(TokioExecutor::new());
            let run = async {
                match (tls, acceptor) {
                    (true, Some(a)) => match tokio::time::timeout(Duration::from_secs(10), a.accept(stream)).await {
                        Ok(Ok(s)) => builder.serve_connection_with_upgrades(TokioIo::new(s), svc).await,
                        Ok(Err(e)) => {
                            log::debug!("TLS handshake with {addr} failed: {e}");
                            Ok(())
                        }
                        Err(_) => Ok(()),
                    },
                    // TLS without a certificate: nothing to answer with
                    (true, None) => Ok(()),
                    (false, _) => builder.serve_connection_with_upgrades(TokioIo::new(stream), svc).await,
                }
            };
            tokio::select! {
                r = run => {
                    if let Err(e) = r {
                        log::debug!("connection {addr}: {e}");
                    }
                }
                _ = stopped.changed() => {}
            }
        });
    }
    let _ = stopped_tx.send(true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Extension;
    use axum::routing::get;
    use std::net::SocketAddr;

    async fn who(ConnectInfo(addr): ConnectInfo<SocketAddr>, Extension(Secure(s)): Extension<Secure>) -> String {
        format!("{} {s}", addr.ip())
    }

    #[tokio::test]
    async fn plain_and_tls_on_one_port() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap().self_signed(&key).unwrap();
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let mut cfg = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert.der().clone()], rustls::pki_types::PrivateKeyDer::Pkcs8(key.serialize_der().into()))
            .unwrap();
        cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve(listener, Router::new().route("/who", get(who)), Some(Arc::new(cfg)), async {
            let _ = rx.await;
        }));
        let plain = reqwest::get(format!("http://127.0.0.1:{port}/who")).await.unwrap().text().await.unwrap();
        assert_eq!(plain, "127.0.0.1 false");
        let roots = {
            let mut r = rustls::RootCertStore::empty();
            r.add(cert.der().clone()).unwrap();
            r
        };
        let tls = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
        let client = reqwest::Client::builder().tls_backend_preconfigured(tls).resolve("localhost", ([127, 0, 0, 1], port).into()).build().unwrap();
        let secure = client.get(format!("https://localhost:{port}/who")).send().await.unwrap().text().await.unwrap();
        assert_eq!(secure, "127.0.0.1 true");
        let _ = tx.send(());
        tokio::time::timeout(Duration::from_secs(2), server).await.unwrap().unwrap();
    }
}
