//! The proxy's self-signed TLS certificate. Created once (ECDSA P-256, valid
//! for 30 years) and kept in the data folder; apps pin its fingerprint when
//! they pair, so it must stay stable. "Reset" creates a new one and every app
//! has to pair again.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rcgen::{CertificateParams, DnType, KeyPair};
use rustls::ServerConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use crate::error::{AppError, AppResult};

pub struct TlsMaterial {
    pub config: Arc<ServerConfig>,
    pub fingerprint: String,
}

fn paths(dir: &Path) -> (PathBuf, PathBuf) {
    (dir.join("proxy-cert.pem"), dir.join("proxy-key.pem"))
}

/// Loads the certificate, creating it on first use.
pub fn load_or_create(dir: &Path, name: &str) -> AppResult<TlsMaterial> {
    let (cert_path, key_path) = paths(dir);
    if !(cert_path.exists() && key_path.exists()) {
        create(dir, name)?;
    }
    match load(&cert_path, &key_path) {
        Ok(m) => Ok(m),
        Err(e) => {
            log::warn!("TLS certificate unreadable ({e}); creating a new one - apps must pair again");
            create(dir, name)?;
            load(&cert_path, &key_path)
        }
    }
}

/// Throws the old certificate away and makes a new one.
pub fn recreate(dir: &Path, name: &str) -> AppResult<TlsMaterial> {
    let (cert_path, key_path) = paths(dir);
    create(dir, name)?;
    load(&cert_path, &key_path)
}

fn create(dir: &Path, name: &str) -> AppResult<()> {
    std::fs::create_dir_all(dir)?;
    let mut params = CertificateParams::new(vec!["ttv-tts-proxy".to_string(), "localhost".to_string()])
        .map_err(|e| AppError::msg(format!("certificate: {e}")))?;
    params.distinguished_name.push(DnType::CommonName, format!("TTV-TTS Proxy ({name})"));
    params.distinguished_name.push(DnType::OrganizationName, "TTV-TTS");
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(365 * 30);
    let key = KeyPair::generate().map_err(|e| AppError::msg(format!("certificate key: {e}")))?;
    let cert = params.self_signed(&key).map_err(|e| AppError::msg(format!("certificate: {e}")))?;
    let (cert_path, key_path) = paths(dir);
    std::fs::write(&key_path, key.serialize_pem())?;
    std::fs::write(&cert_path, cert.pem())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
    }
    log::info!("created a new TLS certificate");
    Ok(())
}

fn load(cert_path: &Path, key_path: &Path) -> AppResult<TlsMaterial> {
    let cert = CertificateDer::from_pem_file(cert_path).map_err(|e| AppError::msg(format!("certificate file: {e}")))?;
    let key = PrivateKeyDer::from_pem_file(key_path).map_err(|e| AppError::msg(format!("key file: {e}")))?;
    let fingerprint = ttv_link::pin::fingerprint(cert.as_ref());
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| AppError::msg(format!("TLS: {e}")))?
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| AppError::msg(format!("TLS: {e}")))?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsMaterial { config: Arc::new(config), fingerprint })
}
