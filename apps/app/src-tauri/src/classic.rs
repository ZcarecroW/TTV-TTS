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
        let mut description = neutral(&v.voice.transcript);
        if description.chars().count() < 20 {
            description = format!("A {} voice, natural and clear, friendly and easy to listen to.", if label.is_empty() { "warm" } else { label.as_str() });
        }
        let basis = format!("{label} {description}");
        let name = if label.is_empty() { v.name.clone() } else { format!("{} ({label})", v.name) };
        index_of.insert(*role, defs.len());
        defs.push(VoiceDef { name, gender: guess_gender(&basis).into(), age: guess_age(&basis), description, fx: None });
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
}
