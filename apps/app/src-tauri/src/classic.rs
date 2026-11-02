//! "Classic": adopts voices made with TTV-TTS 1.x (named "TTS-de 07 Lena
//! (young woman)") as an own preset, so they keep working with presets and
//! can be completed for more languages – without computing them again.

use std::collections::BTreeMap;

use regex::Regex;
use serde::Serialize;
use ttv_link::{LibraryEntry, VoiceDef};

use crate::presets::Preset;

pub const ID: &str = "classic";

#[derive(Debug, Clone, Default, Serialize)]
pub struct Found {
    /// Roles (voice numbers) found.
    pub roles: u32,
    /// Voices found (roles × languages).
    pub voices: u32,
    pub languages: Vec<String>,
}

struct V1Voice<'a> {
    lang: String,
    role: u32,
    name: String,
    label: Option<String>,
    voice: &'a ttv_vcp::Voice,
}

fn matcher() -> Regex {
    // "TTS-de 07 Lena (young woman)", "TTS-en 3 Jake"
    Regex::new(r"^TTS-([a-z]{2}) (\d{1,3}) (.+?)(?: \(([^()]+)\))?$").expect("regex")
}

fn scan<'a>(voices: &'a [ttv_vcp::Voice]) -> Vec<V1Voice<'a>> {
    let re = matcher();
    voices
        .iter()
        .filter_map(|v| {
            let c = re.captures(v.name.trim())?;
            let lang = c[1].to_string();
            ttv_engine::langs::get(&lang)?;
            let role: u32 = c[2].parse().ok().filter(|r| *r > 0)?;
            Some(V1Voice { lang, role, name: c[3].trim().to_string(), label: c.get(4).map(|m| m.as_str().trim().to_string()), voice: v })
        })
        .collect()
}

pub fn detect(voices: &[ttv_vcp::Voice]) -> Found {
    let found = scan(voices);
    let mut roles: Vec<u32> = found.iter().map(|v| v.role).collect();
    roles.sort();
    roles.dedup();
    let mut languages: Vec<String> = found.iter().map(|v| v.lang.clone()).collect();
    languages.sort();
    languages.dedup();
    Found { roles: roles.len() as u32, voices: found.len() as u32, languages }
}

