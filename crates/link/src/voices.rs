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
    /// Quality check: listen to every new voice speaking a new sentence and
    /// list the ones that may sound odd.
    pub quality: bool,
    /// Design odd voices again automatically and keep the better one.
    pub improve: bool,
    /// Keep every usable take for the streamer to choose from ("let me
    /// pick") instead of saving the best one.
    pub pick: bool,
    /// Ready-made packs: when a voice is finished (checked, improved), keep
    /// it only in the engine's archive (lossless sample + data, exportable as
    /// a pack) and remove it from VCP, so VCP's own list stays small.
    pub archive: bool,
}

/// Takes of a voice designed again, waiting for the streamer's choice.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TakeChoice {
    pub key: String,
    /// The voice definition's name.
    pub name: String,
    pub lang: String,
    pub preset_id: String,
    pub index: u32,
    /// Unix seconds.
    pub created: i64,
    pub takes: Vec<TakeInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TakeInfo {
    pub n: u32,
    /// Seconds.
    pub duration: f32,
    /// Speech check: share of misheard words (None = not checked).
    pub wer: Option<f32>,
    /// How well it fits the description (pitch, loudness, pauses, pace).
    pub score: f32,
    /// The take the engine would have kept.
    pub recommended: bool,
}

impl Default for JobOptions {
    fn default() -> Self {
        Self { speech_check: true, takes: 2, backup: true, quality: false, improve: false, pick: false, archive: false }
    }
}

/// What a job does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    /// Design the voices that aren't computed yet.
    #[default]
    Compute,
    /// Design computed voices again; the new one replaces the old one only
    /// if it sounds better (quality score).
    Improve,
    /// Check the quality of computed voices (no new designs).
    Review,
    /// Save the voices of a pack in VCP (no new designs).
    Import,
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
    pub kind: JobKind,
    /// Only these voice keys (empty = all of defs × languages).
    pub only: Vec<String>,
    /// Improve: designs per voice before keeping the old one (default 2).
    pub rounds: u8,
    /// Import: the pack file on the engine's PC.
    pub pack: String,
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
    pub kind: JobKind,
    /// Voices this job has to compute (definitions × languages, minus those
    /// that already existed when it started), improve or check.
    pub total: u32,
    pub done: u32,
    pub failed: u32,
    /// Quality check: voices found that may sound odd.
    pub flagged: u32,
    /// Improve: voices replaced by a better design.
    pub improved: u32,
    pub state: JobState,
    /// "design", "check", "save", "prime", "review", "speak"
    pub phase: String,
    /// The voice being worked on ("Vic Malone · de").
    pub current: Option<String>,
    /// Voices in the current batch (they are designed, checked and saved
    /// together, so `done` grows in steps of a batch).
    pub batch: u32,
    /// Progress inside the current phase of the batch (take 12 of 32).
    pub step: u32,
    pub steps: u32,
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
    /// Speech loudness of the reference sample in dBFS (None = not measured
    /// yet; "Check voices" measures it once and corrects the gain).
    pub level: Option<f32>,
    /// Quality check result (None = not checked).
    pub quality: Option<Quality>,
    /// A lossless copy of the sample exists on the engine's PC.
    pub backup: bool,
    /// Archived voices: what the sample says (VCP no longer knows it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_text: Option<String>,
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
    /// Voices kept only in the engine's archive (for ready-made packs).
    pub archived: u32,
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

/// Result of checking the library against VCP.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Verified {
    /// Library entries looked at.
    pub checked: u32,
    /// Entries removed because their VCP voice is gone.
    pub removed: u32,
    /// Voices whose loudness was measured (once) and gain corrected.
    pub measured: u32,
}

/// Result of the quality check of a computed voice.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Quality {
    /// 1 = nothing odd; below 0.75 the voice is listed for review.
    pub score: f32,
    pub issues: Vec<QualityIssue>,
    /// Median pitch of the reference sample (Hz).
    pub f0: Option<f32>,
    /// Unix seconds.
    pub checked: i64,
    /// The check included speaking a new sentence.
    pub new_text: bool,
    /// The user listened and keeps the voice as it is.
    pub kept: bool,
}

/// One thing that may sound odd ("pitch_high", "quiet", "new_misheard", …).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QualityIssue {
    pub code: String,
    /// The measured value (Hz, dB, seconds, ratio, error rate – see code).
    pub value: f32,
    /// How much it lowers the score (0–1).
    pub weight: f32,
    /// e.g. the other language for "languages_differ".
    pub extra: Option<String>,
}

/// `manifest.json` of a voice pack (`.ttvpack`, a zip file): presets and
/// computed voices with their reference samples (FLAC) – to share voices
/// with friends or move them to another VCP without computing them again.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PackManifest {
    /// "ttv-tts-pack"
    pub format: String,
    pub version: u32,
    /// Unix seconds.
    pub created: i64,
    /// TTV-TTS version that made it.
    pub app: String,
    pub title: String,
    pub presets: Vec<PackPreset>,
    pub voices: Vec<PackVoice>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PackPreset {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    /// The preset in the line format (own presets; built-ins too, so the pack
    /// works with an app that doesn't know them).
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PackVoice {
    pub key: String,
    pub def_hash: String,
    pub lang: String,
    pub preset_id: String,
    pub index: u32,
    pub name: String,
    /// The definition's description (VCP keeps it as the voice's transcript).
    pub description: String,
    /// The sample inside the zip ("voices/<key>.flac").
    pub file: String,
    /// What the sample says.
    pub ref_text: String,
    pub duration: f32,
    pub wer: Option<f32>,
    pub gain: f32,
    pub level: Option<f32>,
    pub quality: Option<Quality>,
}

/// `POST /ttv/v1/packs/export`: what goes into a pack. The engine knows the
/// voices, the app knows the definitions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportRequest {
    pub title: String,
    pub presets: Vec<PackPreset>,
    pub voices: Vec<ExportVoice>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportVoice {
    pub key: String,
    pub preset_id: String,
    pub index: u32,
    pub name: String,
    pub description: String,
}
