//! "Let me pick": a voice designed again keeps all its usable takes for the
//! streamer to listen to, instead of the engine keeping the best one. The
//! old voice stays in use until a take is chosen. Each voice's takes live in
//! `takes/<key>/` (WAV files + choice.json) until one is chosen or they are
//! discarded.

use std::path::PathBuf;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use ttv_link::{LibraryEntry, TakeChoice, VoiceDef};

use crate::{Engine, Inner, Trouble, audio, langs, mix_levels, quality, save_voice};

#[derive(Serialize, Deserialize)]
struct Stored {
    choice: TakeChoice,
    def: VoiceDef,
    /// Keep a lossless copy of the chosen take.
    backup: bool,
}

impl Inner {
    fn takes_dir(&self, key: &str) -> PathBuf {
        self.dir.join("takes").join(key.replace(':', "_"))
    }

    /// Keeps the takes of one voice for the streamer's choice (replacing
    /// earlier ones of the same voice).
    pub(crate) fn store_takes(&self, choice: TakeChoice, def: &VoiceDef, wavs: &[&[u8]], backup: bool) -> Result<(), String> {
        let dir = self.takes_dir(&choice.key);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for (n, wav) in wavs.iter().enumerate() {
            std::fs::write(dir.join(format!("{n}.wav")), wav).map_err(|e| e.to_string())?;
        }
        let s = Stored { choice, def: def.clone(), backup };
        std::fs::write(dir.join("choice.json"), serde_json::to_vec(&s).unwrap_or_default()).map_err(|e| e.to_string())
    }

    fn stored(&self, key: &str) -> Option<Stored> {
        serde_json::from_slice(&std::fs::read(self.takes_dir(key).join("choice.json")).ok()?).ok()
    }
}

impl Engine {
    /// Voices with takes waiting for a choice, newest first.
    pub fn takes(&self) -> Vec<TakeChoice> {
        let Ok(rd) = std::fs::read_dir(self.inner.dir.join("takes")) else { return Vec::new() };
        let mut out: Vec<TakeChoice> = rd
            .flatten()
            .filter_map(|d| serde_json::from_slice::<Stored>(&std::fs::read(d.path().join("choice.json")).ok()?).ok())
            .map(|s| s.choice)
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.created));
        out
    }

    /// One take as WAV, to listen to.
    pub fn take_wav(&self, key: &str, n: u32) -> Option<Vec<u8>> {
        std::fs::read(self.inner.takes_dir(key).join(format!("{n}.wav"))).ok()
    }

    /// Keeps the old voice: the takes are thrown away.
    pub fn discard_takes(&self, key: &str) -> bool {
        std::fs::remove_dir_all(self.inner.takes_dir(key)).is_ok()
    }

    /// Makes take `n` the voice: saves and primes it in VCP, replaces the
    /// old voice (deleted in VCP too) and throws the other takes away.
    pub async fn choose_take(&self, key: &str, n: u32) -> Result<LibraryEntry, String> {
        let inner = &self.inner;
        let s = inner.stored(key).ok_or("These takes don't exist (any more).")?;
        let info = s.choice.takes.iter().find(|t| t.n == n).ok_or("This take doesn't exist.")?.clone();
        let wav = self.take_wav(key, n).ok_or("This take doesn't exist.")?;
        let lang = langs::get(&s.choice.lang).ok_or("unknown language")?;
        let vcp = inner.vcp.read().clone().ok_or("Voice Creator Pro is not connected")?;
        let c = &s.choice;
        let voice_name = format!("TTV {} {} {:03} {}", c.preset_id, lang.code, c.index + 1, s.def.name);
        let save = serde_json::json!({
            "name": voice_name,
            "audio_base64": base64::engine::general_purpose::STANDARD.encode(&wav),
            "transcript": s.def.description,
            "ref_text": lang.text,
            "voice_type": "design",
            "transcript_manual": true,
        });
        let (voice, spoken) = save_voice(&vcp, &save, lang).await.map_err(|t| match t {
            Trouble::Down(e) | Trouble::Failed(e) => e,
        })?;
        let analysis = audio::decode(&wav).map(|(x, r)| quality::analyze(&x, r)).unwrap_or_default();
        let mut entry = LibraryEntry {
            key: key.to_string(),
            def_hash: s.def.hash(),
            lang: lang.code.to_string(),
            preset_id: c.preset_id.clone(),
            index: c.index,
            name: s.def.name.clone(),
            voice_id: voice.id.clone(),
            voice_name,
            created: chrono::Utc::now().timestamp(),
            duration: info.duration,
            wer: info.wer,
            gain: audio::gain_from_db(mix_levels(analysis.level, spoken.analysis.level).unwrap_or(-20.0)),
            backup: false,
            level: analysis.level,
            quality: None,
        };
        entry.backup = s.backup && inner.backup(key, &wav);
        let old = inner.library.get(key);
        inner.change(vec![entry.clone()], &[]);
        if let Some(o) = old
            && o.voice_id != entry.voice_id
            && let Err(e) = vcp.delete_voice(&o.voice_id).await
        {
            log::warn!("deleting the old voice of {key}: {e}");
        }
        self.discard_takes(key);
        log::info!("{key}: take {n} chosen ({})", entry.voice_id);
        Ok(entry)
    }
}
