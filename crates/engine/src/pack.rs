//! Voice packs (`.ttvpack`): a zip with `manifest.json` and one lossless
//! sample (FLAC) per voice. Packs come from friends or downloads, so reading
//! one checks everything and limits sizes.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use ttv_link::{PackManifest, PackPreset, PackVoice};

pub const FORMAT: &str = "ttv-tts-pack";
pub const VERSION: u32 = 1;
pub const EXTENSION: &str = "ttvpack";

/// Limits for packs from elsewhere.
const MAX_VOICES: usize = 50_000;
const MAX_MANIFEST: u64 = 64 << 20;
/// A 15 s mono sample is ~0.5 MB as FLAC; anything far beyond isn't a voice.
const MAX_SAMPLE: u64 = 8 << 20;
const MAX_SECONDS: u64 = 30;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Reads and checks the manifest of a pack.
pub fn read_manifest(path: &Path) -> Result<PackManifest, String> {
    let mut zip = zip::ZipArchive::new(File::open(path).map_err(err)?).map_err(|e| format!("not a voice pack ({e})"))?;
    let entry = zip.by_name("manifest.json").map_err(|_| "not a voice pack (no manifest.json)".to_string())?;
    if entry.size() > MAX_MANIFEST {
        return Err("the pack's manifest is too large".into());
    }
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.take(MAX_MANIFEST).read_to_end(&mut buf).map_err(err)?;
    let m: PackManifest = serde_json::from_slice(&buf).map_err(|e| format!("the pack's manifest is broken ({e})"))?;
    check(&m)?;
    Ok(m)
}

fn check(m: &PackManifest) -> Result<(), String> {
    if m.format != FORMAT {
        return Err("not a TTV-TTS voice pack".into());
    }
    if m.version == 0 || m.version > VERSION {
        return Err(format!("this pack needs a newer TTV-TTS (pack format {})", m.version));
    }
    if m.voices.len() > MAX_VOICES {
        return Err("the pack has too many voices".into());
    }
    for v in &m.voices {
        if crate::langs::get(&v.lang).is_none() {
            return Err(format!("the pack has a voice in an unknown language ({})", v.lang));
        }
        if v.key != format!("{}:{}", v.def_hash, v.lang) || !v.file.starts_with("voices/") || v.file.contains("..") {
            return Err(format!("the pack's entry for {} is broken", v.name));
        }
    }
    Ok(())
}

/// An open pack for reading samples.
pub struct PackReader {
    zip: zip::ZipArchive<File>,
}

impl PackReader {
    pub fn open(path: &Path) -> Result<Self, String> {
        Ok(Self { zip: zip::ZipArchive::new(File::open(path).map_err(err)?).map_err(err)? })
    }

    /// The FLAC of a voice and its decoded mono samples.
    pub fn sample(&mut self, file: &str) -> Result<(Vec<u8>, Vec<f32>, u32), String> {
        let entry = self.zip.by_name(file).map_err(|_| format!("{file} is missing in the pack"))?;
        if entry.size() > MAX_SAMPLE {
            return Err(format!("{file} is too large"));
        }
        let mut flac = Vec::with_capacity(entry.size() as usize);
        entry.take(MAX_SAMPLE).read_to_end(&mut flac).map_err(err)?;
        let (samples, rate) = decode_flac(&flac)?;
        Ok((flac, samples, rate))
    }
}

/// Mono samples of a FLAC file (any bit depth, channels mixed down).
pub fn decode_flac(flac: &[u8]) -> Result<(Vec<f32>, u32), String> {
    let mut reader = claxon::FlacReader::new(std::io::Cursor::new(flac)).map_err(|e| format!("broken sample ({e})"))?;
    let info = reader.streaminfo();
    let (rate, channels, bits) = (info.sample_rate, info.channels.max(1) as usize, info.bits_per_sample);
    if rate == 0 || !(8..=32).contains(&bits) || info.samples.is_some_and(|n| n / rate as u64 > MAX_SECONDS) {
        return Err("the sample isn't a voice sample".into());
    }
    let scale = 1.0 / (1u64 << (bits - 1)) as f32;
    let limit = MAX_SECONDS as usize * rate as usize * channels;
    let mut out = Vec::with_capacity(info.samples.unwrap_or(0).min(limit as u64) as usize);
    let (mut acc, mut n) = (0f32, 0usize);
    for (i, s) in reader.samples().enumerate() {
        if i >= limit {
            return Err("the sample is too long".into());
        }
        acc += s.map_err(|e| format!("broken sample ({e})"))? as f32 * scale;
        n += 1;
        if n == channels {
            out.push(acc / channels as f32);
            (acc, n) = (0.0, 0);
        }
    }
    Ok((out, rate))
}

