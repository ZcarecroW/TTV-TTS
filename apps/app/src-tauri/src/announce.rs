//! Event announcements (optional, off by default – chat bots often do this
//! already): subs, resubs, gifted subs, raids and cheers arrive over the
//! anonymous chat connection, follows and rewards without text over EventSub
//! (Twitch login), and are read in the profile's language by an announcer
//! voice.

use crate::profile::Announcements;
use crate::twitch::irc::ChatMessage;

/// Event kinds and their built-in sentences: [kind, English, German].
const TEXTS: &[[&str; 3]] = &[
    ["sub", "{user} just subscribed!", "{user} hat gerade abonniert!"],
    ["resub", "{user} subscribed for {months} months!", "{user} ist seit {months} Monaten dabei!"],
    ["subgift", "{user} gifted a sub to {recipient}!", "{user} verschenkt ein Abo an {recipient}!"],
    ["submysterygift", "{user} gifted {count} subs to the community!", "{user} verschenkt {count} Abos an die Community!"],
    ["raid", "{user} is raiding with {viewers} viewers!", "{user} raidet mit {viewers} Zuschauern!"],
    ["bits", "{user} cheered {bits} bits!", "{user} hat {bits} Bits gespendet!"],
    ["follow", "{user} is now following!", "{user} folgt jetzt!"],
    ["redeem", "{user} redeemed {reward}!", "{user} hat {reward} eingelöst!"],
];

pub const KINDS: [&str; 8] = ["sub", "resub", "subgift", "submysterygift", "raid", "bits", "follow", "redeem"];

/// The built-in sentence for `kind` in `lang` (English for other languages).
pub fn builtin(kind: &str, lang: &str) -> &'static str {
    let row = TEXTS.iter().find(|r| r[0] == kind).unwrap_or(&TEXTS[0]);
    if lang == "de" { row[2] } else { row[1] }
}