/// Removes the accent sentence the 1.x voice lab appended for the language.
fn neutral(description: &str) -> String {
    let mut d = description.trim().to_string();
    for l in ttv_engine::langs::LANGS {
        d = d.replace(l.accent, "");
    }
    d.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a VCP transcript is a voice description (what the 1.0 voice lab
/// stored) and not the words spoken in the sample (what later 1.x tools
/// stored).
fn is_description(text: &str) -> bool {
    const WORDS: &[&str] = &["voice", "tone", "pitch", "timbre", "speaks", "speaking", "baritone", "tenor", "alto", "soprano", "bass", "accent", "raspy", "husky"];
    // without the "(voice 5)" that tells apart roles with the same sentence
    let t = text.trim();
    let t = match t.rfind(" (voice ") {
        Some(i) if t.ends_with(')') => &t[..i],
        _ => t,
    };
    // whole words: "stone" is no "tone"
    t.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|w| WORDS.contains(&w))
}

/// Role words of the 1.x role voices (German labels like
/// "Trailer-Erzähler", "Raumschiff-KI"): English name, gender, typical age,
/// and how such a voice sounds.
const ROLE_NOUNS: &[(&str, &str, &str, u8, &str)] = &[
    ("erzählerin", "narrator", "f", 45, "expressive storyteller, clear and engaging, measured pace"),
    ("erzähler", "narrator", "m", 50, "expressive storyteller, clear and engaging, measured pace"),
    ("märchenerzählerin", "fairy-tale narrator", "f", 55, "warm, gentle storyteller with a soft, inviting tone"),
    ("sprecherin", "narrator", "f", 40, "clear, warm and professional, calm pace"),
    ("sprecher", "narrator", "m", 45, "clear, warm and professional, calm pace"),
    ("ki", "AI", "n", 35, "calm, precise and slightly synthetic"),
    ("detektiv", "detective", "m", 45, "dry, low and world-weary, unhurried"),
    ("detektivin", "detective", "f", 42, "dry, cool and observant, unhurried"),
    ("elfenkönigin", "elf queen", "f", 40, "ethereal, graceful and regal, soft and clear"),
    ("kriegerkönigin", "warrior queen", "f", 38, "commanding, proud and fierce"),
    ("königin", "queen", "f", 45, "regal, composed and commanding"),
    ("könig", "king", "m", 55, "regal, deep and commanding"),
    ("schurke", "villain", "m", 45, "smooth, charming and quietly menacing"),
    ("schurkin", "villain", "f", 40, "smooth, charming and quietly menacing"),
    ("heldin", "heroine", "f", 30, "brave, energetic and determined"),
    ("held", "hero", "m", 30, "brave, warm and determined"),
    ("revolverheld", "gunslinger", "m", 42, "low, gravelly drawl, slow and confident"),
    ("general", "general", "m", 58, "commanding, firm and authoritative"),
    ("radio", "late-night radio host", "n", 40, "warm, intimate and relaxed"),
    ("dj", "DJ", "m", 32, "warm, smooth and relaxed, easy flow"),
    ("orakel", "oracle", "n", 60, "ethereal, mysterious and serene, slow"),
    ("mentor", "mentor", "m", 65, "wise, warm and patient"),
    ("mentorin", "mentor", "f", 62, "wise, warm and patient"),
    ("hackerin", "hacker", "f", 26, "quick, witty and confident"),
    ("hacker", "hacker", "m", 26, "quick, witty and confident"),
    ("marine", "space marine", "m", 32, "tough, loud and commanding"),
    ("agentin", "secret agent", "f", 35, "cool, controlled and confident"),
    ("agent", "secret agent", "m", 38, "cool, controlled and confident"),
    ("schmuggler", "smuggler", "m", 38, "roguish, charming and laid-back"),
    ("kapitänin", "captain", "f", 45, "calm authority, clear and commanding"),
    ("kapitän", "captain", "m", 50, "calm authority, clear and commanding"),
    ("abenteurerin", "adventurer", "f", 28, "lively, bright and adventurous"),
    ("abenteurer", "adventurer", "m", 30, "lively, warm and adventurous"),
    ("imperator", "emperor", "m", 60, "imperious, deep and commanding"),
    ("söldner", "mercenary", "m", 40, "rough, gravelly and terse"),
    ("himmelsstimme", "heavenly being", "n", 35, "ethereal, luminous and serene"),
];

const ROLE_PREFIXES: &[(&str, &str)] = &[
    ("trailer", "movie-trailer"),
    ("noir", "film-noir"),
    ("doku", "documentary"),
    ("dark", "dark"),
    ("fantasy", "fantasy"),
    ("gothic", "gothic"),
    ("thriller", "thriller"),
    ("nacht", "late-night"),
    ("raumschiff", "spaceship"),
    ("gentleman", "gentleman"),
    ("action", "action"),
];

/// Gender, age and an English description from a 1.x role label.
fn from_label(label: &str) -> Option<(&'static str, u8, String)> {
    let parts: Vec<String> = label.split(['-', ' ']).filter(|p| !p.is_empty()).map(str::to_lowercase).collect();
    let noun = parts.last()?;
    let (_, en, gender, age, style) = ROLE_NOUNS.iter().find(|(de, ..)| de == noun)?;
    let adjectives: Vec<&str> = parts[..parts.len() - 1].iter().filter_map(|p| ROLE_PREFIXES.iter().find(|(de, _)| de == p).map(|(_, en)| *en)).collect();
    let who = if adjectives.is_empty() || en.contains(' ') { en.to_string() } else { format!("{} {en}", adjectives.join(" ")) };
    let article = if who.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U']) { "an" } else { "a" };
    Some((gender, *age, format!("Voice of {article} {who}: {style}.")))
}

fn guess_gender(text: &str) -> &'static str {
    let t = format!(" {} ", text.to_lowercase().replace(|c: char| !c.is_alphanumeric(), " "));
    let has = |words: &[&str]| words.iter().any(|w| t.contains(&format!(" {w} ")));
    if has(&["woman", "girl", "female", "lady", "grandmother", "mother", "she", "her"]) {
        "f"
    } else if has(&["man", "boy", "male", "guy", "grandfather", "father", "he", "his"]) {
        "m"
    } else {
        "n"
    }
}

