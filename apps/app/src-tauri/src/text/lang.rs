//! Offline language detection for short chat messages (from TTV-TTS 1.0):
//! 1. writing system (Cyrillic, CJK, …), 2. typical words, 3. special letters.
//! Short or ambiguous messages ("gg", "ok") return None; the caller then uses
//! the viewer's last detected language or the profile default.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const STOPWORDS: &[(&str, &str)] = &[
    ("de", "ich du er sie wir ihr die war mich dich mir dir uns euch sich mein meine dein deine seine ihre unser und oder aber denn nicht kein keine nichts ist bist sind seid waren wird werden wurde hab habe hast haben hatte kann kannst können muss musst müssen willst wollen soll sollte würde wäre das der den dem des ein eine einen einem einer wer wie wo wann warum wieso weshalb woher ja nein nee doch auch noch schon mal jetzt heute morgen gestern gut schlecht geil krass digga alter bitte danke hallo moin servus tschüss mit von zu zum zur für auf aus bei nach über unter vor ohne gegen wenn dann weil dass ob als sehr viel mehr weniger nur gibt geht gehts gerade grad echt halt eben wieder immer oft hier dort leute spiel spielen machen macht gemacht sagen weiß glaube finde schön richtig einfach ganz bisschen genau stimmt natürlich vielleicht ihn ihm habt seh sieht guten tag abend nacht"),
    ("en", "i you he she it we they me my your his her our their them the and or but not yes yeah yep nope am are were be been being have has had do does did dont doesnt didnt cant wont isnt im ive youre its thats theres this that these those what who how where why which there here just now today tomorrow yesterday good bad great nice cool awesome please thanks thank thx hello hi bye with of to for on at from by about into if then because than very really much more less only too lol lmao lmfao omg wtf bro dude guys play playing game games gonna wanna gotta can could would should know think love like get got go going see look make want need right time"),
    ("fr", "je tu il elle nous vous ils elles moi toi le les une du et ou ne pas plus rien est suis sommes êtes sont était été avoir ai avons avez ont fait faire ce cet cette ces ça cela qui que quoi quel quelle comment où pourquoi quand oui ouais aussi encore déjà maintenant aujourd demain bien bonne merci bonjour bonsoir salut coucou avec pour sur dans par alors parce très trop beaucoup mon ma mes ton ta tes sa ses notre votre leur cest jai nest quil jsuis mdr ptdr jeu"),
    ("es", "yo tú él ella nosotros ellos usted los las el una unos unas y pero sí soy eres somos está estoy estás están fue ser estar hemos han hay tengo tiene tienen hacer hace qué quién cómo dónde donde cuándo cuando porque muy mucho más menos también ya ahora hoy mañana ayer bien bueno buena gracias hola adiós para sobre en del mis tus sus nuestro jaja jajaja vamos juego eso esto"),
    ("it", "io lui lei noi voi loro ci gli uno ma non è sono sei siamo siete ho hai abbiamo hanno fare fatto che chi dove quando perché cosa anche già ora adesso oggi domani ieri bene buono grazie ciao salve per nel nella della dei delle alla mio mia tuo tua suo molto più meno troppo questo questa quello quella cè sì allora quindi però gioco ragazzi"),
    ("pt", "eu você voce vocês ele ela nós eles elas os as uma umas ou mas não sim é sou somos são estou estão foi tenho tem temos têm fazer faz quem onde porque muito mais também já agora hoje amanhã ontem bem bom boa obrigado obrigada olá oi tchau com pra em do dos das meu minha seu sua isso isto aquilo aqui ali kkk kkkk rs galera jogo"),
    ("nl", "ik jij hij zij wij jullie mij mijn jouw zijn haar ons onze het de een en of maar niet geen wel is ben bent waren heb hebt heeft hebben wat hoe waar wanneer waarom nee ook nog nu vandaag gisteren goed mooi leuk dank bedankt hoi doei met van voor op uit bij naar over dan omdat dat dit deze heel veel meer alleen toch even gewoon echt spel jongens"),
    ("pl", "ty ona ono my wy oni mnie cię go jej nas im mój moja twój twoja jest są był była było być mam masz ma mamy macie mają tak ale lub albo że co kto jak gdzie kiedy dlaczego czemu też już teraz dzisiaj dziś jutro wczoraj dobrze dobry dobra dzięki dziękuję cześć siema elo we od po przez dla bez ten tego bardzo dużo więcej mniej tylko jeszcze może można trzeba gra xd"),
    ("tr", "sen biz siz onlar beni seni bana sana bu şu bir ve veya ama değil evet hayır yok mı mu mü kim nasıl nerede neden niye çünkü ki ile için gibi çok daha az sadece şimdi bugün yarın dün iyi güzel teşekkürler teşekkür sağol merhaba selam naber abi kanka oyun yap olur oldu"),
];