/// Writes a pack: samples as they come, the manifest at the end.
pub struct PackWriter {
    zip: zip::ZipWriter<File>,
    manifest: PackManifest,
    path: PathBuf,
}

impl PackWriter {
    pub fn create(path: &Path, title: &str, app: &str, presets: Vec<PackPreset>) -> Result<Self, String> {
        let file = File::create(path).map_err(err)?;
        let manifest = PackManifest { format: FORMAT.into(), version: VERSION, created: chrono::Utc::now().timestamp(), app: app.into(), title: title.into(), presets, voices: Vec::new() };
        Ok(Self { zip: zip::ZipWriter::new(file), manifest, path: path.to_path_buf() })
    }

    /// FLAC is compressed already: stored as it is.
    fn options() -> zip::write::SimpleFileOptions {
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(false)
    }

    pub fn add(&mut self, mut voice: PackVoice, flac: &[u8]) -> Result<(), String> {
        voice.file = format!("voices/{}.flac", voice.key.replace(':', "_"));
        self.zip.start_file(voice.file.as_str(), Self::options()).map_err(err)?;
        self.zip.write_all(flac).map_err(err)?;
        self.manifest.voices.push(voice);
        Ok(())
    }

    pub fn voices(&self) -> usize {
        self.manifest.voices.len()
    }

    pub fn finish(mut self) -> Result<PathBuf, String> {
        check(&self.manifest)?;
        self.zip.start_file("manifest.json", Self::options()).map_err(err)?;
        self.zip.write_all(&serde_json::to_vec_pretty(&self.manifest).map_err(err)?).map_err(err)?;
        self.zip.finish().map_err(err)?.sync_all().map_err(err)?;
        Ok(self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice(key: &str, lang: &str) -> PackVoice {
        PackVoice { key: format!("{key}:{lang}"), def_hash: key.into(), lang: lang.into(), name: "Mia".into(), description: "Bright, quick voice.".into(), ref_text: "Hallo".into(), gain: 1.1, ..Default::default() }
    }

    #[test]
    fn writes_and_reads_a_pack() {
        let dir = std::env::temp_dir().join(format!("ttv-pack-test-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.ttvpack");
        let samples: Vec<f32> = (0..24_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let flac = crate::audio::flac(&samples, 24_000).unwrap();
        let preset = PackPreset { id: "u-test".into(), name: "Test".into(), builtin: false, text: "#ttv-tts preset v1\nname: Test\n---\nMia|f|27|Bright, quick voice.".into() };
        let mut w = PackWriter::create(&path, "Test pack", "2.0.0", vec![preset]).unwrap();
        w.add(voice("v1", "de"), &flac).unwrap();
        w.add(voice("v1", "en"), &flac).unwrap();
        assert_eq!(w.voices(), 2);
        w.finish().unwrap();

        let m = read_manifest(&path).unwrap();
        assert_eq!((m.title.as_str(), m.voices.len(), m.presets.len()), ("Test pack", 2, 1));
        assert_eq!(m.voices[0].file, "voices/v1_de.flac");
        let mut r = PackReader::open(&path).unwrap();
        let (bytes, back, rate) = r.sample(&m.voices[1].file).unwrap();
        assert_eq!((bytes.len(), rate, back.len()), (flac.len(), 24_000, samples.len()));
        // 16-bit lossless: back within one step
        assert!(samples.iter().zip(&back).all(|(a, b)| (a - b).abs() < 1.0 / 16_000.0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_packs_that_arent() {
        let dir = std::env::temp_dir().join(format!("ttv-pack-bad-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let junk = dir.join("junk.ttvpack");
        std::fs::write(&junk, b"not a zip").unwrap();
        assert!(read_manifest(&junk).unwrap_err().contains("not a voice pack"));
        // a manifest pointing outside "voices/"
        let path = dir.join("evil.ttvpack");
        let mut w = PackWriter::create(&path, "x", "2.0.0", vec![]).unwrap();
        w.manifest.voices.push(PackVoice { file: "../../evil".into(), ..voice("v1", "de") });
        assert!(w.finish().is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
