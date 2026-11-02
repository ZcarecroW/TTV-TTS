//! Free HTTPS certificates from Let's Encrypt for a DuckDNS name, fully
//! automatic: nobody has to know what a certificate is.
//!
//! How it works (ACME, RFC 8555, with the DNS-01 check):
//! 1. An ACME account (a key pair kept in the data folder, no e-mail).
//! 2. An order for `name.duckdns.org`; Let's Encrypt answers with a value
//!    that must appear in the name's TXT record.
//! 3. DuckDNS sets the TXT record; we wait until DuckDNS's own name servers
//!    show it, then tell Let's Encrypt to look.
//! 4. Let's Encrypt checks the record from several places on the internet,
//!    and issues the certificate for a new key made here (the key never
//!    leaves this PC). The TXT record is removed again.
//!
//! No port has to be open for this (unlike the HTTP check), and the public
//! address may change any time: the certificate is for the name, not the
//! address. Certificates last 45–90 days (Let's Encrypt is shortening them);
//! we renew when two thirds of the time have passed, or earlier when Let's
//! Encrypt's renewal information (ARI, RFC 9773) suggests it (e.g. before
//! it revokes certificates after an incident). The new certificate is
//! swapped in while the server runs: connections never drop for it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use instant_acme::{Account, AccountCredentials, AuthorizationStatus, CertificateIdentifier, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder, Order, OrderStatus, RetryPolicy};
use parking_lot::{Mutex, RwLock};
use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, Der, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::der::{self, Facts};
use crate::{dns, duckdns};

/// The certificate's state, as the window shows it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CertStatus {
    /// The name the current certificate is for.
    pub domain: Option<String>,
    /// Unix seconds.
    pub valid_from: Option<i64>,
    pub valid_until: Option<i64>,
    /// When it gets renewed.
    pub renew_at: Option<i64>,
    /// Working on a new certificate: "account", "order", "dns", "validating", "issuing".
    pub step: Option<&'static str>,
    /// The last attempt failed (until one works).
    pub error: Option<String>,
    pub error_at: Option<i64>,
    /// The next attempt after a failure.
    pub next_try: Option<i64>,
    /// Certificates from Let's Encrypt's test system (not trusted by browsers).
    pub staging: bool,
}

/// What to get a certificate for: a DuckDNS name and its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Want {
    pub domain: String,
    pub token: String,
}

struct Current {
    key: Arc<CertifiedKey>,
    facts: Facts,
}

#[derive(Serialize, Deserialize)]
struct StoredAccount {
    directory: String,
    credentials: AccountCredentials,
}

