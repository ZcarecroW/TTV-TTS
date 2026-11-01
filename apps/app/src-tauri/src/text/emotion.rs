//! Emotion from chat cues: emotes, emojis, laughing or crying words and
//! punctuation pick one of VCP's emotions and how strongly to speak it
//! ("KEKW KEKW" → joy, "rip 😭" → sad, "WHAT?!" → surprise). VCP's
//! emotional clone (Qwen3 models) then speaks the saved voice with it;
//! measured on the target PC it costs nothing extra (0.51–0.54 s per second
//! of speech instead of 0.50). Mixed or unclear cues give no emotion.

use super::{Analysis, Part};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cue {
    /// One of VCP's emotions: joy, sad, anger, fear, surprise, disgust,
    /// contempt, awe, despair, disapproval.
    pub emotion: &'static str,
    /// "mild", "normal" or "strong" (VCP also knows very_mild and extreme;
    /// extreme distorts voices too easily).
    pub intensity: &'static str,
}

pub const LEVELS: [&str; 3] = ["mild", "normal", "strong"];

/// Well-known emotes, case-sensitive as in chat.
const EMOTES: &[(&str, &str)] = &[
    ("KEKW", "joy"), ("KEKL", "joy"), ("LUL", "joy"), ("LULW", "joy"), ("OMEGALUL", "joy"), ("ICANT", "joy"), ("4Head", "joy"),
    ("LOL", "joy"), ("PepeLaugh", "joy"), ("pepeLaugh", "joy"), ("EleGiggle", "joy"), ("HYPERS", "joy"), ("PogBones", "joy"),
    ("catJAM", "joy"), ("peepoHappy", "joy"), ("FeelsGoodMan", "joy"), ("Clap", "joy"), ("EZClap", "contempt"), ("<3", "joy"),
    ("PogChamp", "surprise"), ("Pog", "surprise"), ("POGGERS", "surprise"), ("PogU", "surprise"), ("PauseChamp", "surprise"),
    ("WAYTOODANK", "surprise"), ("monkaS", "fear"), ("monkaW", "fear"), ("monkaGIGA", "fear"), ("WutFace", "fear"), ("D:", "fear"),
    ("Sadge", "sad"), ("BibleThump", "sad"), ("FeelsBadMan", "sad"), ("PepeHands", "sad"), ("peepoSad", "sad"), ("widepeepoSad", "sad"),
    ("FeelsStrongMan", "sad"), ("NotLikeThis", "despair"), ("SwiftRage", "anger"), ("Madge", "anger"), ("ANGERY", "anger"),
    ("SMOrc", "anger"), ("DansGame", "disgust"), ("EZ", "contempt"), ("FailFish", "disapproval"), ("WeirdChamp", "disapproval"),
];

/// Emote names that tell by their parts ("pepeLaughing", "catSadge").
fn emote_by_name(name: &str) -> Option<&'static str> {
    if let Some((_, e)) = EMOTES.iter().find(|(n, _)| *n == name) {
        return Some(e);
    }
    let l = name.to_ascii_lowercase();
    let has = |p: &[&str]| p.iter().any(|x| l.contains(x));
    if has(&["kek", "lul", "laugh", "lmao", "giggle"]) {
        Some("joy")
    } else if has(&["sadge", "cry", "sad", "tear"]) {
        Some("sad")
    } else if has(&["pog"]) {
        Some("surprise")
    } else if has(&["monka", "scared", "fear"]) {
        Some("fear")
    } else if has(&["rage", "madge", "angry", "angery"]) {
        Some("anger")
    } else if has(&["love", "heart", "happy", "hype"]) {
        Some("joy")
    } else {
        None
    }
}

