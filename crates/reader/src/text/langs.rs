//! Per-language data: VCP's language name, the name announcement, and the
//! words spoken for links and blocked words (from TTV-TTS 1.0).

pub struct Lang {
    pub code: &'static str,
    /// Language name VCP expects.
    pub vcp: &'static str,
    /// Name announcement: {user} and {msg}.
    pub say: &'static str,
    pub link: &'static str,
    pub beep: &'static str,
}

pub const LANGS: &[Lang] = &[
    Lang { code: "de", vcp: "German", say: "{user} sagt: {msg}", link: "Link", beep: "piep" },
    Lang { code: "en", vcp: "English", say: "{user} says: {msg}", link: "link", beep: "beep" },
    Lang { code: "ru", vcp: "Russian", say: "{user} пишет: {msg}", link: "ссылка", beep: "пип" },
    Lang { code: "uk", vcp: "Ukrainian", say: "{user} пише: {msg}", link: "посилання", beep: "піп" },
    Lang { code: "fr", vcp: "French", say: "{user} dit : {msg}", link: "lien", beep: "bip" },
    Lang { code: "es", vcp: "Spanish", say: "{user} dice: {msg}", link: "enlace", beep: "pip" },
    Lang { code: "it", vcp: "Italian", say: "{user} dice: {msg}", link: "link", beep: "bip" },
    Lang { code: "pt", vcp: "Portuguese", say: "{user} diz: {msg}", link: "link", beep: "bip" },
    Lang { code: "nl", vcp: "Dutch", say: "{user} zegt: {msg}", link: "link", beep: "piep" },
    Lang { code: "pl", vcp: "Polish", say: "{user} pisze: {msg}", link: "link", beep: "pip" },
    Lang { code: "tr", vcp: "Turkish", say: "{user} diyor ki: {msg}", link: "bağlantı", beep: "bip" },
    Lang { code: "ja", vcp: "Japanese", say: "{user}さん：{msg}", link: "リンク", beep: "ピー" },
    Lang { code: "zh", vcp: "Chinese", say: "{user}说：{msg}", link: "链接", beep: "哔" },
    Lang { code: "ko", vcp: "Korean", say: "{user}님: {msg}", link: "링크", beep: "삐" },
];

pub fn get(code: &str) -> Option<&'static Lang> {
    LANGS.iter().find(|l| l.code == code)
}

/// Code for a static string (keeps detected codes `'static`).
pub fn intern(code: &str) -> &'static str {
    get(code).map(|l| l.code).unwrap_or("en")
}

/// VCP language name for `code`, if the model speaks it (`supported` = the
/// active model's languages, lower case; None = unknown, assume yes).
/// Otherwise the profile's fallback, else "Auto".
pub fn vcp_name(code: &str, supported: Option<&[String]>, unsupported: &str) -> String {
    let ok = |name: &str| supported.is_none_or(|s| s.iter().any(|x| x.eq_ignore_ascii_case(name)));
    if let Some(l) = get(code)
        && ok(l.vcp)
    {
        return l.vcp.to_string();
    }
    if unsupported != "auto"
        && unsupported != code
        && let Some(l) = get(unsupported)
        && ok(l.vcp)
    {
        return l.vcp.to_string();
    }
    "Auto".into()
}
