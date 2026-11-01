//! Emoji words for `EmojiMode::Describe` (de, en, ru; from TTV-TTS 1.0).
//! Other languages drop emojis.

/// emoji -> [German, English, Russian]
const WORDS: &[(&str, [&str; 3])] = &[
    ("😂", ["lachend", "laughing", "смеётся"]), ("🤣", ["lachend", "laughing", "смеётся"]), ("😆", ["lachend", "laughing", "смеётся"]),
    ("😹", ["lachend", "laughing", "смеётся"]), ("😭", ["weinend", "crying", "плачет"]), ("😢", ["traurig", "sad", "грустно"]),
    ("😞", ["traurig", "sad", "грустно"]), ("😔", ["traurig", "sad", "грустно"]), ("🙁", ["traurig", "sad", "грустно"]),
    ("☹", ["traurig", "sad", "грустно"]), ("😀", ["grinsend", "grinning", "улыбается"]), ("😃", ["grinsend", "grinning", "улыбается"]),
    ("😄", ["grinsend", "grinning", "улыбается"]), ("😁", ["grinsend", "grinning", "улыбается"]), ("😊", ["lächelnd", "smiling", "улыбается"]),
    ("🙂", ["lächelnd", "smiling", "улыбается"]), ("😅", ["verlegen lachend", "nervous laugh", "нервно смеётся"]), ("😉", ["zwinker", "wink", "подмигивает"]),
    ("😏", ["grinsend", "smirk", "ухмыляется"]), ("😍", ["verliebt", "heart eyes", "влюблён"]), ("🥰", ["verliebt", "in love", "влюблён"]),
    ("😘", ["Küsschen", "kiss", "поцелуй"]), ("😎", ["cool", "cool", "круто"]), ("🤔", ["hmm", "hmm", "хмм"]), ("😮", ["oh", "oh", "ох"]),
    ("😯", ["oh", "oh", "ох"]), ("😲", ["wow", "wow", "вау"]), ("😱", ["schockiert", "shocked", "в шоке"]), ("😳", ["oha", "whoa", "ого"]),
    ("😡", ["wütend", "angry", "злится"]), ("😠", ["wütend", "angry", "злится"]), ("🤬", ["fluchend", "cursing", "ругается"]),
    ("😤", ["schnaubt", "huff", "фыркает"]), ("🙄", ["Augenrollen", "eye roll", "закатывает глаза"]), ("😴", ["schläft", "sleeping", "спит"]),
    ("🥱", ["gähn", "yawn", "зевает"]), ("🤯", ["Kopf explodiert", "mind blown", "взрыв мозга"]), ("🥺", ["bitte bitte", "pretty please", "ну пожалуйста"]),
    ("😬", ["autsch", "yikes", "ой"]), ("😇", ["unschuldig", "innocent", "невинно"]), ("🤗", ["Umarmung", "hug", "обнимашки"]), ("🤫", ["psst", "shh", "тсс"]),
    ("🤢", ["bäh", "eww", "фу"]), ("🤮", ["bäh", "eww", "фу"]), ("🥶", ["eiskalt", "freezing", "холодно"]), ("🥵", ["heiß", "hot", "жарко"]),
    ("😋", ["lecker", "yummy", "вкусно"]), ("🤑", ["Geld", "money", "деньги"]), ("🤡", ["Clown", "clown", "клоун"]), ("💀", ["tot", "dead", "умер"]),
    ("☠", ["tot", "dead", "умер"]), ("👻", ["Geist", "ghost", "призрак"]), ("🔥", ["Feuer", "fire", "огонь"]), ("💯", ["hundert Prozent", "hundred percent", "сто процентов"]),
    ("❤", ["Herz", "heart", "сердечко"]), ("🧡", ["Herz", "heart", "сердечко"]), ("💛", ["Herz", "heart", "сердечко"]), ("💚", ["Herz", "heart", "сердечко"]),
    ("💙", ["Herz", "heart", "сердечко"]), ("💜", ["Herz", "heart", "сердечко"]), ("🖤", ["Herz", "heart", "сердечко"]), ("🤍", ["Herz", "heart", "сердечко"]),
    ("💖", ["Herzen", "hearts", "сердечки"]), ("💕", ["Herzen", "hearts", "сердечки"]), ("💗", ["Herzen", "hearts", "сердечки"]), ("💞", ["Herzen", "hearts", "сердечки"]),
    ("💔", ["gebrochenes Herz", "broken heart", "разбитое сердце"]), ("👍", ["Daumen hoch", "thumbs up", "класс"]), ("👎", ["Daumen runter", "thumbs down", "не нравится"]),
    ("👏", ["Applaus", "applause", "аплодисменты"]), ("🙏", ["danke", "thanks", "спасибо"]), ("💪", ["stark", "strong", "сила"]), ("👀", ["Augen", "eyes", "глаза"]),
    ("👋", ["winke winke", "wave", "привет"]), ("🤝", ["Handschlag", "handshake", "рукопожатие"]), ("🤷", ["Schulterzucken", "shrug", "пожимает плечами"]),
    ("🤦", ["Facepalm", "facepalm", "фейспалм"]), ("🫡", ["Salut", "salute", "салют"]), ("🙈", ["upsi", "oops", "упс"]), ("🎉", ["Party", "party", "ура"]),
    ("🥳", ["Party", "party", "ура"]), ("✨", ["glitzer", "sparkles", "блеск"]), ("⭐", ["Stern", "star", "звезда"]), ("🌟", ["Stern", "star", "звезда"]),
    ("🚀", ["Rakete", "rocket", "ракета"]), ("🤖", ["Roboter", "robot", "робот"]), ("🐐", ["GOAT", "GOAT", "легенда"]), ("🍿", ["Popcorn", "popcorn", "попкорн"]),
    ("🍺", ["Bier", "beer", "пиво"]), ("🍻", ["Prost", "cheers", "за здоровье"]), ("☕", ["Kaffee", "coffee", "кофе"]), ("🍕", ["Pizza", "pizza", "пицца"]),
    ("🎮", ["Gaming", "gaming", "игры"]), ("✅", ["Haken", "check", "галочка"]), ("❌", ["nein", "no", "нет"]), ("⚠", ["Achtung", "warning", "внимание"]),
];

/// Word for an emoji grapheme in `lang`, or "" (unknown emoji / language).
pub fn word(grapheme: &str, lang: &str) -> &'static str {
    let idx = match lang {
        "de" => 0,
        "en" => 1,
        "ru" => 2,
        _ => return "",
    };
    // Drop variation selectors and skin tones before looking it up.
    let plain: String = grapheme.chars().filter(|c| !matches!(*c as u32, 0xFE0F | 0x1F3FB..=0x1F3FF)).collect();
    let first: String = plain.chars().take(1).collect();
    WORDS.iter().find(|(e, _)| *e == plain || *e == first).map(|(_, w)| w[idx]).unwrap_or("")
}
