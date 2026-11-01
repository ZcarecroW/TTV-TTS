//! Typed client for the local REST API of Voice Creator Pro (VCP).
//!
//! Only the endpoints TTV-TTS needs are typed; everything else can go through
//! [`Client::request`]. Response types are lenient (`#[serde(default)]`,
//! unknown fields ignored) so a VCP update that adds fields never breaks us.

mod client;
mod types;
pub mod wav;

pub use client::{Client, Error};
pub use types::*;
