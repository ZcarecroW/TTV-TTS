//! Response types of the VCP endpoints TTV-TTS uses (VCP 2.4.x). All lenient.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct VersionResponse {
    pub version: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct GenerationQueue {
    pub queue_size: u32,
    pub is_generating: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct GenerationProgress {
    /// idle, generating, streaming, complete, error, cancelled
    pub status: String,
    pub message: String,
    pub percent: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelStatus {
    pub loaded: bool,
    /// base, voice_design, design, custom_voice …
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub quality: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelList {
    pub models: Vec<ModelCapabilities>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelCapabilities {
    pub model_family: String,
    pub display_name: String,
    pub supports_clone: bool,
    pub supports_design: bool,
    pub supports_custom_voice: bool,
    pub supports_streaming: bool,
    pub supports_emotional_clone: bool,
    pub sample_rate: u32,
    pub ref_min_sec: f32,
    pub ref_min_speech_sec: f32,
    pub ref_max_sec: f32,
    pub custom_voice_speakers: Vec<String>,
    pub generation_params: Vec<GenerationParam>,
    pub variants: Vec<ModelVariant>,
    pub studio_only: bool,
    pub loading: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct GenerationParam {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub default: serde_json::Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub variants: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelVariant {
    pub id: String,
    pub name: String,
    pub tier: String,
    pub size_label: String,
    pub vram_estimate_mb: u32,
    pub download_size_gb: f32,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct LanguageList {
    pub languages: Vec<Language>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Language {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceList {
    pub voices: Vec<Voice>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Voice {
    pub id: String,
    pub name: String,
    /// For design voices VCP stores the natural-language description here.
    pub transcript: String,
    pub ref_text: Option<String>,
    /// "clone" or "design"
    pub voice_type: String,
    pub created_at: String,
    pub duration_seconds: f32,
    /// true = primed (voice prompt cache exists), streams via /speak.
    pub has_voice_clone_prompt: bool,
    pub created_with_model: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SystemInfo {
    pub info: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct GpuDevices {
    pub devices: Vec<GpuDevice>,
    pub current_index: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct GpuDevice {
    pub index: u32,
    pub name: String,
    pub vram_mb: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct AgentSettings {
    pub default_voice: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct HttpApiSettings {
    pub enabled: bool,
    pub port: u16,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelQuality {
    pub current: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct EmotionSettings {
    pub engine: String,
    pub emotions: Vec<Emotion>,
    pub intensities: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Emotion {
    pub name: String,
    pub default_intensity: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceSaveResponse {
    pub success: bool,
    pub voice: Option<Voice>,
    pub message: Option<String>,
}

/// Result of a `?sync=true` generation (`audio_base64` = WAV).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct GenerationResult {
    pub success: bool,
    pub status: String,
    pub audio_base64: Option<String>,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_samples() {
        // Shapes as returned by VCP 2.4.12 (2026-11-01).
        let m: ModelStatus = serde_json::from_str(r#"{"loaded":true,"type":"design","quality":"higher_quality","name":"Higher Quality (1.7B)"}"#).unwrap();
        assert!(m.loaded);
        assert_eq!(m.kind.as_deref(), Some("design"));
        let v: VoiceList = serde_json::from_str(r#"{"voices":[{"id":"voice_5bd048a5","name":"TTS-zh 28 Shulan","transcript":"x","ref_text":null,"voice_type":"clone","created_at":"2026-10-17T19:23:52.820221","duration_seconds":7.2,"has_voice_clone_prompt":true,"word_timestamps":null,"created_with_model":"qwen3-tts","starter_key":null,"source":null}]}"#).unwrap();
        assert_eq!(v.voices[0].id, "voice_5bd048a5");
        assert!(v.voices[0].has_voice_clone_prompt);
        let q: GenerationQueue = serde_json::from_str(r#"{"queue_size":0,"is_generating":false}"#).unwrap();
        assert!(!q.is_generating);
    }
}

/// A saved voice's reference sample (`GET /api/v1/voices/{id}`).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct VoiceSample {
    /// WAV, base64.
    pub audio_base64: String,
    /// What the sample says (VCP 2.4: set for design voices).
    pub ref_text: Option<String>,
    /// Design voices: the description; cloned and 1.x voices: what it says.
    pub transcript: Option<String>,
}

impl VoiceSample {
    /// The words spoken in the sample, if VCP knows them.
    pub fn spoken_text(&self) -> Option<&str> {
        let r = self.ref_text.as_deref().map(str::trim).filter(|t| !t.is_empty());
        // TTV-TTS always sets ref_text; voices from 1.x keep the spoken words in
        // the transcript (and leave ref_text empty).
        r.or_else(|| self.transcript.as_deref().map(str::trim).filter(|t| !t.is_empty()))
    }
}