/// What to announce for a chat event, if anything: (kind, sentence).
pub fn for_message(a: &Announcements, m: &ChatMessage, lang: &str) -> Option<(&'static str, String)> {
    if !a.enabled {
        return None;
    }
    let p = |k: &str| m.notice_params.get(k).cloned().unwrap_or_default();
    let kind: &'static str = match m.notice_type.as_deref() {
        Some("sub") if a.subs => "sub",
        Some("resub") if a.subs => "resub",
        // the single gifts of a community gift are announced as one
        Some("subgift" | "anonsubgift") if a.gifts && !m.notice_params.contains_key("community-gift-id") => "subgift",
        Some("submysterygift" | "anonsubmysterygift") if a.gifts => "submysterygift",
        Some("raid") if a.raids && p("viewerCount").parse::<u32>().unwrap_or(0) >= a.min_raid => "raid",
        Some("follow") if a.follows => "follow",
        Some("redeem") if a.redemptions => "redeem",
        None if a.bits && m.bits > 0 && m.bits >= a.min_bits => "bits",
        _ => return None,
    };
    let template = a.texts.get(kind).filter(|t| !t.trim().is_empty()).map(String::as_str).unwrap_or_else(|| builtin(kind, lang));
    let user = if m.notice_type.as_deref().is_some_and(|t| t.starts_with("anon")) {
        if lang == "de" { "Jemand Anonymes".to_string() } else { "Someone anonymous".to_string() }
    } else if kind == "raid" {
        let d = p("displayName");
        if d.is_empty() { m.display_name.clone() } else { d }
    } else {
        m.display_name.clone()
    };
    let months = { let c = p("cumulative-months"); if c.is_empty() { p("months") } else { c } };
    let text = template
        .replace("{user}", &user)
        .replace("{months}", &months)
        .replace("{recipient}", &p("recipient-display-name"))
        .replace("{count}", &p("mass-gift-count"))
        .replace("{viewers}", &p("viewerCount"))
        .replace("{bits}", &m.bits.to_string())
        .replace("{reward}", &p("reward"));
    Some((kind, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::twitch::irc::Kind;

    fn notice(kind: &str, params: &[(&str, &str)]) -> ChatMessage {
        ChatMessage {
            channel: "chan".into(),
            room_id: String::new(),
            id: "1".into(),
            login: "alice".into(),
            display_name: "Alice".into(),
            user_id: "11".into(),
            text: String::new(),
            action: false,
            roles: Default::default(),
            emotes: String::new(),
            bits: 0,
            reward_id: None,
            first_message: false,
            kind: Kind::Notice,
            notice_type: Some(kind.into()),
            system_msg: None,
            color: None,
            notice_params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    fn on() -> Announcements {
        Announcements { enabled: true, ..Default::default() }
    }

    #[test]
    fn announces_subs_gifts_raids_and_cheers() {
        let a = on();
        assert_eq!(for_message(&a, &notice("resub", &[("cumulative-months", "12")]), "en").unwrap().1, "Alice subscribed for 12 months!");
        assert_eq!(for_message(&a, &notice("resub", &[("cumulative-months", "12")]), "de").unwrap().1, "Alice ist seit 12 Monaten dabei!");
        assert_eq!(for_message(&a, &notice("submysterygift", &[("mass-gift-count", "5"), ("community-gift-id", "x")]), "en").unwrap().1, "Alice gifted 5 subs to the community!");
        // the five single gifts that follow are not announced again
        assert!(for_message(&a, &notice("subgift", &[("recipient-display-name", "Bob"), ("community-gift-id", "x")]), "en").is_none());
        assert_eq!(for_message(&a, &notice("subgift", &[("recipient-display-name", "Bob")]), "en").unwrap().1, "Alice gifted a sub to Bob!");
        assert_eq!(for_message(&a, &notice("raid", &[("displayName", "BigStreamer"), ("viewerCount", "250")]), "en").unwrap().1, "BigStreamer is raiding with 250 viewers!");
        assert_eq!(for_message(&a, &notice("anonsubgift", &[("recipient-display-name", "Bob")]), "en").unwrap().1, "Someone anonymous gifted a sub to Bob!");
        let mut cheer = notice("x", &[]);
        cheer.kind = Kind::Chat;
        cheer.notice_type = None;
        cheer.bits = 500;
        assert_eq!(for_message(&a, &cheer, "en").unwrap(), ("bits", "Alice cheered 500 bits!".into()));
        // over EventSub (Twitch login): follows only when switched on
        assert!(for_message(&a, &notice("follow", &[]), "en").is_none(), "follows are off by default");
        let mut f = on();
        f.follows = true;
        assert_eq!(for_message(&f, &notice("follow", &[]), "de").unwrap().1, "Alice folgt jetzt!");
        assert_eq!(for_message(&a, &notice("redeem", &[("reward", "Hydrate!")]), "en").unwrap().1, "Alice redeemed Hydrate!!");
    }

    #[test]
    fn respects_switches_minimums_and_own_texts() {
        assert!(for_message(&Announcements::default(), &notice("sub", &[]), "en").is_none(), "off by default");
        let mut a = on();
        a.raids = false;
        assert!(for_message(&a, &notice("raid", &[("viewerCount", "50")]), "en").is_none());
        let mut a = on();
        a.min_raid = 10;
        assert!(for_message(&a, &notice("raid", &[("viewerCount", "3")]), "en").is_none());
        let mut cheer = notice("x", &[]);
        cheer.notice_type = None;
        cheer.kind = Kind::Chat;
        cheer.bits = 50;
        assert!(for_message(&on(), &cheer, "en").is_none(), "below the minimum of 100 bits");
        let mut a = on();
        a.texts.insert("sub".into(), "Welcome to the club, {user}!".into());
        assert_eq!(for_message(&a, &notice("sub", &[]), "en").unwrap().1, "Welcome to the club, Alice!");
    }
}