/// (language, letters, weight): letters typical for a language add to its score.
const CHAR_HINTS: &[(&str, &str, u32)] = &[
    ("de", "äöüß", 2),
    ("tr", "şğı", 3),
    ("pl", "ąęłńśźżć", 3),
    ("es", "ñ¿¡", 3),
    ("es", "áíóú", 1),
    ("pt", "ãõ", 3),
    ("pt", "âêôç", 1),
    ("fr", "èêëîïûœàç", 1),
    ("it", "ìòù", 2),
    ("it", "àè", 1),
];

fn sets() -> &'static Vec<(&'static str, HashSet<&'static str>)> {
    static S: OnceLock<Vec<(&'static str, HashSet<&'static str>)>> = OnceLock::new();
    S.get_or_init(|| STOPWORDS.iter().map(|(l, w)| (*l, w.split(' ').collect())).collect())
}

#[derive(Default)]
struct Scripts {
    latin: u32,
    cyrillic: u32,
    hangul: u32,
    kana: u32,
    han: u32,
    greek: u32,
    arabic: u32,
    hebrew: u32,
    thai: u32,
    devanagari: u32,
}

fn scripts(text: &str) -> Scripts {
    let mut s = Scripts::default();
    for c in text.chars() {
        match c as u32 {
            0x41..=0x5A | 0x61..=0x7A | 0xC0..=0x24F | 0x1E00..=0x1EFF => s.latin += 1,
            0x400..=0x4FF | 0x500..=0x52F => s.cyrillic += 1,
            0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => s.hangul += 1,
            0x3040..=0x30FF | 0x31F0..=0x31FF => s.kana += 1,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF => s.han += 1,
            0x370..=0x3FF => s.greek += 1,
            0x600..=0x6FF | 0x750..=0x77F => s.arabic += 1,
            0x590..=0x5FF => s.hebrew += 1,
            0xE00..=0xE7F => s.thai += 1,
            0x900..=0x97F => s.devanagari += 1,
            _ => {}
        }
    }
    s
}

