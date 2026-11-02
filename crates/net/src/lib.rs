//! The internet side of TTV-TTS, shared by the app and the proxy:
//!
//! - [`duckdns`]: keeps a free `name.duckdns.org` pointing at a home
//!   connection whose address changes, and sets its TXT record.
//! - [`certs`]: free HTTPS certificates from Let's Encrypt for that name
//!   (DNS-01 check through DuckDNS), renewed on their own.
//! - [`serve`]: HTTP and HTTPS on one port (LAN links stay plain HTTP,
//!   links from the internet use HTTPS).
//! - [`upnp`]: opens the ports in the home router by itself.
//! - [`dns`]: the tiny DNS client that sees when the TXT record is out.

pub mod certs;
mod der;
pub mod dns;
pub mod duckdns;
pub mod limits;
pub mod serve;
pub mod upnp;

pub use certs::{CertStatus, Certs, Want};
pub use serve::Secure;
