//! The TTV-TTS chat reader, shared by the app (audio on this PC) and the
//! proxy (OBS links on the voice host PC): Twitch chat, the profile's text
//! rules, voice choice, speech generation with VCP, voice effects, event
//! announcements and translation.

pub mod announce;
pub mod output;
pub mod reader;
pub mod storage;
pub mod fx;
pub mod obs;
pub mod profile;
pub mod speech;
pub mod text;
pub mod translate;
pub mod twitch;

/// A random key of 32 URL-safe characters without look-alikes (0/O, 1/l/I).
pub fn random_key() -> String {
    use rand::RngExt;
    const ABC: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789-_";
    let mut rng = rand::rng();
    (0..32).map(|_| ABC[rng.random_range(0..ABC.len())] as char).collect()
}

#[cfg(test)]
mod hostile_tests;
