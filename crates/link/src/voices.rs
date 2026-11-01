//! Voice definitions, computation jobs and the library of computed voices –
//! shared by the app, the proxy and the job engine.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One voice of a preset, independent of language: VCP designs it from the
/// description, the language's accent is added when it is computed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceDef {
    pub name: String,
    /// "m", "f" or "n"
    pub gender: String,
    pub age: u8,
    /// English description of the voice (timbre, pitch, pace, attitude).
    pub description: String,
    /// Optional effect applied when playing (e.g. "radio").
    pub fx: Option<String>,
}

impl VoiceDef {
    /// Stable id of what makes the voice sound the way it does. Renaming a
    /// voice keeps it; changing gender, age or description makes it new.
    pub fn hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.gender.as_bytes());
        h.update([0, self.age]);
        h.update(self.description.trim().as_bytes());
        let d = h.finalize();
        format!("v{}", hex::encode(&d[..6]))
    }

    /// Key of one computed voice: definition and language.
    pub fn key(&self, lang: &str) -> String {
        format!("{}:{lang}", self.hash())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JobDef {
    /// Position in the preset (0-based), used in the voice's name.
    pub index: u32,
    pub def: VoiceDef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JobOptions {
    /// Check every take with speech recognition and retry bad ones.
    pub speech_check: bool,
    /// Takes designed per voice; the best one is kept (1–3).
    pub takes: u8,
    /// Keep a lossless copy of each voice's sample on the engine's PC.
    pub backup: bool,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self { speech_check: true, takes: 2, backup: true }
    }
}

/// `POST /ttv/v1/jobs`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JobRequest {
    pub preset_id: String,
    pub preset_name: String,
    pub defs: Vec<JobDef>,
    /// Language codes (de, en, …).
    pub languages: Vec<String>,
    pub options: JobOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    #[default]
    Queued,
    Running,
    /// Paused by the user.
    Paused,
    /// Waits for a quiet moment in chat (chat goes first).
    Waiting,
    Done,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JobView {
    pub id: String,
    pub preset_id: String,
    pub preset_name: String,
    pub languages: Vec<String>,
    pub options: JobOptions,
    /// Voices this job has to compute (definitions × languages, minus those
    /// that already existed when it started).
    pub total: u32,
    pub done: u32,
    pub failed: u32,
    pub state: JobState,
    /// "design", "check", "save", "prime"
    pub phase: String,
    /// The voice being worked on ("Vic Malone · de").
    pub current: Option<String>,
    /// Unix seconds.
    pub created: i64,
    pub finished: Option<i64>,
    /// Measured seconds per voice (moving average).
    pub secs_per_voice: Option<f32>,
    pub eta_secs: Option<u64>,
    pub error: Option<String>,
}

/// One computed voice in the library.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryEntry {
    /// `VoiceDef::key` (definition hash + language).
    pub key: String,
    pub def_hash: String,
    pub lang: String,
    pub preset_id: String,
    pub index: u32,
    pub name: String,
    /// VCP voice id and name.
    pub voice_id: String,
    pub voice_name: String,
    pub created: i64,
    /// Length of the reference sample (s).
    pub duration: f32,
    /// Word error rate of the speech check (None = not checked).
    pub wer: Option<f32>,
    /// Loudness correction (linear gain) so all voices play equally loud.
    pub gain: f32,
    /// A lossless copy of the sample exists on the engine's PC.
    pub backup: bool,
}

/// `GET /ttv/v1/library`: the whole library at revision `rev`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibrarySnapshot {
    pub rev: u64,
    pub entries: Vec<LibraryEntry>,
}

/// One change of the library (event). A client whose copy is at `base`
/// applies it and is then at `rev`; any other client refetches.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryChange {
    pub base: u64,
    pub rev: u64,
    pub added: Vec<LibraryEntry>,
    pub removed: Vec<String>,
}

/// `GET /ttv/v1/engine`: library summary and running jobs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineStatus {
    pub jobs: Vec<JobView>,
    /// Measured seconds per voice from earlier jobs (None = never measured).
    pub secs_per_voice: Option<f32>,
    pub library_size: u32,
    /// Bumped whenever the library changes (clients refetch when it moves).
    pub library_rev: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_ignores_name_but_not_description() {
        let a = VoiceDef { name: "A".into(), gender: "m".into(), age: 40, description: "Deep warm baritone.".into(), fx: None };
        let mut b = a.clone();
        b.name = "B".into();
        assert_eq!(a.hash(), b.hash());
        b.description = "Deep warm baritone, slow.".into();
        assert_ne!(a.hash(), b.hash());
        assert_eq!(a.key("de"), format!("{}:de", a.hash()));
        assert_eq!(a.hash().len(), 13);
    }
}
