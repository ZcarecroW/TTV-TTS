//! From chat message to speakable text (port of TTV-TTS 1.0's pipeline):
//! split into words, emotes, emojis, links and mentions; detect the language;
//! render the text to speak with the profile's rules.

pub mod emoji;
pub mod lang;
pub mod langs;

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

use crate::profile::{BlockedMode, EmojiMode, EmoteMode, LinkMode, MentionMode, Profile};

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Emote(String),
    Emoji(String),
    Link,
    Mention { name: String, tail: String },
}

#[derive(Debug, Clone, Default)]
pub struct Analysis {
    pub parts: Vec<Part>,
    /// Only the words (for language detection).
    pub plain: String,
    pub emote_only: bool,
    pub has_link: bool,
}

fn link_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i)^(?:(?:https?://|www\.)\S+|[\p{L}\d-]+(?:\.[\p{L}\d-]+)*\.(?:com|net|org|de|at|ch|tv|gg|io|ru|ua|uk|fr|es|it|nl|pl|tr|eu|info|me|co|ly|be|app|dev|xyz|link|live|shop|online|site|store|club|fm|to|cc|us|ca|jp|cn|kr|br|pt)(?:/\S*)?)$").unwrap()
    })
}
fn mention_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^@([\p{L}\p{N}_]+)([^\p{L}\p{N}_]*)$").unwrap())
}
fn emoji_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\p{Extended_Pictographic}|\p{Regional_Indicator}|^[#*0-9]\x{FE0F}?\x{20E3}$").unwrap())
}
fn cheer_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)^[a-z]+\d+$").unwrap())
}

/// Emote names at the code-point positions Twitch sends ("25:0-4,12-16/1902:6-10").
pub fn twitch_emote_names(text: &str, tag: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    if tag.is_empty() {
        return names;
    }
    let chars: Vec<char> = text.chars().collect();
    for group in tag.split('/') {
        let Some((_, ranges)) = group.split_once(':') else { continue };
        for r in ranges.split(',') {
            if let Some((a, b)) = r.split_once('-')
                && let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>())
                && a <= b
                && b < chars.len()
            {
                names.insert(chars[a..=b].iter().collect());
            }
        }
    }
    names
}

/// Splits a message into parts. `original` is the raw message (Twitch emote
/// positions refer to it), `text` the part to read (e.g. without "!tts").
pub fn analyze(text: &str, original: &str, emotes_tag: &str, bits: u32, is_emote: &dyn Fn(&str) -> bool) -> Analysis {
    let twitch = twitch_emote_names(original, emotes_tag);
    let mut a = Analysis::default();
    let (mut emotes, mut others) = (0usize, 0usize);
    for token in text.split_whitespace() {
        if twitch.contains(token) || is_emote(token) {
            a.parts.push(Part::Emote(token.to_string()));
            emotes += 1;
            continue;
        }
        others += 1;
        if bits > 0 && cheer_re().is_match(token) {
            continue;
        }
        if link_re().is_match(token) {
            a.parts.push(Part::Link);
            a.has_link = true;
            continue;
        }
        if let Some(c) = mention_re().captures(token) {
            a.parts.push(Part::Mention { name: c[1].to_string(), tail: c[2].to_string() });
            continue;
        }
        if token.is_ascii() {
            a.parts.push(Part::Text(token.to_string()));
            continue;
        }
        // Split emojis off words ("hello😂").
        let mut buf = String::new();
        for g in token.graphemes(true) {
            if emoji_re().is_match(g) {
                if !buf.is_empty() {
                    a.parts.push(Part::Text(std::mem::take(&mut buf)));
                }
                a.parts.push(Part::Emoji(g.to_string()));
            } else {
                buf.push_str(g);
            }
        }
        if !buf.is_empty() {
            a.parts.push(Part::Text(buf));
        }
    }
    a.plain = a.parts.iter().filter_map(|p| if let Part::Text(t) = p { Some(t.as_str()) } else { None }).collect::<Vec<_>>().join(" ");
    a.emote_only = emotes > 0 && others == 0;
    a
}

/// How an emote is pronounced: alias, else its name split into words
/// ("PepeLaugh" -> "Pepe Laugh"); "" = silent.
pub fn emote_spoken(name: &str, profile: &Profile) -> String {
    if let Some((_, v)) = profile.emotes.aliases.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)) {
        return v.clone();
    }
    if !name.chars().any(char::is_alphabetic) {
        return String::new();
    }
    if !profile.emotes.split_words {
        return name.to_string();
    }
    static RES: OnceLock<[Regex; 4]> = OnceLock::new();
    let res = RES.get_or_init(|| {
        [Regex::new(r"([a-z])([A-Z])").unwrap(), Regex::new(r"([A-Z]+)([A-Z][a-z])").unwrap(), Regex::new(r"(\d)(\p{L})").unwrap(), Regex::new(r"[_:]+").unwrap()]
    });
    let s = res[0].replace_all(name, "$1 $2");
    let s = res[1].replace_all(&s, "$1 $2");
    let s = res[2].replace_all(&s, "$1 $2");
    res[3].replace_all(&s, " ").trim().to_string()
}