/// The certificate of one server (app or proxy) and the loop that keeps it fresh.
pub struct Certs {
    dir: PathBuf,
    directory: String,
    current: RwLock<Option<Current>>,
    status: RwLock<CertStatus>,
    account: Mutex<Option<Account>>,
    /// Renewal information: when it was last asked, and the time it suggested.
    ari: Mutex<Option<(Instant, Option<i64>)>>,
    wake: Notify,
    on_change: Box<dyn Fn() + Send + Sync>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn acme_err(e: instant_acme::Error) -> String {
    match e {
        instant_acme::Error::Api(p) => format!("Let's Encrypt: {}", p.detail.unwrap_or_else(|| p.r#type.unwrap_or_default())),
        instant_acme::Error::Timeout(_) => "Let's Encrypt took too long to answer.".into(),
        e => format!("Let's Encrypt: {e}"),
    }
}

impl Certs {
    /// Loads the certificate kept in `dir` (if any). `on_change` is called
    /// whenever the status changes. `TTV_ACME_STAGING=1` uses Let's Encrypt's
    /// test system; debug builds may point `TTV_ACME_URL` at a test server.
    pub fn new(dir: &Path, on_change: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        let staging = std::env::var("TTV_ACME_STAGING").is_ok_and(|v| v == "1");
        let custom = if cfg!(debug_assertions) { std::env::var("TTV_ACME_URL").ok() } else { None };
        let directory = custom.unwrap_or_else(|| if staging { LetsEncrypt::Staging.url() } else { LetsEncrypt::Production.url() }.to_string());
        let c = Arc::new(Self {
            dir: dir.to_path_buf(),
            directory,
            current: RwLock::new(None),
            status: RwLock::new(CertStatus { staging, ..Default::default() }),
            account: Mutex::new(None),
            ari: Mutex::new(None),
            wake: Notify::new(),
            on_change: Box::new(on_change),
        });
        if let (Ok(chain), Ok(key)) = (std::fs::read_to_string(dir.join("cert.pem")), std::fs::read_to_string(dir.join("key.pem")))
            && let Err(e) = c.install(&chain, &key, false)
        {
            log::warn!("HTTPS certificate unreadable ({e}); a new one will be made");
        }
        c
    }

    /// A TLS configuration that always serves the current certificate.
    pub fn server_config(self: &Arc<Self>) -> Arc<ServerConfig> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let mut c = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("TLS versions")
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(Resolver(self.clone())));
        c.alpn_protocols = vec![b"http/1.1".to_vec()];
        Arc::new(c)
    }

    /// A certificate for `fqdn` that hasn't expired.
    pub fn valid_for(&self, fqdn: &str) -> bool {
        self.current.read().as_ref().is_some_and(|c| c.facts.names.iter().any(|n| n == fqdn) && now() < c.facts.not_after)
    }

    pub fn status(&self) -> CertStatus {
        self.status.read().clone()
    }

    /// Look again now (the name or token changed).
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    fn update(&self, f: impl FnOnce(&mut CertStatus)) {
        f(&mut self.status.write());
        (self.on_change)();
    }

    fn step(&self, s: Option<&'static str>) {
        self.update(|st| st.step = s);
    }

    /// Keeps a valid certificate for whatever `want` returns (`None`: none
    /// needed right now). Runs forever.
    pub async fn run(self: Arc<Self>, want: impl Fn() -> Option<Want> + Send + Sync + 'static) {
        // after a failure: what failed and when to try again
        let mut failed: Option<(Want, Instant, u32)> = None;
        loop {
            let wait = match want() {
                None => None,
                Some(w) => {
                    let retry_later = failed.as_ref().filter(|(fw, at, _)| *fw == w && Instant::now() < *at).map(|(_, at, _)| *at - Instant::now());
                    match retry_later {
                        Some(d) => Some(d),
                        None => match self.due(&duckdns::fqdn(&w.domain)).await {
                            Some(d) => Some(d),
                            None => match self.obtain(&w).await {
                                Ok(()) => {
                                    failed = None;
                                    self.update(|s| {
                                        s.error = None;
                                        s.error_at = None;
                                        s.next_try = None;
                                        s.step = None;
                                    });
                                    continue;
                                }
                                Err(e) => {
                                    let n = failed.as_ref().filter(|(fw, ..)| *fw == w).map_or(1, |f| f.2 + 1);
                                    // 15 min, 1 h, 3 h, 6 h, then twice a day; Let's Encrypt allows
                                    // only a few failed checks per hour
                                    let d = Duration::from_secs([900, 3600, 3 * 3600, 6 * 3600, 12 * 3600][(n as usize - 1).min(4)]);
                                    log::warn!("HTTPS certificate for {}: {e} (next try in {} min)", duckdns::fqdn(&w.domain), d.as_secs() / 60);
                                    self.update(|s| {
                                        s.error = Some(e);
                                        s.error_at = Some(now());
                                        s.next_try = Some(now() + d.as_secs() as i64);
                                        s.step = None;
                                    });
                                    failed = Some((w, Instant::now() + d, n));
                                    Some(d)
                                }
                            },
                        },
                    }
                }
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

    /// `None`: a new certificate is needed now; else how long to wait before looking again.
    async fn due(&self, fqdn: &str) -> Option<Duration> {
        let facts = {
            let cur = self.current.read();
            match cur.as_ref() {
                Some(c) if c.facts.names.iter().any(|n| n == fqdn) => c.facts.clone(),
                _ => return None,
            }
        };
        let mut renew = facts.not_before + (facts.not_after - facts.not_before) * 2 / 3;
        if let Some(t) = self.renewal_info(&facts).await {
            renew = renew.min(t);
        }
        self.update(|s| s.renew_at = Some(renew));
        let left = renew - now();
        // look again at least every 6 hours (renewal information can change)
        (left > 0).then(|| Duration::from_secs(left.min(6 * 3600) as u64))
    }

    /// The renewal time Let's Encrypt suggests, asked at most every 6 hours.
    async fn renewal_info(&self, facts: &Facts) -> Option<i64> {
        if let Some((at, t)) = *self.ari.lock()
            && at.elapsed() < Duration::from_secs(6 * 3600)
        {
            return t;
        }
        let id = CertificateIdentifier::new(Der::from_slice(facts.aki.as_deref()?), Der::from_slice(&facts.serial));
        let account = self.account().await.ok()?;
        let t = match account.renewal_info(&id).await {
            Ok((info, _)) => {
                let (start, end) = (info.suggested_window.start.unix_timestamp(), info.suggested_window.end.unix_timestamp());
                // a random time inside the window spreads renewals out (RFC 9773)
                Some(start + ((end - start).max(0) as f64 * rand::random::<f64>()) as i64)
            }
            Err(e) => {
                log::debug!("renewal information: {e}");
                None
            }
        };
        *self.ari.lock() = Some((Instant::now(), t));
        t
    }

    async fn account(&self) -> Result<Account, String> {
        if let Some(a) = self.account.lock().clone() {
            return Ok(a);
        }
        let path = self.dir.join("account.json");
        let builder = || match std::env::var("TTV_ACME_ROOT").ok().filter(|_| cfg!(debug_assertions)) {
            Some(root) => Account::builder_with_root(root),
            None => Account::builder(),
        };
        let stored = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<StoredAccount>(&b).ok()).filter(|s| s.directory == self.directory);
        let account = match stored {
            Some(s) => builder().map_err(acme_err)?.from_credentials(s.credentials).await.map_err(acme_err)?,
            None => {
                let (account, credentials) = builder()
                    .map_err(acme_err)?
                    .create(&NewAccount { contact: &[], terms_of_service_agreed: true, only_return_existing: false }, self.directory.clone(), None)
                    .await
                    .map_err(acme_err)?;
                let json = serde_json::to_vec_pretty(&StoredAccount { directory: self.directory.clone(), credentials }).map_err(|e| e.to_string())?;
                write_private(&path, &json).map_err(|e| format!("can't keep the account: {e}"))?;
                log::info!("new Let's Encrypt account");
                account
            }
        };
        *self.account.lock() = Some(account.clone());
        Ok(account)
    }

    async fn obtain(&self, w: &Want) -> Result<(), String> {
        let fqdn = duckdns::fqdn(&w.domain);
        log::info!("getting an HTTPS certificate for {fqdn}");
        self.step(Some("account"));
        let account = self.account().await?;
        self.step(Some("order"));
        let ids = [Identifier::Dns(fqdn.clone())];
        // replacing our own certificate gets friendlier rate limits
        let replaces = self.current.read().as_ref().filter(|c| c.facts.names.contains(&fqdn)).and_then(|c| Some(CertificateIdentifier::new(Der::from_slice(c.facts.aki.as_deref()?), Der::from_slice(&c.facts.serial)).into_owned()));
        let mut order = match replaces {
            Some(r) => match account.new_order(&NewOrder::new(&ids).replaces(r)).await {
                Ok(o) => o,
                Err(_) => account.new_order(&NewOrder::new(&ids)).await.map_err(acme_err)?,
            },
            None => account.new_order(&NewOrder::new(&ids)).await.map_err(acme_err)?,
        };
        let mut txt_set = false;
        let r = self.validate(&mut order, w, &fqdn, &mut txt_set).await;
        if txt_set && let Err(e) = duckdns::clear_txt(&w.domain, &w.token).await {
            log::info!("TXT record not removed: {e}");
        }
        r?;
        self.step(Some("issuing"));
        let key = order.finalize().await.map_err(acme_err)?;
        let chain = order.poll_certificate(&RetryPolicy::new().timeout(Duration::from_secs(120))).await.map_err(acme_err)?;
        self.install(&chain, &key, true)?;
        *self.ari.lock() = None;
        self.step(None);
        log::info!("new HTTPS certificate for {fqdn}");
        Ok(())
    }

    async fn validate(&self, order: &mut Order, w: &Want, fqdn: &str, txt_set: &mut bool) -> Result<(), String> {
        {
            let mut auths = order.authorizations();
            while let Some(a) = auths.next().await {
                let mut a = a.map_err(acme_err)?;
                match a.status {
                    AuthorizationStatus::Pending => {}
                    AuthorizationStatus::Valid => continue,
                    s => return Err(format!("Let's Encrypt: the check is {s:?}")),
                }
                let mut ch = a.challenge(ChallengeType::Dns01).ok_or("Let's Encrypt offered no DNS check.")?;
                let value = ch.key_authorization().dns_value();
                self.step(Some("dns"));
                duckdns::set_txt(&w.domain, &w.token, &value).await?;
                *txt_set = true;
                let servers = dns_servers().await;
                if !dns::wait_for_txt(&servers, &format!("_acme-challenge.{fqdn}"), &value, Duration::from_secs(120)).await {
                    log::info!("the TXT record isn't on every DuckDNS name server yet - asking Let's Encrypt anyway");
                }
                ch.set_ready().await.map_err(acme_err)?;
            }
        }
        self.step(Some("validating"));
        let status = order.poll_ready(&RetryPolicy::new().timeout(Duration::from_secs(180))).await.map_err(acme_err)?;
        match status {
            OrderStatus::Ready => Ok(()),
            OrderStatus::Invalid => {
                // why: the challenge's own error
                let mut why = None;
                let mut auths = order.authorizations();
                while let Some(Ok(mut a)) = auths.next().await {
                    if let Ok(s) = a.refresh().await {
                        why = why.or_else(|| s.challenges.iter().find_map(|c| c.error.as_ref()).and_then(|p| p.detail.clone()));
                    }
                }
                Err(format!("Let's Encrypt couldn't confirm the name: {}", why.unwrap_or_else(|| "no reason given".into())))
            }
            s => Err(format!("Let's Encrypt: the order is {s:?}")),
        }
    }

    /// Takes a certificate chain and its key into use (and keeps them).
    fn install(&self, chain: &str, key: &str, save: bool) -> Result<(), String> {
        let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(chain.as_bytes()).collect::<Result<_, _>>().map_err(|e| format!("certificate: {e}"))?;
        let first = certs.first().ok_or("no certificate in the chain")?;
        let facts = der::facts(first.as_ref()).ok_or("certificate unreadable")?;
        let pk = PrivateKeyDer::from_pem_slice(key.as_bytes()).map_err(|e| format!("key: {e}"))?;
        let signing = rustls::crypto::aws_lc_rs::default_provider().key_provider.load_private_key(pk).map_err(|e| format!("key: {e}"))?;
        let ck = Arc::new(CertifiedKey::new(certs, signing));
        if save {
            std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
            write_private(&self.dir.join("key.pem"), key.as_bytes()).map_err(|e| format!("can't keep the key: {e}"))?;
            write_private(&self.dir.join("cert.pem"), chain.as_bytes()).map_err(|e| format!("can't keep the certificate: {e}"))?;
        }
        let renew = facts.not_before + (facts.not_after - facts.not_before) * 2 / 3;
        let (domain, from, until) = (facts.names.first().cloned(), facts.not_before, facts.not_after);
        *self.current.write() = Some(Current { key: ck, facts });
        self.update(|s| {
            s.domain = domain;
            s.valid_from = Some(from);
            s.valid_until = Some(until);
            s.renew_at = Some(renew);
        });
        Ok(())
    }
}

/// DuckDNS's name servers (a debug build may use `TTV_DNS_SERVER`, e.g. a test server).
async fn dns_servers() -> Vec<std::net::SocketAddr> {
    if cfg!(debug_assertions)
        && let Some(a) = std::env::var("TTV_DNS_SERVER").ok().and_then(|s| s.parse().ok())
    {
        return vec![a];
    }
    dns::duckdns_servers().await
}

/// Writes a file only this user can read, replacing the old one in one step.
fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

#[derive(Debug)]
struct Resolver(Arc<Certs>);

impl std::fmt::Debug for Certs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Certs").field("dir", &self.dir).finish()
    }
}

impl ResolvesServerCert for Resolver {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        self.0.current.read().as_ref().map(|c| c.key.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn self_signed(name: &str, days: i64) -> (String, String) {
        let mut p = rcgen::CertificateParams::new(vec![name.to_string()]).unwrap();
        let t = time::OffsetDateTime::now_utc();
        p.not_before = t - time::Duration::days(1);
        p.not_after = t + time::Duration::days(days);
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = p.self_signed(&key).unwrap();
        (cert.pem(), key.serialize_pem())
    }

    #[test]
    fn installs_keeps_and_reloads_a_certificate() {
        let dir = std::env::temp_dir().join(format!("ttv-net-certs-{}", rand::random::<u32>()));
        let changes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let n = changes.clone();
        let c = Certs::new(&dir, move || {
            n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
        assert!(!c.valid_for("my.duckdns.org"));
        let (chain, key) = self_signed("my.duckdns.org", 90);
        c.install(&chain, &key, true).unwrap();
        assert!(c.valid_for("my.duckdns.org"));
        assert!(!c.valid_for("other.duckdns.org"));
        assert!(changes.load(std::sync::atomic::Ordering::Relaxed) >= 1);
        let s = c.status();
        assert_eq!(s.domain.as_deref(), Some("my.duckdns.org"));
        let (from, until, renew) = (s.valid_from.unwrap(), s.valid_until.unwrap(), s.renew_at.unwrap());
        assert!(renew > from && renew < until && (renew - from) * 3 / 2 - (until - from) < 10);
        // a new start finds it again
        let again = Certs::new(&dir, || {});
        assert!(again.valid_for("my.duckdns.org"));
        assert!(c.install("nonsense", &key, false).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_expired_or_foreign_certificate_is_due_at_once() {
        let dir = std::env::temp_dir().join(format!("ttv-net-certs-{}", rand::random::<u32>()));
        let c = Certs::new(&dir, || {});
        assert_eq!(c.due("my.duckdns.org").await, None, "no certificate yet");
        let (chain, key) = self_signed("other.duckdns.org", 90);
        c.install(&chain, &key, false).unwrap();
        assert_eq!(c.due("my.duckdns.org").await, None, "for another name");
        // a fresh one is not due (no renewal info for a self-signed certificate: no AKI)
        let (chain, key) = self_signed("my.duckdns.org", 90);
        c.install(&chain, &key, false).unwrap();
        let d = c.due("my.duckdns.org").await.expect("not due");
        assert_eq!(d, Duration::from_secs(6 * 3600));
        let (chain, key) = self_signed("my.duckdns.org", 0);
        c.install(&chain, &key, false).unwrap();
        assert_eq!(c.due("my.duckdns.org").await, None, "two thirds have passed");
    }

    /// Live, the whole way with a real DuckDNS name against Let's Encrypt's
    /// staging system: TXT record through DuckDNS, wait for DuckDNS's name
    /// servers, validation, issuing, install, TXT removed again.
    /// `TTV_LIVE_DUCKDNS_DOMAIN=name TTV_LIVE_DUCKDNS_TOKEN=… cargo test -p ttv-net duckdns_staging -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn duckdns_staging_live() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let (Ok(domain), Ok(token)) = (std::env::var("TTV_LIVE_DUCKDNS_DOMAIN"), std::env::var("TTV_LIVE_DUCKDNS_TOKEN")) else {
            eprintln!("TTV_LIVE_DUCKDNS_DOMAIN / TTV_LIVE_DUCKDNS_TOKEN not set - skipped");
            return;
        };
        // SAFETY: set before anything reads it; no other test uses it
        unsafe { std::env::set_var("TTV_ACME_STAGING", "1") };
        let dir = std::env::temp_dir().join(format!("ttv-net-duckdns-{}", rand::random::<u32>()));
        let c = Certs::new(&dir, || {});
        let w = Want { domain: domain.clone(), token };
        let fqdn = duckdns::fqdn(&domain);
        let t0 = Instant::now();
        let r = c.obtain(&w).await;
        println!("{} ms: {r:?}", t0.elapsed().as_millis());
        r.unwrap();
        assert!(c.valid_for(&fqdn));
        let s = c.status();
        println!("{s:?}");
        assert!(s.staging && s.domain.as_deref() == Some(fqdn.as_str()) && s.step.is_none());
        // the TXT record is gone again
        let servers = dns::duckdns_servers().await;
        let txt = dns::txt(servers[0], &format!("_acme-challenge.{fqdn}")).await.unwrap();
        println!("TXT after: {txt:?}");
        assert!(txt.iter().all(|t| t.is_empty()), "{txt:?}");
        // a second start finds the certificate and doesn't want a new one
        let again = Certs::new(&dir, || {});
        assert!(again.due(&fqdn).await.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live, against Let's Encrypt's staging (test) system: an account (kept
    /// and reused), an order for a DuckDNS name and its DNS check value – up to
    /// where DuckDNS would set the TXT record (the unanswered check expires there).
    /// `cargo test -p ttv-net staging -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn staging_account_and_order() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        // SAFETY: set before anything reads it; no other test uses it
        unsafe { std::env::set_var("TTV_ACME_STAGING", "1") };
        let dir = std::env::temp_dir().join(format!("ttv-net-staging-{}", rand::random::<u32>()));
        let c = Certs::new(&dir, || {});
        assert!(c.status().staging);
        let t0 = Instant::now();
        let account = c.account().await.unwrap();
        println!("account in {} ms", t0.elapsed().as_millis());
        assert!(dir.join("account.json").exists());
        // a second start reuses the stored account
        let again = Certs::new(&dir, || {});
        let t1 = Instant::now();
        let same = again.account().await.unwrap();
        assert_eq!(account.id(), same.id());
        println!("stored account in {} ms", t1.elapsed().as_millis());
        let name = format!("ttv-tts-test-{}.duckdns.org", rand::random::<u32>());
        let ids = [Identifier::Dns(name.clone())];
        let mut order = account.new_order(&NewOrder::new(&ids)).await.unwrap();
        let mut auths = order.authorizations();
        let mut a = auths.next().await.unwrap().unwrap();
        assert_eq!(a.status, AuthorizationStatus::Pending);
        let value = a.challenge(ChallengeType::Dns01).expect("a DNS check").key_authorization().dns_value();
        println!("{name}: TXT _acme-challenge = {value}");
        assert_eq!(value.len(), 43, "base64url of a SHA-256");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
#[path = "pebble_test.rs"]
mod pebble_test;
