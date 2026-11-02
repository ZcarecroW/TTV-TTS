//! Small JSON files in the data folder, written atomically (temp file +
//! rename) so a crash or power loss never leaves a half-written file.

use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("{} is damaged ({e}); keeping a copy and starting fresh", path.display());
            let _ = std::fs::copy(path, path.with_extension("damaged.json"));
            None
        }
    }
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)
}