fn emoji(g: &str) -> Option<&'static str> {
    let plain: String = g.chars().filter(|c| !matches!(*c as u32, 0xFE0F | 0x1F3FB..=0x1F3FF)).collect();
    let c = plain.chars().next()?;
    Some(match c {
        '😂' | '🤣' | '😆' | '😹' | '😄' | '😁' | '😀' | '😃' | '😊' | '🙂' | '😍' | '🥰' | '😘' | '🥳' | '🎉' | '💀' | '👏' | '🙌' | '😻' => "joy",
        '❤' | '🧡' | '💛' | '💚' | '💙' | '💜' | '🖤' | '🤍' | '💖' | '💕' | '💗' | '💞' => "joy",
        '😭' | '😢' | '😞' | '😔' | '🙁' | '☹' | '💔' | '🥺' | '😿' => "sad",
        '😡' | '😠' | '🤬' | '😤' | '👿' | '💢' => "anger",
        '😱' | '😨' | '😰' | '😧' | '😦' => "fear",
        '😮' | '😯' | '😲' | '😳' | '🤯' | '😵' => "surprise",
        '🤢' | '🤮' => "disgust",
        '🙄' | '😒' => "contempt",
        '🤩' | '✨' => "awe",
        _ => return None,
    })
}

/// Words with a clear feeling (lower case, letters only).
fn word(w: &str) -> Option<&'static str> {
    // laughter in many spellings: haha, hahaa, hehe, hihi, jaja, xd, xdd
    let laugh = |w: &str| {
        let b = w.as_bytes();
        b.len() >= 4 && b.chunks(2).all(|p| p.len() == 2 && matches!(p[0], b'h' | b'j') && matches!(p[1], b'a' | b'e' | b'i')) || (w.starts_with("xd") && w[1..].bytes().all(|c| c == b'd'))
    };
    let stretched = |w: &str, base: &str| w.len() > base.len() && {
        // "nooo", "yaaay", "ewww": the base with its letters repeated
        let mut it = w.chars().peekable();
        base.chars().all(|c| {
            let mut n = 0;
            while it.peek() == Some(&c) {
                it.next();
                n += 1;
            }
            n > 0
        }) && it.next().is_none()
    };
    Some(match w {
        "lol" | "lul" | "lmao" | "lmfao" | "rofl" | "kek" | "yay" | "juhu" | "hurra" | "woohoo" => "joy",
        "rip" | "sad" | "schade" | "traurig" | "sadly" | "crying" => "sad",
        "angry" | "wütend" | "grr" | "argh" => "anger",
        "scary" | "gruselig" | "creepy" | "yikes" | "hilfe" => "fear",
        "omg" | "omfg" | "wow" | "whoa" | "woah" | "krass" | "wtf" | "wat" => "surprise",
        "ew" | "eww" | "igitt" | "yuck" | "gross" | "bäh" => "disgust",
        "ez" | "lame" | "cringe" => "contempt",
        "beautiful" | "gorgeous" | "wunderschön" | "majestic" => "awe",
        _ if laugh(w) => "joy",
        _ if stretched(w, "no") && w.len() >= 4 => "sad",
        _ if stretched(w, "nein") => "sad",
        _ if stretched(w, "yay") || stretched(w, "wow") => "joy",
        _ if stretched(w, "ew") => "disgust",
        _ if stretched(w, "grr") || stretched(w, "argh") => "anger",
        _ => return None,
    })
}