/// Detects the language of `text` (ISO 639-1), or None if unsure.
pub fn detect(text: &str) -> Option<&'static str> {
    let s = scripts(text);
    let ranked = [
        (s.latin, "latin"),
        (s.cyrillic, "cyrillic"),
        (s.hangul, "hangul"),
        (s.kana, "kana"),
        (s.han, "han"),
        (s.greek, "greek"),
        (s.arabic, "arabic"),
        (s.hebrew, "hebrew"),
        (s.thai, "thai"),
        (s.devanagari, "devanagari"),
    ];
    let (count, top) = ranked.iter().copied().max_by_key(|(n, _)| *n)?;
    if count == 0 {
        return None;
    }
    match top {
        "cyrillic" => return Some(if text.chars().any(|c| matches!(c, 'і' | 'ї' | 'є' | 'ґ' | 'І' | 'Ї' | 'Є' | 'Ґ')) { "uk" } else { "ru" }),
        "hangul" => return Some("ko"),
        "kana" => return Some("ja"),
        "han" => return Some(if s.kana > 0 { "ja" } else { "zh" }),
        "greek" => return Some("el"),
        "arabic" => return Some("ar"),
        "hebrew" => return Some("he"),
        "thai" => return Some("th"),
        "devanagari" => return Some("hi"),
        _ => {}
    }
    // Latin script: count typical words, add letter hints.
    let lower = text.to_lowercase().replace(['\'', '’', '`'], "");
    let mut scores: Vec<(&str, u32)> = sets().iter().map(|(l, _)| (*l, 0)).collect();
    for w in lower.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()) {
        for (i, (_, set)) in sets().iter().enumerate() {
            if set.contains(w) {
                scores[i].1 += 1;
            }
        }
    }
    let mut bonus: HashMap<&str, u32> = HashMap::new();
    for (lang, letters, weight) in CHAR_HINTS {
        let n = lower.chars().filter(|c| letters.contains(*c)).count() as u32;
        *bonus.entry(lang).or_insert(0) += n * weight;
    }
    for (lang, b) in bonus {
        if let Some(s) = scores.iter_mut().find(|(l, _)| *l == lang) {
            s.1 += b.min(4);
        }
    }
    scores.sort_by(|a, b| b.1.cmp(&a.1));
    let (best, second) = (scores[0], scores[1]);
    if best.1 < 1 || best.1 == second.1 { None } else { Some(best.0) }
}

/// Remembers each viewer's last detected language for a while, so short
/// messages ("gg", "ok") keep the language the viewer usually writes in.
pub struct LangMemory {
    ttl: Duration,
    map: HashMap<String, (&'static str, Instant)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    Detected,
    Remembered,
    Default,
}

impl LangMemory {
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, map: HashMap::new() }
    }

    pub fn set_ttl(&mut self, ttl: Duration) {
        self.ttl = ttl;
    }

    pub fn choose(&mut self, text: &str, viewer: &str, default: &'static str) -> (&'static str, How) {
        let now = Instant::now();
        if let Some(l) = detect(text) {
            self.map.insert(viewer.to_string(), (l, now));
            return (l, How::Detected);
        }
        if let Some((l, t)) = self.map.get(viewer)
            && now.duration_since(*t) < self.ttl
        {
            return (l, How::Remembered);
        }
        (default, How::Default)
    }

    /// Drops expired entries (call now and then).
    pub fn prune(&mut self) {
        let ttl = self.ttl;
        self.map.retain(|_, (_, t)| t.elapsed() < ttl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_chat_languages() {
        assert_eq!(detect("Hallo zusammen, wie geht es euch heute?"), Some("de"));
        assert_eq!(detect("hey guys how are you doing today"), Some("en"));
        assert_eq!(detect("Привет всем, как дела?"), Some("ru"));
        assert_eq!(detect("Привіт, як справи? Їжак"), Some("uk"));
        assert_eq!(detect("salut tout le monde, ça va bien ?"), Some("fr"));
        assert_eq!(detect("hola a todos, ¿qué tal?"), Some("es"));
        assert_eq!(detect("ciao ragazzi, come state oggi"), Some("it"));
        assert_eq!(detect("oi galera, tudo bem com vocês?"), Some("pt"));
        assert_eq!(detect("こんにちは、みなさん"), Some("ja"));
        assert_eq!(detect("大家好"), Some("zh"));
        assert_eq!(detect("안녕하세요 여러분"), Some("ko"));
        assert_eq!(detect("gg"), None);
        assert_eq!(detect("ok"), None);
        assert_eq!(detect("123 !!!"), None);
    }

    #[test]
    fn memory_keeps_the_viewers_language() {
        let mut m = LangMemory::new(Duration::from_secs(900));
        assert_eq!(m.choose("Привет всем, как дела?", "anya", "en"), ("ru", How::Detected));
        assert_eq!(m.choose("gg", "anya", "en"), ("ru", How::Remembered));
        assert_eq!(m.choose("gg", "bob", "en"), ("en", How::Default));
    }
}
