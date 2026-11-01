//! Fallback voice: when the voice host can't speak (VCP stopped, the PC
//! unreachable), the operating system's own voice reads the message
//! (optional, per profile). It goes through the app's audio output like
//! every other voice, so it reaches the chosen device.
//!
//! - Windows: WinRT SpeechSynthesizer → WAV in memory, a voice of the
//!   message's language if one is installed.
//! - macOS: `say` writes a WAV file.
//! - Linux: `espeak-ng --stdout`.

/// Speaks `text` in language `lang` ("de", "en", …). Returns mono samples
/// at their rate, brought to the loudness of the computed voices.
pub async fn speak(text: &str, lang: &str) -> Result<(Vec<f32>, u32), String> {
    let (text, lang) = (text.to_string(), lang.to_string());
    let wav = tokio::task::spawn_blocking(move || synth(&text, &lang)).await.map_err(|e| e.to_string())??;
    let (mut samples, rate) = ttv_engine::decode_wav(&wav).ok_or("the system voice gave unreadable audio")?;
    normalize(&mut samples);
    Ok((samples, rate))
}

/// To about −20 dBFS speech loudness (like the computed voices), never
/// above full scale.
fn normalize(x: &mut [f32]) {
    let (mut sum, mut n) = (0f64, 0usize);
    for s in x.iter().filter(|s| s.abs() > 0.01) {
        sum += (*s as f64).powi(2);
        n += 1;
    }
    if n < 500 {
        return;
    }
    let rms = (sum / n as f64).sqrt() as f32;
    let peak = x.iter().fold(0f32, |m, s| m.max(s.abs()));
    let g = (0.1 / rms).min(0.95 / peak.max(1e-6));
    x.iter_mut().for_each(|s| *s *= g);
}

#[cfg(windows)]
fn synth(text: &str, lang: &str) -> Result<Vec<u8>, String> {
    use windows::Media::SpeechSynthesis::SpeechSynthesizer;
    use windows::Storage::Streams::DataReader;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    use windows::core::HSTRING;

    let e = |e: windows::core::Error| format!("system voice: {}", e.message());
    // WinRT needs COM on this (blocking-pool) thread
    let inited = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let result = (|| {
        let synth = SpeechSynthesizer::new().map_err(e)?;
        if let Ok(voices) = SpeechSynthesizer::AllVoices() {
            let want = lang.to_ascii_lowercase();
            if let Some(v) = voices.into_iter().find(|v| v.Language().is_ok_and(|l| l.to_string().to_ascii_lowercase().starts_with(&want))) {
                synth.SetVoice(&v).map_err(e)?;
            }
        }
        let stream = synth.SynthesizeTextToStreamAsync(&HSTRING::from(text)).map_err(e)?.join().map_err(e)?;
        let size = stream.Size().map_err(e)? as u32;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(e)?).map_err(e)?;
        reader.LoadAsync(size).map_err(e)?.join().map_err(e)?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf).map_err(e)?;
        Ok(buf)
    })();
    if inited {
        unsafe { CoUninitialize() };
    }
    result
}

#[cfg(target_os = "macos")]
fn synth(text: &str, lang: &str) -> Result<Vec<u8>, String> {
    use std::process::Command;
    // a voice of the language: `say -v ?` lists "Anna   de_DE   # Hallo …"
    let voice = Command::new("say").args(["-v", "?"]).output().ok().and_then(|o| {
        String::from_utf8_lossy(&o.stdout).lines().find_map(|l| {
            let mut parts = l.split_whitespace();
            let name = parts.next()?;
            let locale = l.split('#').next()?.split_whitespace().last()?;
            locale.to_ascii_lowercase().starts_with(lang).then(|| name.to_string())
        })
    });
    let path = std::env::temp_dir().join(format!("ttv-tts-say-{}.wav", std::process::id()));
    let mut cmd = Command::new("say");
    cmd.args(["--file-format=WAVE", "--data-format=LEI16@22050", "-o"]).arg(&path);
    if let Some(v) = voice {
        cmd.args(["-v", &v]);
    }
    let ok = cmd.arg(text).status().map_err(|e| format!("system voice (say): {e}"))?.success();
    let wav = std::fs::read(&path).map_err(|e| format!("system voice: {e}"));
    let _ = std::fs::remove_file(&path);
    if !ok {
        return Err("system voice (say) failed".into());
    }
    wav
}

#[cfg(all(unix, not(target_os = "macos")))]
fn synth(text: &str, lang: &str) -> Result<Vec<u8>, String> {
    let out = std::process::Command::new("espeak-ng")
        .args(["-v", lang, "--stdout", text])
        .output()
        .map_err(|_| "No system voice: install espeak-ng for the fallback voice.".to_string())?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(format!("espeak-ng: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(out.stdout)
}

#[cfg(not(any(windows, unix)))]
fn synth(_text: &str, _lang: &str) -> Result<Vec<u8>, String> {
    Err("no system voice on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brings_quiet_speech_up_and_keeps_peaks_below_full_scale() {
        let mut x: Vec<f32> = (0..24_000).map(|i| (i as f32 * 0.05).sin() * 0.02).collect();
        normalize(&mut x);
        let peak = x.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.1 && peak <= 0.95, "{peak}");
    }

    /// Live: the system voice speaks German and English (no sound played).
    #[tokio::test]
    #[ignore]
    async fn system_voice_live() {
        for (text, lang) in [("Das ist die Ersatzstimme des Systems.", "de"), ("This is the system's fallback voice.", "en")] {
            let t0 = std::time::Instant::now();
            let (s, rate) = speak(text, lang).await.unwrap();
            println!("{lang}: {:.1} s of audio at {rate} Hz in {} ms", s.len() as f32 / rate as f32, t0.elapsed().as_millis());
            assert!(s.len() as f32 / rate as f32 > 1.0);
        }
    }
}
