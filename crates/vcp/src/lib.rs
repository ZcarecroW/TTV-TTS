//! Typed client for the local REST API of Voice Creator Pro (VCP).
//!
//! Only the endpoints TTV-TTS needs are typed; everything else can go through
//! [`Client::request`]. Response types are lenient (`#[serde(default)]`,
//! unknown fields ignored) so a VCP update that adds fields never breaks us.

mod client;
mod types;
pub mod perf;
pub mod wav;

pub use client::{Client, Error, Turn, background_turn, before_next_chat, exclusive, is_work, set_exclusive, turn};
pub use reqwest::Method;
pub use types::*;
