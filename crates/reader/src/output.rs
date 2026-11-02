//! Where the voices play: an audio device (the app) or an OBS browser source
//! ([`crate::obs`]).

use std::time::Duration;

/// An audio output. Clips are mono, at any sample rate.
pub trait Output: Send + Sync {
    /// Queues a clip; it plays right after the earlier ones. Returns its id.
    fn play(&self, samples: Vec<f32>, rate: u32) -> u64;
    /// Clip `id` has played (or was skipped).
    fn is_finished(&self, id: u64) -> bool;
    /// Stops everything queued up to now.
    fn skip_all(&self);
    /// Somebody hears it. An audio device always counts; an OBS link only
    /// while a browser source is connected.
    fn listening(&self) -> bool {
        true
    }
}

/// Waits until clip `id` has played (or was skipped).
pub async fn wait(out: &dyn Output, id: u64) {
    while !out.is_finished(id) {
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
}
