//! Texts for computing voices (from TTV-TTS 1.0's Voice Lab): the sentence a
//! new voice speaks in its reference sample (~6–8 s, neutral), the accent
//! instruction added to the description, and a second sentence for priming.

pub struct DesignLang {
    pub code: &'static str,
    pub vcp: &'static str,
    /// The speech check uses VCP's speech recognition for this language.
    pub asr: bool,
    pub accent: &'static str,
    pub text: &'static str,
    pub verify: &'static str,
    /// Typical speaking rate of designed voices (letters or CJK characters
    /// per second of talking): median of 420 real VCP voices (2026-11-01).
    pub letters_per_sec: f32,
}

pub const LANGS: &[DesignLang] = &[
    DesignLang {
        code: "en",
        vcp: "English",
        asr: true,
        accent: "Native speaker of American English with a natural General American accent.",
        text: "Hey everyone, it's great to see you here again today. Let's see what else we're going to experience in the stream.",
        verify: "Thanks for the follow, that really means a lot to me!",
        letters_per_sec: 14.2,
    },
    DesignLang {
        code: "de",
        vcp: "German",
        asr: true,
        accent: "Native German speaker with standard High German pronunciation and no foreign accent.",
        text: "Hallo zusammen, schön dass ihr heute wieder dabei seid. Mal sehen, was wir im Stream heute noch alles erleben werden.",
        verify: "Danke für den Follow, das freut mich wirklich sehr!",
        letters_per_sec: 14.7,
    },
    DesignLang {
        code: "ru",
        vcp: "Russian",
        asr: true,
        accent: "Native Russian speaker with standard Moscow pronunciation and no foreign accent.",
        text: "Всем привет, как здорово снова видеть вас здесь. Посмотрим, что ещё интересного произойдёт сегодня на стриме.",
        verify: "Спасибо за подписку, мне это очень приятно!",
        letters_per_sec: 12.3,
    },
    DesignLang {
        code: "fr",
        vcp: "French",
        asr: true,
        accent: "Native French speaker from France with standard Parisian pronunciation and no foreign accent.",
        text: "Salut tout le monde, c'est super de vous revoir ici aujourd'hui. Voyons ce qui va encore se passer pendant le stream.",
        verify: "Merci pour le follow, ça me fait vraiment plaisir !",
        letters_per_sec: 15.0,
    },
    DesignLang {
        code: "es",
        vcp: "Spanish",
        asr: true,
        accent: "Native Spanish speaker with a neutral, standard accent and no foreign accent.",
        text: "Hola a todos, qué bueno verlos de nuevo por aquí hoy. Vamos a ver qué más pasa hoy en el directo.",
        verify: "Gracias por el follow, ¡me alegra muchísimo!",
        letters_per_sec: 12.7,
    },
    DesignLang {
        code: "it",
        vcp: "Italian",
        asr: true,
        accent: "Native Italian speaker with standard Italian pronunciation and no foreign accent.",
        text: "Ciao a tutti, che bello rivedervi qui anche oggi. Vediamo cos'altro succederà oggi nella live.",
        verify: "Grazie per il follow, mi fa davvero piacere!",
        letters_per_sec: 12.5,
    },
    DesignLang {
        code: "pt",
        vcp: "Portuguese",
        asr: true,
        accent: "Native Brazilian Portuguese speaker with a natural Brazilian accent.",
        text: "Olá a todos, que bom ver vocês aqui de novo hoje. Vamos ver o que mais vai acontecer na live.",
        verify: "Valeu pelo follow, isso me deixa muito feliz!",
        letters_per_sec: 11.1,
    },
    DesignLang {
        code: "ja",
        vcp: "Japanese",
        asr: false,
        accent: "Native Japanese speaker with standard Tokyo pronunciation.",
        text: "みなさん、こんにちは。今日もまた来てくれてうれしいです。配信でこれから何が起きるか楽しみですね。",
        verify: "フォローありがとうございます、本当にうれしいです！",
        letters_per_sec: 5.6,
    },
    DesignLang {
        code: "zh",
        vcp: "Chinese",
        asr: false,
        accent: "Native Mandarin Chinese speaker with standard Beijing pronunciation.",
        text: "大家好，很高兴今天又在这里见到你们。让我们看看今天的直播还会发生什么吧。",
        verify: "谢谢你的关注，我真的很开心！",
        letters_per_sec: 4.8,
    },
    DesignLang {
        code: "ko",
        vcp: "Korean",
        asr: false,
        accent: "Native Korean speaker with standard Seoul pronunciation.",
        text: "여러분 안녕하세요, 오늘도 다시 만나서 정말 반가워요. 오늘 방송에서 또 어떤 일이 일어날지 기대돼요.",
        verify: "팔로우해 주셔서 감사합니다, 정말 기뻐요!",
        letters_per_sec: 5.3,
    },
];

pub fn get(code: &str) -> Option<&'static DesignLang> {
    LANGS.iter().find(|l| l.code == code)
}

/// Word error rate (characters for Japanese/Chinese) of `heard` against `reference`.
pub fn error_rate(reference: &str, heard: &str, lang: &str) -> f32 {
    let norm = |s: &str| -> String {
        s.to_lowercase().replace('ё', "е").chars().map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' }).collect()
    };
    let by_char = matches!(lang, "ja" | "zh");
    let tokens = |s: &str| -> Vec<String> {
        let n = norm(s);
        if by_char { n.chars().filter(|c| !c.is_whitespace()).map(|c| c.to_string()).collect() } else { n.split_whitespace().map(str::to_string).collect() }
    };
    let a = tokens(reference);
    let b = tokens(heard);
    if a.is_empty() {
        return if b.is_empty() { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
        prev = cur;
    }
    prev[b.len()] as f32 / a.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_rates() {
        assert_eq!(error_rate("Hello there my friend", "hello there my friend!", "en"), 0.0);
        assert!((error_rate("one two three four", "one two tree four", "en") - 0.25).abs() < 1e-6);
        assert!(error_rate("大家好", "大家好", "zh") == 0.0);
        assert_eq!(error_rate("Ёлка", "елка", "ru"), 0.0);
    }

    #[test]
    fn ten_languages() {
        assert_eq!(LANGS.len(), 10);
        assert!(get("de").is_some() && get("xx").is_none());
    }
}
