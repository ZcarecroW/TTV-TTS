//! The system's own voices: the fallback when the voice host can't speak
//! (VCP stopped, the PC unreachable – optional, per profile). They go
//! through the app's audio output like every other voice.
//!
//! - Windows: the modern voices (WinRT SpeechSynthesizer, "OneCore": what
//!   Settings → Speech installs) and classic SAPI5 voices (from other makers,
//!   e.g. RHVoice); both render to WAV in memory. More voices: TTV-TTS can
//!   install Microsoft's voice pack of a language (admin rights, one UAC
//!   prompt) or open the Speech settings.
//! - macOS: `say` writes a WAV file; more voices in System Settings →
//!   Accessibility → Spoken Content.
//! - Linux: `espeak-ng --stdout` (more voices: its language and mbrola packages).
//!
//! Which voice reads a language, how fast and how high, is set in the
//! settings (one choice for all profiles); a message's language picks it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One installed system voice.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OsVoice {
    /// "onecore:<token>", "sapi:<token>", "say:<name>", "espeak:<name>".
    pub id: String,
    pub name: String,
    /// BCP 47 ("de-DE"); may be empty.
    pub language: String,
    /// "female", "male" or "".
    pub gender: String,
}

/// The settings of the system voice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct OsVoiceConfig {
    /// Language code ("de") -> voice id; missing: the first voice of the language.
    pub voices: BTreeMap<String, String>,
    /// 1 = normal, 0.5 … 2.
    pub rate: f32,
    /// 1 = normal, 0.5 … 1.5 (not every engine can).
    pub pitch: f32,
}

impl Default for OsVoiceConfig {
    fn default() -> Self {
        Self { voices: BTreeMap::new(), rate: 1.0, pitch: 1.0 }
    }
}

impl OsVoiceConfig {
    pub fn normalize(&mut self) {
        self.rate = if self.rate.is_finite() { self.rate.clamp(0.5, 2.0) } else { 1.0 };
        self.pitch = if self.pitch.is_finite() { self.pitch.clamp(0.5, 1.5) } else { 1.0 };
    }
}

/// Every installed voice (takes a few milliseconds).
pub fn voices() -> Vec<OsVoice> {
    let mut v = imp::voices();
    v.sort_by(|a, b| a.language.cmp(&b.language).then(a.name.cmp(&b.name)));
    v
}

/// The voice for `lang` ("de"): the chosen one if it's still there, else the
/// first of that language, else none (the engine's default).
pub fn pick(all: &[OsVoice], cfg: &OsVoiceConfig, lang: &str) -> Option<OsVoice> {
    let lang = lang.to_ascii_lowercase();
    if let Some(id) = cfg.voices.get(&lang)
        && let Some(v) = all.iter().find(|v| v.id == *id)
    {
        return Some(v.clone());
    }
    all.iter().find(|v| v.language.to_ascii_lowercase().split(['-', '_']).next() == Some(lang.as_str())).cloned()
}

/// Speaks `text` in language `lang` ("de", "en", …) with the configured voice.
/// Returns mono samples at their rate, brought to the loudness of the
/// computed voices.
pub async fn speak(text: &str, lang: &str, cfg: &OsVoiceConfig) -> Result<(Vec<f32>, u32), String> {
    let (text, lang, cfg) = (text.to_string(), lang.to_string(), cfg.clone());
    let wav = tokio::task::spawn_blocking(move || {
        let all = voices();
        let voice = pick(&all, &cfg, &lang);
        imp::synth(&text, voice.as_ref(), cfg.rate, cfg.pitch)
    })
    .await
    .map_err(|e| e.to_string())??;
    let (mut samples, rate) = ttv_engine::decode_wav(&wav).ok_or("the system voice gave unreadable audio")?;
    normalize(&mut samples);
    Ok((samples, rate))
}