fn guess_age(text: &str) -> u8 {
    let t = text.to_lowercase();
    let rules: &[(&str, u8)] = &[
        ("early twenties", 22),
        ("late twenties", 28),
        ("twenties", 25),
        ("early thirties", 32),
        ("thirties", 35),
        ("around forty", 40),
        ("forties", 45),
        ("fifties", 55),
        ("sixties", 65),
        ("seventy", 72),
        ("seventies", 74),
        ("eighty", 82),
        ("elderly", 72),
        ("teen", 16),
        ("child", 10),
        ("kid", 10),
        ("middle-aged", 45),
        ("young", 24),
        ("old", 70),
    ];
    if let Some(&(_, a)) = rules.iter().find(|(w, _)| t.contains(w)) {
        return a;
    }
    // "~40", "40 years"
    let digits: String = t.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u8>().ok().filter(|a| (5..=99).contains(a)).unwrap_or(35)
}

/// Builds the preset and the library entries for the 1.x voices.
pub fn build(voices: &[ttv_vcp::Voice]) -> Option<(Preset, Vec<LibraryEntry>)> {
    let found = scan(voices);
    if found.is_empty() {
        return None;
    }
    // One definition per role, from its English voice if there is one.
    let mut roles: BTreeMap<u32, &V1Voice> = BTreeMap::new();
    for v in &found {
        let better = match roles.get(&v.role) {
            None => true,
            Some(cur) => cur.lang != "en" && (v.lang == "en" || cur.voice.transcript.is_empty() && !v.voice.transcript.is_empty()),
        };
        if better {
            roles.insert(v.role, v);
        }
    }
    let mut defs = Vec::new();
    let mut index_of: BTreeMap<u32, usize> = BTreeMap::new();
    for (role, v) in &roles {
        let label = v.label.clone().unwrap_or_default();
        let name = if label.is_empty() { v.name.clone() } else { format!("{} ({label})", v.name) };
        index_of.insert(*role, defs.len());
        let stored = neutral(&v.voice.transcript);
        // The 1.0 voice lab kept the description; later voices keep the
        // spoken words – then the role label tells who the voice is.
        if is_description(&stored) && stored.chars().count() >= 20 {
            let basis = format!("{label} {stored}");
            defs.push(VoiceDef { name, gender: guess_gender(&basis).into(), age: guess_age(&basis), description: stored, fx: None });
        } else if let Some((gender, age, description)) = from_label(&label) {
            defs.push(VoiceDef { name, gender: gender.into(), age, description, fx: None });
        } else {
            let description = format!("A {} voice, natural and clear, friendly and easy to listen to.", if label.is_empty() { "warm" } else { label.as_str() });
            let basis = format!("{label} {description}");
            defs.push(VoiceDef { name, gender: guess_gender(&basis).into(), age: guess_age(&basis), description, fx: None });
        }
    }
    // Two roles with the same text would be one voice: keep them apart.
    let mut seen = std::collections::HashSet::new();
    for (i, d) in defs.iter_mut().enumerate() {
        if !seen.insert(d.description.to_lowercase()) {
            d.description = format!("{} (voice {})", d.description, i + 1);
            seen.insert(d.description.to_lowercase());
        }
    }
    let now = chrono::Utc::now().timestamp();
    let entries = found
        .iter()
        .map(|v| {
            let index = index_of[&v.role];
            let def = &defs[index];
            LibraryEntry {
                key: def.key(&v.lang),
                def_hash: def.hash(),
                lang: v.lang.clone(),
                preset_id: ID.into(),
                index: index as u32,
                name: def.name.clone(),
                voice_id: v.voice.id.clone(),
                voice_name: v.voice.name.clone(),
                created: now,
                duration: v.voice.duration_seconds,
                wer: None,
                gain: 1.0,
                backup: false,
                level: None,
                quality: None,
                ref_text: None,
            }
        })
        .collect();
    let n = defs.len();
    let preset = Preset {
        id: ID.into(),
        name: format!("Classic {n}"),
        description: "Your voices from TTV-TTS 1.x".into(),
        icon: "ClockRotate".into(),
        builtin: false,
        target: n as u32,
        defs,
    };
    Some((preset, entries))
}

