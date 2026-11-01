//! Voice presets: themed lists of language-neutral voice definitions.
//! 20 come with the app, users can make their own (by hand or with an AI).
//!
//! Exchange format (lean, AI-friendly, survives cut-off answers):
//! ```text
//! #ttv-tts preset v1
//! name: Noir City
//! desc: 1940s detectives, singers, mobsters
//! ---
//! Vic Malone|m|52|Gravelly baritone, slow and dry, tired cynicism.
//! Lola Fine|f|31|Velvety low alto, unhurried, teasing warmth.|fx=radio
//! ```

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use ttv_link::VoiceDef;

use crate::library::Counts;

pub const MAGIC: &str = "#ttv-tts preset v1";
/// Effects the player knows (M6); others are accepted with a warning.
pub const EFFECTS: &[&str] = &["radio", "telephone", "megaphone", "robot", "cave", "hall", "walkie", "underwater", "whisper"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Nucleo icon name used in the app.
    pub icon: String,
    pub builtin: bool,
    /// Planned size of an own preset (the AI prompt asks for this many).
    pub target: u32,
    pub defs: Vec<VoiceDef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub builtin: bool,
    pub count: u32,
    pub target: u32,
    /// (men, women, neutral) among the first 30 voices.
    pub mix: (u32, u32, u32),
}

impl Preset {
    pub fn summary(&self) -> PresetSummary {
        let first = &self.defs[..self.defs.len().min(30)];
        let count = |g: &str| first.iter().filter(|d| d.gender == g).count() as u32;
        PresetSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            icon: self.icon.clone(),
            builtin: self.builtin,
            count: self.defs.len() as u32,
            target: self.target.max(self.defs.len() as u32),
            mix: (count("m"), count("f"), count("n")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
    /// 1-based line in the pasted text (0 = whole text).
    pub line: usize,
    /// "error" or "warning"
    pub level: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Parsed {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub defs: Vec<VoiceDef>,
    pub issues: Vec<Issue>,
}

impl Parsed {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.level == "error")
    }
}

fn gender(s: &str) -> Option<&'static str> {
    match s.trim().to_ascii_lowercase().as_str() {
        "m" | "male" | "man" | "masculine" => Some("m"),
        "f" | "female" | "woman" | "feminine" | "w" => Some("f"),
        "n" | "neutral" | "nb" | "non-binary" | "nonbinary" | "x" | "androgynous" => Some("n"),
        _ => None,
    }
}