/// How a viewer's name is spoken: alias, else the display name cleaned up
/// ("max_mustermann123" -> "max mustermann").
pub fn spoken_name(login: &str, display: &str, profile: &Profile) -> String {
    if let Some(a) = profile.names.aliases.get(&login.to_ascii_lowercase()).filter(|a| !a.trim().is_empty()) {
        return a.clone();
    }
    let name = if display.is_empty() { login } else { display };
    if profile.names.cleanup {
        let cleaned: String = name.chars().map(|c| if matches!(c, '_' | '-' | '.') { ' ' } else { c }).collect();
        let cleaned = cleaned.trim_end_matches(|c: char| c.is_ascii_digit()).split_whitespace().collect::<Vec<_>>().join(" ");
        if cleaned.chars().any(char::is_alphabetic) {
            return cleaned;
        }
    }
    name.to_string()
}

/// "sooooo" -> "so", "hahahahaha" -> "haha", "!!!!!" -> "!", "?!?!" -> "?".
pub fn collapse_repeats(s: &str) -> String {
    static RES: OnceLock<[Regex; 3]> = OnceLock::new();
    let res = RES.get_or_init(|| [Regex::new(r"[!?]{2,}").unwrap(), Regex::new(r",{2,}").unwrap(), Regex::new(r"\.{4,}").unwrap()]);
    let s = res[0].replace_all(s, |c: &regex::Captures| if c[0].contains('?') { "?" } else { "!" });
    let s = res[1].replace_all(&s, ",");
    let s = res[2].replace_all(&s, "...").into_owned();
    // Repeated 2–4 letter units (3+ times) -> twice.
    let chars: Vec<char> = s.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    'outer: while i < chars.len() {
        for unit in 2..=4 {
            if i + unit * 3 > chars.len() || !chars[i..i + unit].iter().all(|c| c.is_alphabetic()) {
                continue;
            }
            let mut reps = 1;
            while i + unit * (reps + 1) <= chars.len() && lower[i + unit * reps..i + unit * (reps + 1)] == lower[i..i + unit] {
                reps += 1;
            }
            if reps >= 3 {
                out.extend_from_slice(&chars[i..i + unit]);
                out.extend_from_slice(&chars[i..i + unit]);
                i += unit * reps;
                continue 'outer;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    // The same letter 3+ times -> once.
    let mut res2 = String::with_capacity(out.len());
    let mut j = 0;
    while j < out.len() {
        let c = out[j];
        let mut k = j + 1;
        while k < out.len() && out[k] == c {
            k += 1;
        }
        if c.is_alphabetic() && k - j >= 3 {
            res2.push(c);
        } else {
            out[j..k].iter().for_each(|ch| res2.push(*ch));
        }
        j = k;
    }
    res2
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

/// Replaces whole words (case-insensitive), like `(?<![\p{L}\p{N}])word(?![\p{L}\p{N}])`.
pub fn replace_words(s: &str, from: &str, to: &str) -> String {
    let Ok(re) = Regex::new(&format!("(?i){}", regex::escape(from))) else { return s.to_string() };
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    let mut pos = 0;
    while let Some(m) = re.find_at(s, pos) {
        let before_ok = s[..m.start()].chars().next_back().is_none_or(|c| !is_word(c));
        let after_ok = s[m.end()..].chars().next().is_none_or(|c| !is_word(c));
        if before_ok && after_ok {
            out.push_str(&s[last..m.start()]);
            out.push_str(to);
            last = m.end();
        }
        pos = if m.end() > m.start() { m.end() } else { m.end() + s[m.end()..].chars().next().map_or(1, char::len_utf8) };
        if pos > s.len() {
            break;
        }
    }
    out.push_str(&s[last..]);
    out
}

/// Shortens to `max` characters, preferably at a sentence end, else a comma.
pub fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    let sentence = [". ", "! ", "? ", "… "].iter().filter_map(|p| cut.rfind(p)).max();
    let clause = [", ", "; "].iter().filter_map(|p| cut.rfind(p)).max();
    let len = cut.len();
    if let Some(i) = sentence.filter(|i| *i >= len / 2) {
        return cut[..=i].to_string();
    }
    if let Some(i) = clause.filter(|i| *i * 10 >= len * 6) {
        return format!("{} …", &cut[..i]);
    }
    let space = cut.rfind(' ').filter(|i| *i * 10 >= len * 6).unwrap_or(len);
    format!("{} …", cut[..space].trim())
}

/// The text to speak in `lang`, or None if nothing speakable remains (or a
/// blocked word drops the message).
pub fn render(a: &Analysis, lang: &str, profile: &Profile) -> Option<String> {
    let l = langs::get(lang).or_else(|| langs::get(&profile.language.default));
    let mut pieces: Vec<(bool, String)> = Vec::new(); // (symbol?, text)
    let (mut seen_emotes, mut seen_emojis) = (HashSet::new(), HashSet::new());
    for p in &a.parts {
        match p {
            Part::Text(t) => pieces.push((false, t.clone())),
            Part::Link => {
                if profile.text.links == LinkMode::Replace
                    && let Some(l) = l
                    && pieces.last().map(|x| x.1.as_str()) != Some(l.link)
                {
                    pieces.push((false, l.link.to_string()));
                }
            }
            Part::Mention { name, tail } => {
                if profile.text.mentions == MentionMode::Name {
                    pieces.push((false, format!("{}{}", spoken_name(name, name, profile), tail)));
                }
            }
            Part::Emote(e) => match profile.emotes.mode {
                EmoteMode::Remove => {}
                EmoteMode::Keep => pieces.push((false, e.clone())),
                EmoteMode::Once => {
                    let key = e.to_lowercase();
                    if seen_emotes.contains(&key) || seen_emotes.len() >= profile.emotes.max_per_message {
                        continue;
                    }
                    seen_emotes.insert(key);
                    let spoken = emote_spoken(e, profile);
                    if !spoken.is_empty() {
                        pieces.push((true, spoken));
                    }
                }
            },
            Part::Emoji(g) => match profile.emojis.mode {
                EmojiMode::Keep => pieces.push((false, g.clone())),
                EmojiMode::Remove => {}
                EmojiMode::Describe => {
                    let w = emoji::word(g, lang);
                    if w.is_empty() || seen_emojis.contains(w) || seen_emojis.len() >= profile.emojis.max_per_message {
                        continue;
                    }
                    seen_emojis.insert(w);
                    pieces.push((true, w.to_string()));
                }
            },
        }
    }
    // Emote/emoji words get commas around them: more natural pauses.
    let mut out = String::new();
    for (i, (sym, text)) in pieces.iter().enumerate() {
        if i > 0 {
            out.push_str(if *sym || pieces[i - 1].0 { ", " } else { " " });
        }
        out.push_str(text);
    }
    if profile.text.collapse_repeats {
        out = collapse_repeats(&out);
    }
    if profile.text.fix_shouting {
        let letters = out.chars().filter(|c| c.is_alphabetic()).count();
        let upper = out.chars().filter(|c| c.is_uppercase()).count();
        if letters >= 8 && upper * 10 > letters * 7 {
            out = out.to_lowercase();
        }
    }
    for key in ["all", lang] {
        if let Some(map) = profile.text.replacements.get(key) {
            for (from, to) in map {
                out = replace_words(&out, from, to);
            }
        }
    }
    for word in profile.text.blocked_words.iter().filter(|w| !w.trim().is_empty()) {
        let Ok(re) = Regex::new(&format!("(?i){}", regex::escape(word.trim()))) else { continue };
        if !re.is_match(&out) {
            continue;
        }
        if profile.text.blocked_mode == BlockedMode::Skip {
            return None;
        }
        out = re.replace_all(&out, l.map(|l| l.beep).unwrap_or("beep")).into_owned();
    }
    // Tidy spaces and punctuation.
    static TIDY: OnceLock<[Regex; 4]> = OnceLock::new();
    let tidy = TIDY.get_or_init(|| [Regex::new(r"\s+").unwrap(), Regex::new(r"\s+([,.!?])").unwrap(), Regex::new(r"([.!?…:;]),").unwrap(), Regex::new(r"^[\s,.;:!?-]+").unwrap()]);
    let out = tidy[0].replace_all(&out, " ");
    let out = tidy[1].replace_all(&out, "$1");
    let out = tidy[2].replace_all(&out, "$1");
    let out = tidy[3].replace_all(&out, "").trim().to_string();
    let out = shorten(&out, profile.reading.max_chars);
    out.chars().any(char::is_alphanumeric).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> Profile {
        Profile::default()
    }

    fn say(text: &str, lang: &str, third: &[&str]) -> Option<String> {
        let set: HashSet<String> = third.iter().map(|s| s.to_string()).collect();
        render(&analyze(text, text, "", 0, &|w| set.contains(w)), lang, &p())
    }

    #[test]
    fn emotes_once_with_aliases_and_split_words() {
        assert_eq!(say("KEKW KEKW KEKW", "en", &["KEKW"]).as_deref(), Some("kek"));
        assert_eq!(say("that was great PepeLaugh catJAM", "en", &["PepeLaugh", "catJAM"]).as_deref(), Some("that was great, Pepe Laugh, cat JAM"));
        assert_eq!(say(":) :(", "en", &[":)", ":("]), None, "silent smileys leave nothing");
    }

    #[test]
    fn twitch_emote_positions_are_code_points() {
        // "😂 Kappa" – the emoji is one code point, Kappa at 2-6
        let names = twitch_emote_names("😂 Kappa", "25:2-6");
        assert!(names.contains("Kappa"));
    }

    #[test]
    fn emojis_become_words_once() {
        assert_eq!(say("das war gut 😂😂😂", "de", &[]).as_deref(), Some("das war gut, lachend"));
        assert_eq!(say("hello😂", "en", &[]).as_deref(), Some("hello, laughing"));
        assert_eq!(say("привет 🔥", "ru", &[]).as_deref(), Some("привет, огонь"));
        assert_eq!(say("bonjour 🔥", "fr", &[]).as_deref(), Some("bonjour"));
    }

    #[test]
    fn links_mentions_repeats_caps() {
        assert_eq!(say("look at https://example.com/x now", "en", &[]).as_deref(), Some("look at link now"));
        assert_eq!(say("schau twitch.tv/zcarecrow", "de", &[]).as_deref(), Some("schau Link"));
        assert_eq!(say("@max_mustermann123, hi", "en", &[]).as_deref(), Some("max mustermann, hi"));
        assert_eq!(say("sooooo cool hahahahaha!!!!", "en", &[]).as_deref(), Some("so cool haha!"));
        assert_eq!(say("WHAT IS GOING ON HERE", "en", &[]).as_deref(), Some("what is going on here"));
    }

    #[test]
    fn replacements_are_whole_words_per_language() {
        assert_eq!(say("gg wp", "en", &[]).as_deref(), Some("good game well played"));
        assert_eq!(say("gg", "de", &[]).as_deref(), Some("gut gespielt"));
        assert_eq!(say("eggs", "en", &[]).as_deref(), Some("eggs"), "no match inside words");
        assert_eq!(say("come w/ me", "en", &[]).as_deref(), Some("come with me"));
        assert_eq!(replace_words("GG gg Gg", "gg", "x"), "x x x");
    }

    #[test]
    fn blocked_words() {
        let mut prof = p();
        prof.text.blocked_words = vec!["badword".into()];
        let set: HashSet<String> = HashSet::new();
        assert_eq!(render(&analyze("you badword!", "you badword!", "", 0, &|w| set.contains(w)), "en", &prof), None);
        prof.text.blocked_mode = BlockedMode::Beep;
        assert_eq!(render(&analyze("you badword!", "you badword!", "", 0, &|w| set.contains(w)), "en", &prof).as_deref(), Some("you beep!"));
    }

    #[test]
    fn cheermotes_are_dropped_when_bits_are_sent() {
        let set: HashSet<String> = HashSet::new();
        assert_eq!(render(&analyze("Cheer100 nice", "Cheer100 nice", "", 100, &|w| set.contains(w)), "en", &p()).as_deref(), Some("nice"));
    }

    #[test]
    fn long_messages_are_cut_at_a_sentence_end() {
        let mut prof = p();
        prof.reading.max_chars = 40;
        let set: HashSet<String> = HashSet::new();
        let text = "This is the first sentence here. And this one goes on and on forever.";
        assert_eq!(render(&analyze(text, text, "", 0, &|w| set.contains(w)), "en", &prof).as_deref(), Some("This is the first sentence here."));
    }

    #[test]
    fn names() {
        let mut prof = p();
        assert_eq!(spoken_name("xx_gamer_xx", "xx_gamer_xx", &prof), "xx gamer xx");
        assert_eq!(spoken_name("user123", "User123", &prof), "User");
        assert_eq!(spoken_name("12345", "12345", &prof), "12345");
        prof.names.aliases.insert("xx_gamer_xx".into(), "Gamer".into());
        assert_eq!(spoken_name("XX_Gamer_XX", "XX_Gamer_XX", &prof), "Gamer");
    }
}
