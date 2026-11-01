//! The library of computed voices: an append-only log (`library.jsonl`) so
//! adding one voice never rewrites the whole file, even with tens of
//! thousands of entries. Compacted on load when removals pile up.
//!
//! Every change bumps a revision once and is handed out as a small
//! [`LibraryChange`], so clients patch their copy instead of refetching.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use ttv_link::{LibraryChange, LibraryEntry, LibrarySnapshot};

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
// Lines only live while being written; boxing the entry gains nothing.
#[allow(clippy::large_enum_variant)]
enum Line {
    Add { entry: LibraryEntry },
    Del { key: String },
}

struct State {
    map: HashMap<String, LibraryEntry>,
    rev: u64,
}

pub struct Library {
    path: PathBuf,
    state: Mutex<State>,
}

impl Library {
    pub fn load(path: PathBuf) -> Self {
        let mut map: HashMap<String, LibraryEntry> = HashMap::new();
        let mut lines = 0usize;
        if let Ok(text) = std::fs::read_to_string(&path) {
            for l in text.lines().filter(|l| !l.trim().is_empty()) {
                lines += 1;
                match serde_json::from_str::<Line>(l) {
                    Ok(Line::Add { entry }) => {
                        map.insert(entry.key.clone(), entry);
                    }
                    Ok(Line::Del { key }) => {
                        map.remove(&key);
                    }
                    Err(e) => log::warn!("library: skipped a damaged line ({e})"),
                }
            }
        }
        // A random start revision: a client that kept a copy from before a
        // restart never mistakes it for the current one.
        let rev = (rand::random::<u32>() as u64) << 20;
        let lib = Self { path, state: Mutex::new(State { map, rev }) };
        // Rewrite when more than half of the log is history.
        if lines > 64 && lines > lib.len() * 2 {
            lib.compact();
        }
        lib
    }

    fn append(&self, lines: &[Line]) {
        if lines.is_empty() {
            return;
        }
        let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) else {
            log::warn!("library: can't write {}", self.path.display());
            return;
        };
        let mut s = String::new();
        for l in lines {
            s.push_str(&serde_json::to_string(l).unwrap_or_default());
            s.push('\n');
        }
        let _ = f.write_all(s.as_bytes());
    }

    fn compact(&self) {
        let st = self.state.lock();
        let tmp = self.path.with_extension("tmp");
        let mut out = String::new();
        for e in st.map.values() {
            out.push_str(&serde_json::to_string(&Line::Add { entry: e.clone() }).unwrap_or_default());
            out.push('\n');
        }
        if std::fs::write(&tmp, out).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }

    /// Adds and removes entries in one step. None if nothing changed.
    pub fn apply(&self, added: Vec<LibraryEntry>, removed: &[String]) -> Option<LibraryChange> {
        let mut st = self.state.lock();
        let mut lines = Vec::new();
        let mut gone = Vec::new();
        for k in removed {
            if st.map.remove(k).is_some() {
                lines.push(Line::Del { key: k.clone() });
                gone.push(k.clone());
            }
        }
        for e in &added {
            st.map.insert(e.key.clone(), e.clone());
            lines.push(Line::Add { entry: e.clone() });
        }
        if lines.is_empty() {
            return None;
        }
        self.append(&lines);
        let base = st.rev;
        st.rev += 1;
        Some(LibraryChange { base, rev: st.rev, added, removed: gone })
    }

    /// Whether a VCP voice belongs to the library (guests may only use these).
    pub fn has_voice(&self, voice_id: &str) -> bool {
        self.state.lock().map.values().any(|e| e.voice_id == voice_id)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.state.lock().map.contains_key(key)
    }

    pub fn get(&self, key: &str) -> Option<LibraryEntry> {
        self.state.lock().map.get(key).cloned()
    }

    pub fn len(&self) -> usize {
        self.state.lock().map.len()
    }

    pub fn entries(&self) -> Vec<LibraryEntry> {
        self.state.lock().map.values().cloned().collect()
    }

    pub fn snapshot(&self) -> LibrarySnapshot {
        let st = self.state.lock();
        LibrarySnapshot { rev: st.rev, entries: st.map.values().cloned().collect() }
    }

    pub fn rev(&self) -> u64 {
        self.state.lock().rev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survives_reload_and_compacts() {
        let dir = std::env::temp_dir().join(format!("ttv-lib-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("library.jsonl");
        let lib = Library::load(path.clone());
        for i in 0..100 {
            lib.apply(vec![LibraryEntry { key: format!("k{i}:de"), voice_id: format!("voice_{i}"), ..Default::default() }], &[]);
        }
        let keys: Vec<String> = (0..80).map(|i| format!("k{i}:de")).collect();
        let change = lib.apply(vec![], &keys).unwrap();
        assert_eq!(change.removed.len(), 80);
        assert_eq!(change.rev, change.base + 1);
        assert!(lib.apply(vec![], &keys).is_none(), "removing again changes nothing");
        let again = Library::load(path.clone());
        assert_eq!(again.len(), 20);
        assert!(again.contains("k99:de") && !again.contains("k0:de"));
        // compacted: one line per remaining entry
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 20);
        let _ = std::fs::remove_dir_all(dir);
    }
}