/// The feeling of a message, or None. `max` caps the intensity ("mild",
/// "normal" or "strong").
pub fn detect(a: &Analysis, raw: &str, max: &str) -> Option<Cue> {
    let mut score: Vec<(&'static str, f32)> = Vec::new();
    let mut add = |e: &'static str, w: f32| match score.iter_mut().find(|(x, _)| *x == e) {
        Some((_, s)) => *s += w,
        None => score.push((e, w)),
    };
    let mut seen_emotes: Vec<(String, u32)> = Vec::new();
    // the same emote ten times is one strong cue, not ten
    let mut emote = |name: &str, e: Option<&'static str>, add: &mut dyn FnMut(&'static str, f32)| {
        let n = match seen_emotes.iter_mut().find(|(x, _)| *x == name) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                seen_emotes.push((name.to_string(), 1));
                1
            }
        };
        if n <= 3
            && let Some(e) = e
        {
            add(e, 1.0);
        }
    };
    // letters of the words (not emote names) for telling shouting
    let (mut letters, mut upper) = (0usize, 0usize);
    for p in &a.parts {
        match p {
            Part::Emote(name) => emote(name, emote_by_name(name), &mut add),
            Part::Emoji(g) => {
                if let Some(e) = emoji(g) {
                    add(e, 1.0);
                }
            }
            Part::Text(t) => {
                // well-known emote names typed without the emote ("KEKW")
                let token = t.trim_end_matches(['!', '?', '.', ',']);
                if let Some((n, e)) = EMOTES.iter().find(|(n, _)| *n == token) {
                    emote(n, Some(e), &mut add);
                    continue;
                }
                for w in t.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()) {
                    letters += w.chars().count();
                    upper += w.chars().filter(|c| c.is_uppercase()).count();
                    if let Some(e) = word(&w.to_lowercase()) {
                        add(e, 0.8);
                    }
                }
            }
            _ => {}
        }
    }
    if raw.contains("?!") || raw.contains("!?") {
        add("surprise", 0.8);
    }
    score.sort_by(|x, y| y.1.total_cmp(&x.1));
    let &(emotion, best) = score.first()?;
    if best < 0.8 || score.get(1).is_some_and(|s| s.1 >= best * 0.6) {
        return None;
    }
    // shouting and exclamation marks make it stronger
    let shouting = letters >= 6 && upper * 10 >= letters * 7;
    let bangs = raw.split(|c| c != '!').map(str::len).max().unwrap_or(0);
    let s = best + if shouting { 1.0 } else { 0.0 } + if bangs >= 3 { 1.0 } else if bangs == 2 { 0.5 } else { 0.0 };
    let level = if s < 1.5 { 0 } else if s < 2.5 { 1 } else { 2 };
    let cap = LEVELS.iter().position(|l| *l == max).unwrap_or(2);
    Some(Cue { emotion, intensity: LEVELS[level.min(cap)] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(text: &str) -> Option<(&'static str, &'static str)> {
        let emotes = ["KEKW", "Sadge", "PogChamp", "monkaS", "catSadge", "pepeLaughing", "Kappa"];
        let a = super::super::analyze(text, text, "", 0, &|w| emotes.contains(&w));
        detect(&a, text, "strong").map(|c| (c.emotion, c.intensity))
    }

    #[test]
    fn reads_the_feeling_from_cues() {
        assert_eq!(cue("that boss fight KEKW"), Some(("joy", "mild")));
        assert_eq!(cue("KEKW KEKW KEKW"), Some(("joy", "strong")));
        assert_eq!(cue("hahaha that was great"), Some(("joy", "mild")));
        assert_eq!(cue("rip 😭"), Some(("sad", "normal")));
        assert_eq!(cue("nooooo my run"), Some(("sad", "mild")));
        assert_eq!(cue("catSadge"), Some(("sad", "mild")));
        assert_eq!(cue("pepeLaughing"), Some(("joy", "mild")));
        assert_eq!(cue("WHAT?!"), Some(("surprise", "mild")));
        assert_eq!(cue("NO WAY THAT JUST HAPPENED PogChamp !!!"), Some(("surprise", "strong")));
        assert_eq!(cue("monkaS behind you"), Some(("fear", "mild")));
        assert_eq!(cue("eww 🤢"), Some(("disgust", "normal")));
        // known emote names count when the channel doesn't have the emote
        assert_eq!(cue("LULW LULW that was hilarious"), Some(("joy", "normal")), "typed emote names are no shouting");
        assert_eq!(cue("4Head"), Some(("joy", "mild")));
        assert_eq!(cue("D:"), Some(("fear", "mild")));
        assert_eq!(cue("love you <3"), Some(("joy", "mild")));
    }

    #[test]
    fn plain_or_mixed_messages_stay_neutral() {
        assert_eq!(cue("hello everyone, how is the stream going"), None);
        assert_eq!(cue("Kappa"), None);
        assert_eq!(cue("haha 😭"), None, "joy and sadness at once");
        assert_eq!(cue("haha but also sad 😭"), Some(("sad", "normal")), "sadness wins clearly");
        assert_eq!(cue("no"), None);
        assert_eq!(cue("now"), None);
        assert_eq!(cue("hehe"), Some(("joy", "mild")));
        assert_eq!(cue("he"), None);
    }

    #[test]
    fn the_profile_caps_the_strength() {
        let a = super::super::analyze("KEKW KEKW KEKW", "KEKW KEKW KEKW", "", 0, &|w| w == "KEKW");
        assert_eq!(detect(&a, "KEKW KEKW KEKW", "mild").unwrap().intensity, "mild");
        assert_eq!(detect(&a, "KEKW KEKW KEKW", "normal").unwrap().intensity, "normal");
    }
}
