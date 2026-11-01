//! What TTV-TTS (the app) and TTV-TTS Proxy (on the Voice Creator Pro PC)
//! share: the API types, the LAN discovery packets and certificate pinning.
//!
//! The proxy speaks HTTPS with a self-signed certificate. The app pins that
//! certificate's SHA-256 fingerprint when it pairs, so no certificate
//! authority is involved and nobody else on the LAN can pose as the proxy.

pub mod discovery;
pub mod pin;
pub mod protocol;

pub use protocol::*;