/// Parses the exchange format. Tolerant of what AI chats add (code fences,
/// numbering, bullets, "female", "40s"); reports problems per line.
pub fn parse(text: &str) -> Parsed {
    let mut p = Parsed::default();
    let mut names: HashSet<String> = HashSet::new();
    let mut descs: HashSet<String> = HashSet::new();
    let mut in_header = true;
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with("```") || line.eq_ignore_ascii_case(MAGIC) {
            continue;
        }
        if line == "---" {
            in_header = false;
            continue;
        }
        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if !line.contains('|') {
            if in_header && let Some((k, v)) = line.split_once(':') {
                match k.trim().to_ascii_lowercase().as_str() {
                    "name" => p.name = v.trim().chars().take(60).collect(),
                    "desc" | "description" => p.description = v.trim().chars().take(200).collect(),
                    "icon" => p.icon = v.trim().to_string(),
                    _ => {}
                }
                continue;
            }
            p.issues.push(Issue { line: n, level: "warning", message: "not a voice line (expected Name|gender|age|description) – skipped".into() });
            continue;
        }
        in_header = false;
        // Drop "1. ", "12) ", "- ", "* " that AIs like to add.
        let body = line.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches(['.', ')', '-', '*', ' ']);
        let body = if body.contains('|') { body } else { line };
        let fields: Vec<&str> = body.split('|').map(str::trim).collect();
        if fields.len() < 4 {
            p.issues.push(Issue { line: n, level: "error", message: format!("{} fields – needs Name|gender|age|description", fields.len()) });
            continue;
        }
        let name: String = fields[0].trim_matches('*').trim().chars().take(40).collect();
        let Some(g) = gender(fields[1]) else {
            p.issues.push(Issue { line: n, level: "error", message: format!("gender \"{}\" – use m, f or n", fields[1]) });
            continue;
        };
        let digits: String = fields[2].chars().filter(|c| c.is_ascii_digit()).collect();
        let age = match digits.parse::<u32>() {
            Ok(a) if (5..=99).contains(&a) => a as u8,
            _ => {
                p.issues.push(Issue { line: n, level: "error", message: format!("age \"{}\" – use a number from 5 to 99", fields[2]) });
                continue;
            }
        };
        let description = fields[3].trim_matches('"').trim().to_string();
        let len = description.chars().count();
        if len < 20 {
            p.issues.push(Issue { line: n, level: "error", message: "description too short – describe pitch, timbre, pace and attitude".into() });
            continue;
        }
        if len > 400 {
            p.issues.push(Issue { line: n, level: "error", message: "description too long (max. 400 characters)".into() });
            continue;
        }
        let mut fx = None;
        for extra in &fields[4..] {
            let e = extra.trim();
            if e.is_empty() {
                continue;
            }
            let name = e.strip_prefix("fx=").unwrap_or(e).trim().to_ascii_lowercase();
            if EFFECTS.contains(&name.as_str()) {
                fx = Some(name);
            } else {
                p.issues.push(Issue { line: n, level: "warning", message: format!("unknown effect \"{e}\" – ignored") });
            }
        }
        if name.is_empty() {
            p.issues.push(Issue { line: n, level: "error", message: "missing name".into() });
            continue;
        }
        let dkey = description.to_lowercase();
        if !descs.insert(dkey) {
            p.issues.push(Issue { line: n, level: "error", message: "same description as an earlier voice – every voice must be unique".into() });
            continue;
        }
        let mut final_name = name.clone();
        if !names.insert(name.to_lowercase()) {
            let mut k = 2;
            while names.contains(&format!("{} {k}", name.to_lowercase())) {
                k += 1;
            }
            final_name = format!("{name} {k}");
            names.insert(final_name.to_lowercase());
            p.issues.push(Issue { line: n, level: "warning", message: format!("name used twice – renamed to \"{final_name}\"") });
        }
        p.defs.push(VoiceDef { name: final_name, gender: g.into(), age, description, fx });
    }
    if p.defs.is_empty() {
        p.issues.push(Issue { line: 0, level: "error", message: "no voices found".into() });
    }
    p
}

/// Writes a preset in the exchange format.
pub fn serialize(p: &Preset) -> String {
    let mut s = format!("{MAGIC}\nname: {}\ndesc: {}\n---\n", p.name, p.description);
    for d in &p.defs {
        s.push_str(&format!("{}|{}|{}|{}", d.name, d.gender, d.age, d.description));
        if let Some(fx) = &d.fx {
            s.push_str(&format!("|fx={fx}"));
        }
        s.push('\n');
    }
    s
}

// ── built-in presets ────────────────────────────────────────────────────

macro_rules! builtin {
    ($($id:literal),* $(,)?) => {
        &[$(($id, include_str!(concat!("../presets/", $id, ".txt")))),*]
    };
}

/// (id, text) – the files live in `src-tauri/presets/` (name, desc and
/// icon in their header). Parsed once, on first use.
const BUILTIN: &[(&str, &str)] = builtin![
    "cinematic",
    "everyday-neighbors",
    "fantasy-tavern",
    "starship-crew",
    "noir-city",
    "saturday-cartoons",
    "high-seas",
    "royal-court",
    "midnight-horror",
    "arena-casters",
    "on-air",
    "wild-planet",
    "storytime",
    "frontier-west",
    "neon-streets",
    "olympus",
    "capes-and-masks",
    "grand-stage",
    "quest-givers",
    "calm-and-cozy",
];

