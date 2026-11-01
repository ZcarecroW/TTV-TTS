//! Certificate pinning: the proxy's self-signed certificate is trusted by its
//! SHA-256 fingerprint, learned once when pairing.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error, SignatureScheme};
use sha2::{Digest, Sha256};

/// Lower-case hex SHA-256 of a DER certificate.
pub fn fingerprint(der: &[u8]) -> String {
    hex::encode(Sha256::digest(der))
}

/// Short, human-comparable form: first 16 hex digits in groups of four,
/// e.g. "3F9A-0C12-77DE-41B0". Shown on both sides when pairing.
pub fn short(fp: &str) -> String {
    let up = fp.to_ascii_uppercase();
    let s = &up[..up.len().min(16)];
    s.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap_or("")).collect::<Vec<_>>().join("-")
}

/// Accepts exactly one certificate (by fingerprint), whatever its names or
/// dates say. Signatures of the handshake are still verified normally.
#[derive(Debug)]
pub struct PinnedVerifier {
    fingerprint: String,
    algorithms: WebPkiSupportedAlgorithms,
}

impl PinnedVerifier {
    pub fn new(fingerprint: &str, provider: &CryptoProvider) -> Arc<Self> {
        Arc::new(Self {
            fingerprint: fingerprint.to_ascii_lowercase(),
            algorithms: provider.signature_verification_algorithms,
        })
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if fingerprint(end_entity.as_ref()) == self.fingerprint {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::General("the proxy's certificate changed (fingerprint mismatch) - pair again".into()))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// Accepts any certificate and reports its fingerprint: used only for the
/// very first contact (discovery / manual address) before pairing, so the
/// app can show the fingerprint and compare it with the proxy window.
#[derive(Debug)]
pub struct LearningVerifier {
    algorithms: WebPkiSupportedAlgorithms,
    seen: std::sync::Mutex<Option<String>>,
}

impl LearningVerifier {
    pub fn new(provider: &CryptoProvider) -> Arc<Self> {
        Arc::new(Self { algorithms: provider.signature_verification_algorithms, seen: Default::default() })
    }

    pub fn seen(&self) -> Option<String> {
        self.seen.lock().ok()?.clone()
    }
}

impl ServerCertVerifier for LearningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if let Ok(mut s) = self.seen.lock() {
            *s = Some(fingerprint(end_entity.as_ref()));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_form() {
        let fp = fingerprint(b"hello");
        assert_eq!(fp.len(), 64);
        assert_eq!(short(&fp), "2CF2-4DBA-5FB0-A30E");
    }
}