/// Speaks with one particular voice (to try it).
pub async fn speak_with(text: &str, voice_id: &str, cfg: &OsVoiceConfig) -> Result<(Vec<f32>, u32), String> {
    let (text, id, cfg) = (text.to_string(), voice_id.to_string(), cfg.clone());
    let wav = tokio::task::spawn_blocking(move || {
        let v = voices().into_iter().find(|v| v.id == id).ok_or("this voice isn't installed (any more)")?;
        imp::synth(&text, Some(&v), cfg.rate, cfg.pitch)
    })
    .await
    .map_err(|e| e.to_string())??;
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

/// Languages TTV-TTS reads and the Windows voice pack ("Features on Demand")
/// for each.
pub const PACKS: [(&str, &str); 10] = [("en", "en-US"), ("de", "de-DE"), ("ru", "ru-RU"), ("fr", "fr-FR"), ("es", "es-ES"), ("it", "it-IT"), ("pt", "pt-BR"), ("ja", "ja-JP"), ("zh", "zh-CN"), ("ko", "ko-KR")];

/// Installs Microsoft's voices of a language (Windows; admin rights, one
/// UAC prompt). Takes a minute or two (a download).
pub async fn install(locale: &str) -> Result<(), String> {
    if !PACKS.iter().any(|(_, l)| *l == locale) {
        return Err(format!("no voice pack for {locale}"));
    }
    let locale = locale.to_string();
    tokio::task::spawn_blocking(move || imp::install(&locale)).await.map_err(|e| e.to_string())?
}

/// Opens the system's own voice settings.
pub fn open_settings() -> Result<(), String> {
    imp::open_settings()
}

#[cfg(windows)]
mod imp {
    use super::OsVoice;
    use windows::Media::SpeechSynthesis::{SpeechSynthesizer, VoiceGender};
    use windows::Storage::Streams::DataReader;
    use windows::Win32::Media::Speech::{ISpDataKey, ISpObjectToken, ISpObjectTokenCategory, ISpStream, ISpVoice, SPCAT_VOICES, SPF_DEFAULT, SPF_IS_XML, SpObjectTokenCategory, SpStream, SpVoice};
    use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, IStream, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET};
    use windows::core::{GUID, HSTRING, PCWSTR, PWSTR};

    /// SAPI's format id for a WAVEFORMATEX stream.
    const SPDFID_WAVEFORMATEX: GUID = GUID::from_u128(0xc31adbae_527f_4ff5_a230_f62bb61ff70c);

    /// COM on this (blocking-pool) thread. It stays initialized: shutting it
    /// down between calls breaks the speech objects Windows caches for the
    /// process (access violation on the next call).
    struct Com;
    impl Com {
        fn new() -> Self {
            thread_local!(static ON: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
            ON.with(|on| {
                if !on.get() {
                    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
                    on.set(true);
                }
            });
            Self
        }
    }

    fn e(e: windows::core::Error) -> String {
        format!("system voice: {}", e.message())
    }

    fn take(p: PWSTR) -> String {
        if p.is_null() {
            return String::new();
        }
        let s = unsafe { p.to_string() }.unwrap_or_default();
        unsafe { CoTaskMemFree(Some(p.0 as _)) };
        s
    }

    /// "409;9" (hex LCIDs) -> "en-US".
    fn lcid_name(list: &str) -> String {
        let Some(lcid) = list.split(';').next().and_then(|h| u32::from_str_radix(h.trim(), 16).ok()) else { return String::new() };
        let mut buf = [0u16; 85];
        let n = unsafe { windows::Win32::Globalization::LCIDToLocaleName(lcid, Some(&mut buf), 0) };
        if n > 1 { String::from_utf16_lossy(&buf[..n as usize - 1]) } else { String::new() }
    }

    pub fn voices() -> Vec<OsVoice> {
        let _com = Com::new();
        let mut out = Vec::new();
        if let Ok(all) = SpeechSynthesizer::AllVoices() {
            for v in all {
                let gender = match v.Gender() {
                    Ok(g) if g == VoiceGender::Female => "female",
                    Ok(_) => "male",
                    Err(_) => "",
                };
                out.push(OsVoice {
                    id: format!("onecore:{}", v.Id().map(|s| s.to_string()).unwrap_or_default()),
                    name: v.DisplayName().map(|s| s.to_string()).unwrap_or_default(),
                    language: v.Language().map(|s| s.to_string()).unwrap_or_default(),
                    gender: gender.into(),
                });
            }
        }
        // classic SAPI5 voices; Microsoft's own "… Desktop" ones are the same
        // voices as above
        for (id, name, lang, gender) in sapi_tokens() {
            let base = name.trim_end_matches(" Desktop");
            if name.ends_with(" Desktop") && out.iter().any(|v| v.name == base) {
                continue;
            }
            out.push(OsVoice { id: format!("sapi:{id}"), name, language: lang, gender });
        }
        out
    }

    pub(super) fn sapi_tokens() -> Vec<(String, String, String, String)> {
        let r = (|| -> windows::core::Result<Vec<(String, String, String, String)>> {
            let cat: ISpObjectTokenCategory = unsafe { CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL)? };
            unsafe { cat.SetId(SPCAT_VOICES, false)? };
            let en = unsafe { cat.EnumTokens(PCWSTR::null(), PCWSTR::null())? };
            let mut n = 0u32;
            unsafe { en.GetCount(&mut n)? };
            let mut out = Vec::new();
            for i in 0..n {
                let Ok(t) = (unsafe { en.Item(i) }) else { continue };
                let id = take(unsafe { t.GetId() }.unwrap_or(PWSTR::null()));
                let attrs: Option<ISpDataKey> = unsafe { t.OpenKey(windows::core::w!("Attributes")) }.ok();
                let get = |k: PCWSTR| attrs.as_ref().map(|a| take(unsafe { a.GetStringValue(k) }.unwrap_or(PWSTR::null()))).unwrap_or_default();
                // the short name ("Microsoft David Desktop"); the default value adds the language
                let name = Some(get(windows::core::w!("Name"))).filter(|n| !n.is_empty()).unwrap_or_else(|| take(unsafe { t.GetStringValue(PCWSTR::null()) }.unwrap_or(PWSTR::null())));
                let lang = lcid_name(&get(windows::core::w!("Language")));
                let gender = get(windows::core::w!("Gender")).to_ascii_lowercase();
                out.push((id, name, lang, gender));
            }
            Ok(out)
        })();
        r.unwrap_or_default()
    }

    pub fn synth(text: &str, voice: Option<&OsVoice>, rate: f32, pitch: f32) -> Result<Vec<u8>, String> {
        let _com = Com::new();
        match voice.map(|v| v.id.as_str()) {
            Some(id) if id.starts_with("sapi:") => sapi(text, &id[5..], rate, pitch),
            other => onecore(text, other.and_then(|i| i.strip_prefix("onecore:")), rate, pitch),
        }
    }

    fn onecore(text: &str, id: Option<&str>, rate: f32, pitch: f32) -> Result<Vec<u8>, String> {
        let synth = SpeechSynthesizer::new().map_err(e)?;
        if let Some(id) = id
            && let Ok(all) = SpeechSynthesizer::AllVoices()
            && let Some(v) = all.into_iter().find(|v| v.Id().is_ok_and(|x| x == id))
        {
            synth.SetVoice(&v).map_err(e)?;
        }
        if let Ok(o) = synth.Options() {
            let _ = o.SetSpeakingRate(rate as f64);
            let _ = o.SetAudioPitch(pitch as f64);
        }
        let stream = synth.SynthesizeTextToStreamAsync(&HSTRING::from(text)).map_err(e)?.join().map_err(e)?;
        let size = stream.Size().map_err(e)? as u32;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(e)?).map_err(e)?;
        reader.LoadAsync(size).map_err(e)?.join().map_err(e)?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf).map_err(e)?;
        Ok(buf)
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    fn sapi(text: &str, token_id: &str, rate: f32, pitch: f32) -> Result<Vec<u8>, String> {
        unsafe {
            let cat: ISpObjectTokenCategory = CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL).map_err(e)?;
            cat.SetId(SPCAT_VOICES, false).map_err(e)?;
            let en = cat.EnumTokens(PCWSTR::null(), PCWSTR::null()).map_err(e)?;
            let mut n = 0u32;
            en.GetCount(&mut n).map_err(e)?;
            let mut token: Option<ISpObjectToken> = None;
            for i in 0..n {
                if let Ok(t) = en.Item(i)
                    && take(t.GetId().unwrap_or(PWSTR::null())) == token_id
                {
                    token = Some(t);
                    break;
                }
            }
            let token = token.ok_or("this voice isn't installed (any more)")?;
            let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_ALL).map_err(e)?;
            voice.SetVoice(&token).map_err(e)?;
            // 22.05 kHz, 16 bit, mono into memory
            let wf = windows::Win32::Media::Audio::WAVEFORMATEX { wFormatTag: 1, nChannels: 1, nSamplesPerSec: 22_050, nAvgBytesPerSec: 44_100, nBlockAlign: 2, wBitsPerSample: 16, cbSize: 0 };
            let mem: IStream = windows::Win32::UI::Shell::SHCreateMemStream(None).ok_or("no memory stream")?;
            let stream: ISpStream = CoCreateInstance(&SpStream, None, CLSCTX_ALL).map_err(e)?;
            stream.SetBaseStream(&mem, &SPDFID_WAVEFORMATEX, &wf).map_err(e)?;
            voice.SetOutput(&stream, false).map_err(e)?;
            // SAPI's rate is -10 … 10 (each step about 10 % faster or slower)
            let r = ((rate.ln() / 1.1f32.ln()).round() as i32).clamp(-10, 10);
            voice.SetRate(r).map_err(e)?;
            let p = ((pitch - 1.0) * 20.0).round() as i32;
            let xml = format!("<pitch absmiddle=\"{}\">{}</pitch>", p.clamp(-10, 10), xml_escape(text));
            let w = HSTRING::from(xml);
            voice.Speak(PCWSTR(w.as_ptr()), (SPF_DEFAULT.0 | SPF_IS_XML.0) as u32, None).map_err(e)?;
            voice.WaitUntilDone(60_000).map_err(e)?;
            // raw PCM out of the memory stream -> WAV
            let mut st = STATSTG::default();
            mem.Stat(&mut st, STATFLAG_NONAME).map_err(e)?;
            let len = st.cbSize as usize;
            mem.Seek(0, STREAM_SEEK_SET, None).map_err(e)?;
            let mut pcm = vec![0u8; len];
            let mut read = 0u32;
            let _ = mem.Read(pcm.as_mut_ptr() as _, len as u32, Some(&mut read));
            pcm.truncate(read as usize);
            Ok(wav16(&pcm, 22_050))
        }
    }

    /// A WAV header for 16-bit mono PCM.
    fn wav16(pcm: &[u8], rate: u32) -> Vec<u8> {
        let mut w = Vec::with_capacity(44 + pcm.len());
        w.extend(b"RIFF");
        w.extend(((36 + pcm.len()) as u32).to_le_bytes());
        w.extend(b"WAVEfmt ");
        w.extend(16u32.to_le_bytes());
        w.extend(1u16.to_le_bytes());
        w.extend(1u16.to_le_bytes());
        w.extend(rate.to_le_bytes());
        w.extend((rate * 2).to_le_bytes());
        w.extend(2u16.to_le_bytes());
        w.extend(16u16.to_le_bytes());
        w.extend(b"data");
        w.extend((pcm.len() as u32).to_le_bytes());
        w.extend(pcm);
        w
    }

    /// Runs PowerShell with admin rights (UAC prompt) and waits for it.
    pub fn install(locale: &str) -> Result<(), String> {
        use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
        use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
        use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
        // the voices of the language (TextToSpeech) – no display language, no keyboard
        let script = format!("$ErrorActionPreference='Stop'; Add-WindowsCapability -Online -Name 'Language.TextToSpeech~~~{locale}~0.0.1.0' | Out-Null");
        let encoded = {
            use base64::Engine as _;
            let utf16: Vec<u8> = script.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
            base64::engine::general_purpose::STANDARD.encode(utf16)
        };
        let params = HSTRING::from(format!("-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -EncodedCommand {encoded}"));
        let (verb, file) = (HSTRING::from("runas"), HSTRING::from("powershell.exe"));
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            nShow: SW_HIDE.0,
            ..Default::default()
        };
        // SAFETY: the strings outlive the call; the process handle is closed below.
        unsafe {
            if let Err(err) = ShellExecuteExW(&mut info) {
                return Err(if err.code().0 as u32 == 0x800704C7 { "cancelled".into() } else { format!("could not get admin rights ({})", err.message()) });
            }
            if info.hProcess.is_invalid() {
                return Ok(());
            }
            let wait = WaitForSingleObject(info.hProcess, INFINITE);
            let mut code = 1u32;
            let _ = GetExitCodeProcess(info.hProcess, &mut code);
            let _ = CloseHandle(info.hProcess);
            if wait != WAIT_OBJECT_0 || code != 0 {
                return Err(format!("Windows couldn't install the voices (exit code {code}). Try Settings → Time & language → Speech → Add voices."));
            }
        }
        Ok(())
    }

    pub fn open_settings() -> Result<(), String> {
        std::process::Command::new("explorer.exe").arg("ms-settings:speech").spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::OsVoice;
    use std::process::Command;

    /// `say -v ?`: "Anna   de_DE   # Hallo, ich heiße Anna."
    pub fn voices() -> Vec<OsVoice> {
        let Ok(o) = Command::new("say").args(["-v", "?"]).output() else { return Vec::new() };
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter_map(|l| {
                let head = l.split('#').next()?.trim_end();
                let (name, locale) = head.rsplit_once(char::is_whitespace)?;
                let name = name.trim();
                (!name.is_empty()).then(|| OsVoice { id: format!("say:{name}"), name: name.to_string(), language: locale.replace('_', "-"), gender: String::new() })
            })
            .collect()
    }

    pub fn synth(text: &str, voice: Option<&OsVoice>, rate: f32, _pitch: f32) -> Result<Vec<u8>, String> {
        let path = std::env::temp_dir().join(format!("ttv-tts-say-{}-{}.wav", std::process::id(), rand::random::<u32>()));
        let mut cmd = Command::new("say");
        cmd.args(["--file-format=WAVE", "--data-format=LEI16@22050", "-o"]).arg(&path);
        if let Some(v) = voice.and_then(|v| v.id.strip_prefix("say:")) {
            cmd.args(["-v", v]);
        }
        // about 180 words per minute is normal
        cmd.args(["-r", &((180.0 * rate).round() as u32).to_string()]);
        let ok = cmd.arg(text).status().map_err(|e| format!("system voice (say): {e}"))?.success();
        let wav = std::fs::read(&path).map_err(|e| format!("system voice: {e}"));
        let _ = std::fs::remove_file(&path);
        if !ok {
            return Err("system voice (say) failed".into());
        }
        wav
    }

    pub fn install(_locale: &str) -> Result<(), String> {
        Err("On macOS, add voices in System Settings → Accessibility → Spoken Content → System voice → Manage Voices.".into())
    }

    pub fn open_settings() -> Result<(), String> {
        Command::new("open").arg("x-apple.systempreferences:com.apple.preference.universalaccess").spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::OsVoice;
    use std::process::Command;

    /// `espeak-ng --voices`: "Pty Language Age/Gender VoiceName File Other Langs".
    pub fn voices() -> Vec<OsVoice> {
        let Ok(o) = Command::new("espeak-ng").arg("--voices").output() else { return Vec::new() };
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .skip(1)
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                let (lang, ag, name, file) = (f.get(1)?, f.get(2)?, f.get(3)?, f.get(4)?);
                let gender = if ag.ends_with('F') { "female" } else if ag.ends_with('M') { "male" } else { "" };
                Some(OsVoice { id: format!("espeak:{file}"), name: name.replace('_', " "), language: lang.to_string(), gender: gender.into() })
            })
            .collect()
    }

    pub fn synth(text: &str, voice: Option<&OsVoice>, rate: f32, pitch: f32) -> Result<Vec<u8>, String> {
        let v = voice.and_then(|v| v.id.strip_prefix("espeak:")).unwrap_or("en");
        let out = Command::new("espeak-ng")
            .args(["-v", v, "-s", &((175.0 * rate).round() as u32).to_string(), "-p", &((50.0 * pitch).round().clamp(0.0, 99.0) as u32).to_string(), "--stdout", text])
            .output()
            .map_err(|_| "No system voice: install espeak-ng for the fallback voice.".to_string())?;
        if !out.status.success() || out.stdout.is_empty() {
            return Err(format!("espeak-ng: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(out.stdout)
    }

    pub fn install(_locale: &str) -> Result<(), String> {
        Err("On Linux, install espeak-ng (and its mbrola voices) with your package manager, e.g. sudo apt install espeak-ng.".into())
    }

    pub fn open_settings() -> Result<(), String> {
        Err("Linux has no common voice settings: see the documentation.".into())
    }
}

#[cfg(not(any(windows, unix)))]
mod imp {
    use super::OsVoice;
    pub fn voices() -> Vec<OsVoice> {
        Vec::new()
    }
    pub fn synth(_text: &str, _voice: Option<&OsVoice>, _rate: f32, _pitch: f32) -> Result<Vec<u8>, String> {
        Err("no system voice on this platform".into())
    }
    pub fn install(_locale: &str) -> Result<(), String> {
        Err("not on this platform".into())
    }
    pub fn open_settings() -> Result<(), String> {
        Err("not on this platform".into())
    }
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

    #[test]
    fn picks_the_chosen_voice_or_one_of_the_language() {
        let v = |id: &str, lang: &str| OsVoice { id: id.into(), name: id.into(), language: lang.into(), gender: String::new() };
        let all = vec![v("a", "en-US"), v("b", "de-DE"), v("c", "de-AT")];
        let mut cfg = OsVoiceConfig::default();
        assert_eq!(pick(&all, &cfg, "de").unwrap().id, "b");
        cfg.voices.insert("de".into(), "c".into());
        assert_eq!(pick(&all, &cfg, "de").unwrap().id, "c");
        cfg.voices.insert("de".into(), "gone".into());
        assert_eq!(pick(&all, &cfg, "DE").unwrap().id, "b", "an uninstalled choice falls back");
        assert!(pick(&all, &cfg, "ja").is_none());
        let mut c = OsVoiceConfig { rate: 9.0, pitch: f32::NAN, ..Default::default() };
        c.normalize();
        assert_eq!((c.rate, c.pitch), (2.0, 1.0));
    }

    /// Live: lists the system's voices and speaks with each kind (no sound played).
    #[tokio::test]
    #[ignore]
    async fn system_voices_live() {
        let all = voices();
        for v in &all {
            println!("{} | {} | {} | {}", v.id, v.name, v.language, v.gender);
        }
        assert!(!all.is_empty());
        let mut cfg = OsVoiceConfig::default();
        for (text, lang) in [("Das ist die Ersatzstimme des Systems.", "de"), ("This is the system's fallback voice.", "en")] {
            let t0 = std::time::Instant::now();
            let (s, rate) = speak(text, lang, &cfg).await.unwrap();
            println!("{lang}: {:.1} s of audio at {rate} Hz in {} ms", s.len() as f32 / rate as f32, t0.elapsed().as_millis());
            assert!(s.len() as f32 / rate as f32 > 1.0);
        }
        // every voice, normal and faster/higher
        for v in all.iter().take(8) {
            let (s, rate) = speak_with("One, two, three.", &v.id, &cfg).await.unwrap_or_else(|e| panic!("{}: {e}", v.id));
            let normal = s.len() as f32 / rate as f32;
            cfg.rate = 1.6;
            cfg.pitch = 1.3;
            let (s, rate) = speak_with("One, two, three.", &v.id, &cfg).await.unwrap();
            let fast = s.len() as f32 / rate as f32;
            cfg.rate = 1.0;
            cfg.pitch = 1.0;
            println!("{}: {normal:.2} s, faster {fast:.2} s", v.name);
            assert!(fast < normal * 0.9, "{}: rate has an effect ({normal} -> {fast})", v.name);
        }
        // classic SAPI5 voices too (also the ones hidden as duplicates)
        #[cfg(windows)]
        if let Some((id, name, lang, _)) = imp::sapi_tokens().into_iter().next() {
            let v = OsVoice { id: format!("sapi:{id}"), name: name.clone(), language: lang, gender: String::new() };
            let len = |rate: f32, pitch: f32| {
                let w = imp::synth("One, two, three, four & five <six>.", Some(&v), rate, pitch).unwrap();
                let (s, r) = ttv_engine::decode_wav(&w).unwrap();
                s.len() as f32 / r as f32
            };
            let (normal, fast, high) = (len(1.0, 1.0), len(1.6, 1.0), len(1.0, 1.4));
            println!("SAPI {name}: {normal:.2} s, faster {fast:.2} s, higher {high:.2} s");
            assert!(normal > 1.0 && fast < normal * 0.9);
        }
    }
}