pub fn builtins() -> &'static [Preset] {
    static B: OnceLock<Vec<Preset>> = OnceLock::new();
    B.get_or_init(|| {
        BUILTIN
            .iter()
            .map(|(id, text)| {
                let p = parse(text);
                if p.has_errors() {
                    log::error!("built-in preset {id} has errors: {:?}", p.issues.iter().filter(|i| i.level == "error").take(3).collect::<Vec<_>>());
                }
                let icon = if p.icon.is_empty() { "Waveform".to_string() } else { p.icon };
                Preset { id: id.to_string(), name: p.name, description: p.description, icon, builtin: true, target: p.defs.len() as u32, defs: p.defs }
            })
            .collect()
    })
}

// ── own presets ─────────────────────────────────────────────────────────

pub struct PresetStore {
    path: PathBuf,
    own: RwLock<Vec<Preset>>,
}

impl PresetStore {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("presets.json");
        let own: Vec<Preset> = crate::storage::read_json(&path).unwrap_or_default();
        Self { path, own: RwLock::new(own) }
    }

    fn save(&self) {
        let own = self.own.read().clone();
        if let Err(e) = crate::storage::write_json(&self.path, &own) {
            log::warn!("saving presets failed: {e}");
        }
    }


    pub fn get(&self, id: &str) -> Option<Preset> {
        builtins().iter().find(|p| p.id == id).cloned().or_else(|| self.own.read().iter().find(|p| p.id == id).cloned())
    }

    /// Creates or replaces an own preset (built-ins can't be changed).
    pub fn put(&self, mut p: Preset) -> Result<Preset, String> {
        if builtins().iter().any(|b| b.id == p.id) {
            return Err("Built-in presets can't be changed – duplicate it first.".into());
        }
        if p.id.is_empty() {
            p.id = format!("u-{}", crate::secrets::random_key()[..8].to_ascii_lowercase());
        }
        p.builtin = false;
        p.name = p.name.trim().chars().take(60).collect();
        if p.name.is_empty() {
            p.name = "My preset".into();
        }
        if p.icon.is_empty() {
            p.icon = "Waveform".into();
        }
        p.target = p.target.clamp(1, 1000).max(p.defs.len() as u32);
        let mut own = self.own.write();
        match own.iter_mut().find(|x| x.id == p.id) {
            Some(x) => *x = p.clone(),
            None => own.push(p.clone()),
        }
        drop(own);
        self.save();
        Ok(p)
    }

    pub fn delete(&self, id: &str) {
        self.own.write().retain(|p| p.id != id);
        self.save();
    }

    /// Runs `f` with a preset without cloning it (the reader, per message).
    pub fn with<R>(&self, id: &str, f: impl FnOnce(&Preset) -> R) -> Option<R> {
        if let Some(b) = builtins().iter().find(|p| p.id == id) {
            return Some(f(b));
        }
        self.own.read().iter().find(|p| p.id == id).map(f)
    }

    pub fn summaries(&self, c: &Counts) -> Vec<PresetView> {
        let own = self.own.read();
        builtins().iter().chain(own.iter()).map(|p| PresetView::new(p, c)).collect()
    }

    /// Calls `f` for every preset (built-ins first).
    pub fn each(&self, mut f: impl FnMut(&Preset)) {
        for p in builtins() {
            f(p);
        }
        for p in self.own.read().iter() {
            f(p);
        }
    }
}

// ── views for the window ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PresetView {
    #[serde(flatten)]
    pub summary: PresetSummary,
    /// language -> computed voices (of this preset's definitions)
    pub computed: BTreeMap<String, u32>,
    /// Definitions computed in at least one language.
    pub ready: u32,
    /// Voices the quality check lists (may sound odd).
    pub odd: u32,
}