/// Whether an adopted Classic preset was built by the old rules (spoken
/// words as descriptions, no genders) and should be rebuilt.
pub fn outdated(p: &Preset) -> bool {
    let n = p.defs.len().max(1);
    p.defs.iter().filter(|d| d.gender == "n" && !is_description(&d.description)).count() * 5 >= n * 4
}

/// Rebuilds an outdated Classic preset from the 1.x voices and moves its
/// library entries to the new definitions (same VCP voices, nothing is
/// generated or deleted). Returns how many entries moved.
pub fn migrate(old: &Preset, voices: &[ttv_vcp::Voice], library: &[LibraryEntry]) -> Option<(Preset, Vec<LibraryEntry>, Vec<String>)> {
    if !outdated(old) {
        return None;
    }
    let (mut preset, fresh) = build(voices)?;
    preset.name.clone_from(&old.name);
    let by_voice: std::collections::HashMap<&str, &LibraryEntry> = library.iter().filter(|e| e.preset_id == ID).map(|e| (e.voice_id.as_str(), e)).collect();
    let entries: Vec<LibraryEntry> = fresh
        .into_iter()
        .map(|e| match by_voice.get(e.voice_id.as_str()) {
            // keep what was measured; the quality report belonged to the old definition
            Some(o) => LibraryEntry { gain: o.gain, level: o.level, wer: o.wer, backup: o.backup, created: o.created, duration: o.duration, quality: None, ..e },
            None => e,
        })
        .collect();
    let keep: std::collections::HashSet<&str> = entries.iter().map(|e| e.key.as_str()).collect();
    let remove: Vec<String> = library.iter().filter(|e| e.preset_id == ID && !keep.contains(e.key.as_str())).map(|e| e.key.clone()).collect();
    Some((preset, entries, remove))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: &str, name: &str, transcript: &str) -> ttv_vcp::Voice {
        ttv_vcp::Voice { id: id.into(), name: name.into(), transcript: transcript.into(), ..Default::default() }
    }

    #[test]
    fn adopts_v1_voices_by_role() {
        let en = ttv_engine::langs::get("en").unwrap().accent;
        let de = ttv_engine::langs::get("de").unwrap().accent;
        let desc = "A young woman in her early twenties with a bright, friendly and cheerful voice, lively but clear.";
        let voices = vec![
            v("voice_1", "TTS-de 01 Lena (young woman)", &format!("{desc} {de}")),
            v("voice_2", "TTS-en 01 Emma (young woman)", &format!("{desc} {en}")),
            v("voice_3", "TTS-en 02 Jake (young man)", "A young man in his early twenties with a relaxed, casual voice."),
            v("voice_4", "My own voice", ""),
            v("voice_5", "TTS-xx 03 Nope", ""),
        ];
        let f = detect(&voices);
        assert_eq!((f.roles, f.voices), (2, 3));
        assert_eq!(f.languages, vec!["de", "en"]);
        let (p, entries) = build(&voices).unwrap();
        assert_eq!(p.defs.len(), 2);
        assert_eq!(p.defs[0].description, desc, "accent removed");
        assert_eq!((p.defs[0].gender.as_str(), p.defs[0].age), ("f", 22));
        assert_eq!((p.defs[1].gender.as_str(), p.defs[1].age), ("m", 22));
        assert_eq!(p.defs[0].name, "Emma (young woman)");
        assert_eq!(entries.len(), 3);
        let lena = entries.iter().find(|e| e.voice_id == "voice_1").unwrap();
        assert_eq!(lena.key, p.defs[0].key("de"));
        assert_eq!(lena.index, 0);
    }

    #[test]
    fn role_labels_give_descriptions_genders_and_ages() {
        // later 1.x voices keep the spoken words in the transcript
        let voices = vec![
            v("voice_1", "TTS-en 01 Marcus (Trailer-Erzähler)", "In a world on the edge of darkness, one single choice will decide everything."),
            v("voice_2", "TTS-de 01 Konrad (Trailer-Erzähler)", "In einer Welt am Rande der Dunkelheit entscheidet eine einzige Wahl alles."),
            v("voice_3", "TTS-en 02 Scarlett (Noir-Erzählerin)", "The rain hadn't stopped for three days."),
            v("voice_4", "TTS-en 04 Nova (Raumschiff-KI)", "All systems are online."),
            v("voice_5", "TTS-en 15 Cole (Space Marine)", "Hold the line."),
        ];
        let (p, entries) = build(&voices).unwrap();
        let d = |i: usize| (&p.defs[i].gender[..], p.defs[i].age, &p.defs[i].description[..]);
        assert_eq!(d(0), ("m", 50, "Voice of a movie-trailer narrator: expressive storyteller, clear and engaging, measured pace."));
        assert_eq!(d(1).0, "f");
        assert!(d(1).2.starts_with("Voice of a film-noir narrator"));
        assert_eq!(d(2), ("n", 35, "Voice of a spaceship AI: calm, precise and slightly synthetic."));
        assert_eq!(d(3).0, "m");
        assert!(d(3).2.starts_with("Voice of a space marine"));
        assert_eq!(entries.len(), 5);
        assert_eq!(p.defs[0].name, "Marcus (Trailer-Erzähler)");
    }

    #[test]
    fn migrates_an_old_adoption_without_touching_vcp() {
        let voices = vec![
            v("voice_1", "TTS-en 01 Marcus (Trailer-Erzähler)", "In a world on the edge of darkness, one single choice will decide everything."),
            v("voice_2", "TTS-en 02 Scarlett (Noir-Erzählerin)", "The rain hadn't stopped for three days."),
        ];
        // what the old rules made: spoken words as description, gender n
        let old_defs: Vec<VoiceDef> = voices.iter().map(|x| VoiceDef { name: x.name.clone(), gender: "n".into(), age: 35, description: x.transcript.clone(), fx: None }).collect();
        let old = Preset { id: ID.into(), name: "Classic 2".into(), description: String::new(), icon: "ClockRotate".into(), builtin: false, target: 2, defs: old_defs.clone() };
        let lib: Vec<LibraryEntry> = old_defs
            .iter()
            .zip(&voices)
            .map(|(d, x)| LibraryEntry { key: d.key("en"), def_hash: d.hash(), lang: "en".into(), preset_id: ID.into(), voice_id: x.id.clone(), gain: 1.3, level: Some(-22.0), ..Default::default() })
            .collect();
        assert!(outdated(&old));
        let (p, entries, remove) = migrate(&old, &voices, &lib).unwrap();
        assert!(!outdated(&p), "rebuilt");
        assert_eq!(remove.len(), 2, "old keys go");
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.gain == 1.3 && e.level == Some(-22.0) && e.quality.is_none()), "measurements stay");
        assert_eq!(entries[0].voice_id, "voice_1");
        assert!(migrate(&p, &voices, &entries).is_none(), "only once");
        assert!(!is_description("Long ago, legends were written in fire and stone."));
        assert!(!is_description("The rain hadn't stopped for three days. (voice 5)"), "the duplicate marker isn't a description");
        assert!(is_description("A warm, low voice with a calm tone."));
    }
}

