//! Hostile chat: anybody can type anything into a Twitch chat, and a panic
//! ends the whole app (release builds abort). Every text path must survive
//! odd input: multi-byte punctuation at cut points, emoji sequences,
//! combining marks, control characters, broken emote tags, broken IRC lines.

use crate::profile::Profile;
use crate::text;

/// Small deterministic generator (xorshift), so a failure repeats.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn pick<'a>(&mut self, v: &[&'a str]) -> &'a str {
        v[(self.next() % v.len() as u64) as usize]
    }
}

const BITS: &[&str] = &[
    "… ", "…", "。", "！", "？ ", ". ", "! ", "? ", ", ", "; ", ": ", "、", "，", " ", "  ", "\t", "\u{00A0}", "\u{200B}", "\u{200D}", "\u{FEFF}", "\u{202E}",
    "😂", "👍🏽", "👨‍👩‍👧‍👦", "🇩🇪", "#️⃣", "1️⃣", "e\u{0301}", "Z\u{0336}\u{0335}\u{0334}", "ß", "İ", "ǅ", "ﬃ", "日本語", "한국어", "русский", "العربية", "עברית", "हिन्दी", "ไทย",
    "KEKW", "Kappa", "PogChamp", "xddd", "hahaha", "jajaja", "@user", "@", "@😂", "@ä,", "https://x.y/…", "www.ä.de", "a.com", "cheer100", "Cheer1",
    "aaaaaaaaaaaa", "!!!!!", "????", ",,,,", "......", "-", "_", ":", "\\", "\"", "'", "<b>", "&amp;", "%s", "{}", "\u{0}", "\u{7}", "\r", "\n", "\u{1F}",
    "\u{10FFFF}", "\u{E000}", "\u{FFFD}", "x", "Hallo", "Welt", "hello", "WORLD", "123", "4️", "½", "²", "Ⅻ",
];

fn hostile(r: &mut Rng, parts: usize) -> String {
    (0..parts).map(|_| r.pick(BITS)).collect()
}

#[test]
fn shorten_survives_every_cut_point() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..4000 {
        let len = 1 + (r.next() % 120) as usize;
        let s = hostile(&mut r, len);
        for max in [0, 1, 2, 3, 5, 8, 13, 20, 40, 100] {
            let out = text::shorten(&s, max);
            assert!(out.chars().count() <= max + 2, "{max}: {out:?}");
        }
    }
    // the case that crashed: "… " in the second half of the cut
    assert_eq!(text::shorten("aaaaaaaa… bbbbbbbbbbbbbbbbbbbbbbbbbbb", 12), "aaaaaaaa…");
}

#[test]
fn the_text_pipeline_survives_hostile_messages() {
    let mut r = Rng(0xDEAD_BEEF_CAFE_F00D);
    let emotes = ["KEKW", "Kappa", "PogChamp"];
    let mut profiles = Vec::new();
    for max in [20, 33, 57, 400] {
        let mut p = Profile::default();
        p.reading.max_chars = max;
        profiles.push(p);
    }
    for i in 0..1000 {
        let len = 1 + (r.next() % 160) as usize;
        let s = hostile(&mut r, len);
        // broken emote tags (positions past the end, reversed, garbage)
        let n = s.chars().count();
        let tag = match i % 5 {
            0 => String::new(),
            1 => format!("25:0-{}", n.saturating_sub(1)),
            2 => format!("25:{}-{},3-1/1:x-y,/:,{}-{}", n, n + 5, usize::MAX, usize::MAX),
            3 => "25:0-4,12-16/1902:6-10".to_string(),
            _ => {
                let (a, b) = (r.next() % 50, r.next() % 50);
                format!("1:{a}-{b}")
            }
        };
        let bits = (r.next() % 3) as u32 * 100;
        let a = text::analyze(&s, &s, &tag, bits, &|w| emotes.contains(&w));
        let _ = text::emotion::detect(&a, &s, "strong");
        let _ = text::lang::detect(&s);
        let _ = text::collapse_repeats(&s);
        let (from, to) = (r.pick(BITS), r.pick(BITS));
        let _ = text::replace_words(&s, from, to);
        for p in &profiles {
            for lang in ["en", "de", "ja", "xx", ""] {
                if let Some(t) = text::render(&a, lang, p) {
                    let _ = crate::speech::split(&t);
                }
            }
        }
        let _ = crate::speech::split(&s);
    }
}

#[test]
fn irc_lines_never_panic() {
    let mut r = Rng(0x0123_4567_89AB_CDEF);
    let prefixes = ["", "@", "@a=b;c", "@a=b :", ":", ": ", "@a=\\s\\:\\\\\\ :x!x@x PRIVMSG #c :", "@emotes=1:0-99;display-name=😂;user-id= :u!u@u PRIVMSG #c :", "PING", "PING :", ":tmi USERNOTICE #c", "@msg-id=raid;msg-param-viewerCount=x :tmi USERNOTICE #c :"];
    for _ in 0..3000 {
        let len = (r.next() % 40) as usize;
        let head = r.pick(&prefixes);
        let line = format!("{head}{}", hostile(&mut r, len));
        let _ = crate::twitch::irc::parse(&line);
    }
}

#[test]
fn commands_are_stripped_without_cutting_characters() {
    use crate::reader::strip_command;
    assert_eq!(strip_command("!TTS hello", "!tts"), Some("hello"));
    assert_eq!(strip_command("!tts", "!tts"), None, "nothing after it");
    assert_eq!(strip_command("!ttsx hi", "!tts"), None);
    assert_eq!(strip_command("!Sprich hallo", "!sprich"), Some("hallo"));
    // the Kelvin sign lowercases to "k" with another length
    assert_eq!(strip_command("!spea\u{212A} hi", "!speak"), None);
    assert_eq!(strip_command("!speaK hi", "!speak"), Some("hi"), "a plain capital K");
    assert_eq!(strip_command("!spea\u{212A}\u{212A} hi", "!speak"), None);
    assert_eq!(strip_command("İİ hi", "!ii"), None);
    let mut r = Rng(0x5151_5151_5151_5151);
    for _ in 0..3000 {
        let len = (r.next() % 12) as usize;
        let s = hostile(&mut r, len);
        for cmd in ["!tts", "!speak", "!ß", "!日本", "!", "k"] {
            let _ = strip_command(&s, cmd);
            let _ = strip_command(&format!("{cmd}{s}"), cmd);
        }
    }
}