impl PresetView {
    pub fn new(p: &Preset, c: &Counts) -> Self {
        let mut computed = BTreeMap::new();
        let (mut ready, mut odd) = (0, 0);
        for d in &p.defs {
            let hash = d.hash();
            if let Some(langs) = c.by_def.get(&hash).filter(|l| !l.is_empty()) {
                ready += 1;
                for l in langs {
                    *computed.entry(l.clone()).or_insert(0) += 1;
                }
                odd += c.odd.get(&hash).map_or(0, |l| l.len() as u32);
            }
        }
        Self { summary: p.summary(), computed, ready, odd }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceView {
    pub index: u32,
    #[serde(flatten)]
    pub def: VoiceDef,
    pub hash: String,
    /// Languages this voice is computed in.
    pub langs: Vec<String>,
    /// Languages whose voice may sound odd (quality check).
    pub odd: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetDetail {
    #[serde(flatten)]
    pub view: PresetView,
    pub voices: Vec<VoiceView>,
}

impl PresetDetail {
    pub fn new(p: &Preset, c: &Counts) -> Self {
        let voices = p
            .defs
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let hash = d.hash();
                let mut langs = c.by_def.get(&hash).cloned().unwrap_or_default();
                langs.sort();
                let mut odd = c.odd.get(&hash).cloned().unwrap_or_default();
                odd.sort();
                VoiceView { index: i as u32, def: d.clone(), hash, langs, odd }
            })
            .collect();
        Self { view: PresetView::new(p, c), voices }
    }
}

// ── AI prompt builder ───────────────────────────────────────────────────

/// Voices per AI answer: long lists get cut off or repetitive.
pub const CHUNK: u32 = 60;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PromptRequest {
    /// The theme in the user's words ("a pirate crew", "my cozy café").
    pub theme: String,
    pub name: String,
    /// Voices wanted in total.
    pub total: u32,
    /// Voices already there (pasted earlier); the prompt continues after them.
    pub have: Vec<VoiceDef>,
    /// Allow optional sound effects.
    pub effects: bool,
}

/// Builds the prompt for the next chunk. The app never calls an AI itself.
pub fn prompt(r: &PromptRequest) -> String {
    let total = r.total.clamp(1, 1000);
    let have = r.have.len() as u32;
    let start = have + 1;
    let count = total.saturating_sub(have).clamp(1, CHUNK);
    let end = start + count - 1;
    let parts = total.div_ceil(CHUNK);
    let part = have / CHUNK + 1;
    let theme = r.theme.trim();
    let name = if r.name.trim().is_empty() { theme } else { r.name.trim() };
    let mut s = format!(
        "You design voices for a text-to-speech app that reads Twitch chat aloud. A voice-design model creates each voice from your description, so describe only how the voice SOUNDS.\n\nTheme: {theme}\n\n"
    );
    if have == 0 {
        s.push_str(&format!("Write {count} voice definitions. Reply with ONLY these lines – no commentary, no numbering, no code block:\n\n{MAGIC}\nname: {name}\ndesc: <one short line about the theme>\n---\nName|gender|age|description\n…\n\n"));
    } else {
        s.push_str(&format!(
            "This is part {part} of {parts}: write voices {start} to {end} ({count} lines). Reply with ONLY the voice lines – no header, no commentary, no numbering, no code block:\n\nName|gender|age|description\n…\n\n"
        ));
    }
    s.push_str("Rules:\n");
    s.push_str("- gender: m, f or n (androgynous)\n");
    s.push_str("- age: one number from 8 to 95\n");
    s.push_str("- description: one or two English sentences, at most 300 characters: pitch, timbre, texture, pace, energy and attitude. No language, nationality or accent (the app adds the language), no real people, fictional characters, brands or sound effects.\n");
    s.push_str("- Every voice must sound clearly different from all others – vary pitch, texture, pace and mood, not just the wording.\n");
    s.push_str("- Balance: about 45 % m, 45 % f, 10 % n, mixed evenly through the list; ages spread widely (at least 15 % under 30 and 15 % over 55).\n");
    s.push_str("- Names: invented, fitting the theme, each used only once. No \"|\" inside a field.\n");
    if r.effects {
        s.push_str(&format!("- Optional, at most one voice in ten, only where the theme clearly calls for it: append |fx=<effect> with one of: {}.\n", EFFECTS.join(", ")));
    }
    if have > 0 {
        let names: Vec<&str> = r.have.iter().map(|d| d.name.as_str()).collect();
        s.push_str(&format!("\nAlready used – don't repeat these names or sound like these voices:\n{}\n", names.join(", ")));
        // The last few lines remind the AI of the style.
        s.push_str("\nThe last voices so far, for style:\n");
        let tail = r.have.len().saturating_sub(5);
        for d in &r.have[tail..] {
            s.push_str(&format!("{}|{}|{}|{}\n", d.name, d.gender, d.age, d.description));
        }
    } else {
        s.push_str("\nExample lines (style only – don't reuse them):\nVic Malone|m|52|Gravelly baritone, slow and dry, with tired cynicism and long pauses.\nLola Fine|f|31|Velvety low alto, unhurried and teasing, a smile in every word.\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_example() {
        let p = parse("#ttv-tts preset v1\nname: Noir City\ndesc: 1940s detectives\n---\nVic Malone|m|52|Gravelly baritone, slow and dry, tired cynicism.\nLola Fine|f|31|Velvety low alto, unhurried, teasing warmth.|fx=radio\n");
        assert!(!p.has_errors(), "{:?}", p.issues);
        assert_eq!(p.name, "Noir City");
        assert_eq!(p.defs.len(), 2);
        assert_eq!(p.defs[1].fx.as_deref(), Some("radio"));
        assert_eq!(p.defs[0].age, 52);
    }

    #[test]
    fn tolerates_ai_formatting() {
        let text = "Here you go:\n```\n1. Ada Quill | Female | 40s | Crisp, precise contralto with dry wit and quick pacing.\n2) Bo Hart|male|~65|Booming, jovial bass, slow and warm with a hearty laugh.\n```";
        let p = parse(text);
        assert_eq!(p.defs.len(), 2, "{:?}", p.issues);
        assert_eq!(p.defs[0].gender, "f");
        assert_eq!(p.defs[0].age, 40);
        assert_eq!(p.defs[1].age, 65);
        assert!(p.issues.iter().all(|i| i.level == "warning"));
    }

    #[test]
    fn reports_problems_per_line() {
        let p = parse("A|q|30|A long enough description of a voice.\nB|m|200|A long enough description of a voice two.\nC|f|30|short\nD|f|30|A long enough description of a voice.\nE|f|30\n");
        let errs: Vec<usize> = p.issues.iter().filter(|i| i.level == "error").map(|i| i.line).collect();
        assert_eq!(errs, vec![1, 2, 3, 5]);
        assert_eq!(p.defs.len(), 1);
        // D duplicates A's description? No – A failed (gender), so D is fine.
        assert_eq!(p.defs[0].name, "D");
    }

    #[test]
    fn duplicate_names_are_renamed_duplicate_descriptions_rejected() {
        let p = parse("Max|m|30|First unique description of a calm voice.\nMax|m|40|Second unique description of a calm voice.\nOther|f|20|First unique description of a calm voice.\n");
        assert_eq!(p.defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["Max", "Max 2"]);
        assert!(p.issues.iter().any(|i| i.line == 3 && i.level == "error"));
    }

    #[test]
    fn roundtrip() {
        let p = parse("name: X\n---\nVic|m|52|Gravelly baritone, slow and dry, tired cynicism.|fx=radio\n");
        let preset = Preset { name: p.name.clone(), defs: p.defs.clone(), ..Default::default() };
        let again = parse(&serialize(&preset));
        assert_eq!(again.defs, p.defs);
    }

    #[test]
    fn builtins_are_valid_unique_and_balanced() {
        let mut all_descs = HashSet::new();
        for b in builtins() {
            let errors: Vec<_> = parse(BUILTIN.iter().find(|x| x.0 == b.id).unwrap().1).issues.into_iter().filter(|i| i.level == "error").collect();
            assert!(errors.is_empty(), "{}: {:?}", b.id, &errors[..errors.len().min(5)]);
            assert_eq!(b.defs.len(), 240, "{} has {} voices", b.id, b.defs.len());
            // Every tier is mixed: at least 40 % of each gender among the first 30 and 60.
            for tier in [30, 60, 120, 240] {
                let t = &b.defs[..tier];
                let m = t.iter().filter(|d| d.gender == "m").count();
                let f = t.iter().filter(|d| d.gender == "f").count();
                assert!(m * 10 >= tier * 4 && f * 10 >= tier * 4, "{} tier {tier}: {m} m / {f} f", b.id);
                let young = t.iter().filter(|d| d.age < 30).count();
                let old = t.iter().filter(|d| d.age >= 55).count();
                assert!(young * 10 >= tier && old * 10 >= tier, "{} tier {tier}: ages too narrow ({young} young, {old} old)", b.id);
            }
            for d in &b.defs {
                assert!(all_descs.insert(d.description.to_lowercase()), "{}: description used twice: {}", b.id, d.description);
            }
            assert!(!b.name.is_empty() && !b.description.is_empty() && b.icon != "Waveform", "{}: header (name, desc, icon) incomplete", b.id);
            // Warnings would mean silently skipped lines or unknown effects.
            let warnings = parse(BUILTIN.iter().find(|x| x.0 == b.id).unwrap().1).issues.into_iter().filter(|i| i.level == "warning").count();
            assert_eq!(warnings, 0, "{} has warnings", b.id);
        }
        assert_eq!(builtins().len(), 20);
        // Different definitions never share a hash (the library key).
        let hashes: HashSet<String> = builtins().iter().flat_map(|b| b.defs.iter().map(|d| d.hash())).collect();
        assert_eq!(hashes.len(), 20 * 240);
    }

    #[test]
    fn prompt_continues_in_parts() {
        let first = prompt(&PromptRequest { theme: "a cozy café".into(), name: "Café".into(), total: 150, have: vec![], effects: false });
        assert!(first.contains(MAGIC) && first.contains("Write 60 voice definitions") && !first.contains("fx="));
        let have: Vec<VoiceDef> = (0..60).map(|i| VoiceDef { name: format!("Guest {i}"), gender: "f".into(), age: 30, description: format!("Warm voice number {i}, calm and kind."), fx: None }).collect();
        let second = prompt(&PromptRequest { theme: "a cozy café".into(), name: "Café".into(), total: 150, have, effects: true });
        assert!(second.contains("part 2 of 3") && second.contains("voices 61 to 120") && second.contains("Guest 59") && second.contains("fx=<effect>"));
        assert!(!second.contains(MAGIC), "later parts only add voice lines");
    }

    /// Cost of the first parse and of one preset list (the window asks for
    /// it after library changes). `cargo test --release -p ttv-tts preset_timing -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn preset_timing() {
        let t = std::time::Instant::now();
        let n: usize = BUILTIN.iter().map(|(_, text)| parse(text).defs.len()).sum();
        let parse_ms = t.elapsed().as_secs_f64() * 1000.0;
        // A library with every voice computed in 10 languages.
        let mut by_def: std::collections::HashMap<String, Vec<String>> = Default::default();
        // and 10 % of them listed by the quality check
        let mut odd: std::collections::HashMap<String, Vec<String>> = Default::default();
        for b in builtins() {
            for d in &b.defs {
                by_def.insert(d.hash(), ["en", "de", "ru", "fr", "es", "it", "pt", "ja", "zh", "ko"].iter().map(|s| s.to_string()).collect());
                if d.name.len() % 10 == 0 {
                    odd.insert(d.hash(), vec!["en".into()]);
                }
            }
        }
        let store = PresetStore { path: std::env::temp_dir().join("ttv-presets-timing.json"), own: RwLock::new(Vec::new()) };
        let t = std::time::Instant::now();
        let views = store.summaries(&Counts { by_def: &by_def, odd: &odd });
        let list_ms = t.elapsed().as_secs_f64() * 1000.0;
        let json = serde_json::to_string(&views).unwrap();
        println!("parse {n} voices: {parse_ms:.2} ms · preset list (48 000 computed voices): {list_ms:.2} ms · {} KB JSON", json.len() / 1024);
        assert_eq!(views.len(), 20);
    }
}