#[cfg(test)]
mod real {
    /// `TTV_VOICES=.work/diag/voices.json cargo test -p ttv-tts --lib classic_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn classic_real() {
        #[derive(serde::Deserialize)]
        struct L {
            voices: Vec<ttv_vcp::Voice>,
        }
        let l: L = serde_json::from_slice(&std::fs::read(std::env::var("TTV_VOICES").unwrap()).unwrap()).unwrap();
        let (p, entries) = super::build(&l.voices).unwrap();
        for d in &p.defs {
            println!("{} | {} | {} | {}", d.name, d.gender, d.age, d.description);
        }
        println!("{} defs, {} entries", p.defs.len(), entries.len());
    }
}

#[cfg(test)]
mod real_preset {
    /// `TTV_PRESETS=%APPDATA%/io.github.zcarecrow.ttvtts/presets.json cargo test -p ttv-tts --lib classic_outdated_real -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn classic_outdated_real() {
        let all: Vec<crate::presets::Preset> = serde_json::from_slice(&std::fs::read(std::env::var("TTV_PRESETS").unwrap()).unwrap()).unwrap();
        let p = all.iter().find(|p| p.id == super::ID).expect("classic");
        for d in p.defs.iter().take(5) {
            println!("{} | {} | {} | is_description={}", d.name, d.gender, d.description, super::is_description(&d.description));
        }
        println!("outdated: {}", super::outdated(p));
    }
}
