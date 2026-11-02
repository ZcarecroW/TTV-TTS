//! Finds voices that may sound odd: measures a voice's reference sample and
//! how it speaks a new sentence, compares that with its definition (gender,
//! age, description) and its other languages, and turns the differences into
//! a score with readable issues. Pure CPU work (~10–30 ms per clip).

use serde::{Deserialize, Serialize};
use ttv_link::{Quality, QualityIssue, VoiceDef};

/// Below this score a voice is listed for review.
pub const FLAG: f32 = 0.75;

/// What a clip sounds like, measurably.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    /// Length of the clip (s).
    pub duration: f32,
    /// Speech loudness (dBFS, RMS above a gate).
    pub level: Option<f32>,
    pub peak: f32,
    /// Share of samples at full scale.
    pub clipped: f32,
    /// Median pitch (Hz) of the voiced parts.
    pub f0: Option<f32>,
    /// Pitch movement: spread between the 25th and 75th percentile (semitones).
    pub f0_spread: Option<f32>,
    /// Time from the first to the last spoken frame (s).
    pub span: f32,
    /// `span` without pauses longer than 250 ms (s).
    pub talk: f32,
    /// Longest pause inside the speech (s).
    pub pause: f32,
    /// Speech level above the noise floor (dB); None = digital silence.
    pub snr: Option<f32>,
}

/// Measures a mono clip.
pub fn analyze(samples: &[f32], rate: u32) -> Analysis {
    let duration = samples.len() as f32 / rate.max(1) as f32;
    let mut a = Analysis { duration, level: crate::audio::loudness_db(samples), ..Default::default() };
    if samples.is_empty() || rate == 0 {
        return a;
    }
    let mut clipped = 0usize;
    for s in samples {
        let x = s.abs();
        a.peak = a.peak.max(x);
        if x >= 0.999 {
            clipped += 1;
        }
    }
    a.clipped = clipped as f32 / samples.len() as f32;

    // 10 ms frames: energy in dB.
    let hop = (rate as usize / 100).max(1);
    let frames: Vec<f32> = samples.chunks(hop).map(|c| 10.0 * ((c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32) + 1e-12).log10()).collect();
    let mut sorted = frames.clone();
    sorted.sort_by(f32::total_cmp);
    let pct = |p: f32| sorted[((sorted.len() - 1) as f32 * p) as usize];
    let loud = pct(0.95);
    // Speech = frames within 35 dB of the loud part (and above −65 dBFS).
    let gate = (loud - 35.0).max(-65.0);
    let voiced: Vec<bool> = frames.iter().map(|&d| d > gate).collect();
    if let (Some(first), Some(last)) = (voiced.iter().position(|&v| v), voiced.iter().rposition(|&v| v)) {
        a.span = (last - first + 1) as f32 / 100.0;
        let (mut run, mut longest, mut paused) = (0usize, 0usize, 0usize);
        for &v in &voiced[first..=last] {
            if v {
                if run >= 25 {
                    paused += run;
                }
                run = 0;
            } else {
                run += 1;
                longest = longest.max(run);
            }
        }
        a.pause = longest as f32 / 100.0;
        a.talk = a.span - paused as f32 / 100.0;
    }
    // Noise floor: the quietest frames; TTS output is often digitally silent.
    let floor = pct(0.1);
    a.snr = (floor > -90.0).then(|| pct(0.9) - floor);

    let (f0, spread) = pitch(samples, rate, gate);
    a.f0 = f0;
    a.f0_spread = spread;
    a
}

/// Median pitch with YIN (de Cheveigné & Kawahara 2002) on a ~12 kHz copy,
/// 30 ms windows every 10 ms, only on frames louder than `gate` dB.
fn pitch(samples: &[f32], rate: u32, gate: f32) -> (Option<f32>, Option<f32>) {
    let k = ((rate as f32 / 12_000.0).round() as usize).max(1);
    let r = rate as f32 / k as f32;
    let d: Vec<f32> = samples.chunks_exact(k).map(|c| c.iter().sum::<f32>() / k as f32).collect();
    let win = (r * 0.03) as usize;
    let hop = (r * 0.01) as usize;
    let tmin = (r / 450.0) as usize;
    let tmax = (r / 60.0) as usize;
    if d.len() < win + tmax + 1 || hop == 0 {
        return (None, None);
    }
    let mut diff = vec![0f32; tmax + 1];
    let mut f0s = Vec::new();
    let mut i = 0;
    while i + win + tmax < d.len() {
        let frame = &d[i..i + win];
        let e = frame.iter().map(|x| x * x).sum::<f32>() / win as f32;
        if 10.0 * (e + 1e-12).log10() > gate {
            for (t, slot) in diff.iter_mut().enumerate().skip(1) {
                let lag = &d[i + t..i + t + win];
                *slot = frame.iter().zip(lag).map(|(a, b)| (a - b) * (a - b)).sum();
            }
            // Cumulative mean normalised difference; first dip below 0.12.
            let mut run = 0f32;
            let mut best = None;
            let mut t = 1;
            while t <= tmax {
                run += diff[t];
                let c = diff[t] * t as f32 / run.max(1e-12);
                if t >= tmin && c < 0.12 {
                    // follow the dip down to its minimum
                    let mut cur = c;
                    while t < tmax {
                        let next_run = run + diff[t + 1];
                        let next = diff[t + 1] * (t + 1) as f32 / next_run.max(1e-12);
                        if next >= cur {
                            break;
                        }
                        t += 1;
                        run = next_run;
                        cur = next;
                    }
                    best = Some(t);
                    break;
                }
                t += 1;
            }
            if let Some(t) = best {
                f0s.push(r / t as f32);
            }
        }
        i += hop;
    }
    if f0s.len() < 30 {
        return (None, None);
    }
    f0s.sort_by(f32::total_cmp);
    let q = |p: f32| f0s[((f0s.len() - 1) as f32 * p) as usize];
    (Some(q(0.5)), Some(12.0 * (q(0.75) / q(0.25)).log2()))
}

/// Letters (or CJK characters) of a text, for speaking rates.
pub fn letters(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphanumeric()).count()
}

/// Everything known about one voice.
pub struct Input<'a> {
    pub def: &'a VoiceDef,
    pub lang: &'a crate::langs::DesignLang,
    /// The reference sample and what it says.
    pub reference: &'a Analysis,
    pub reference_text: &'a str,
    pub reference_wer: Option<f32>,
    /// How it spoke a new sentence (the priming sentence).
    pub new: Option<(&'a Analysis, &'a str)>,
    pub new_wer: Option<f32>,
}

fn has(description: &str, words: &[&str]) -> bool {
    let d = description.to_lowercase();
    words.iter().any(|w| d.contains(w))
}

/// Plausible speaking pitch (Hz) for a gender and age, widened where the
/// description asks for it.
fn pitch_range(def: &VoiceDef) -> Option<(f32, f32)> {
    let (lo, hi) = match (def.gender.as_str(), def.age) {
        ("m", 0..13) | ("f", 0..13) => (180.0, 440.0),
        ("m", 13..16) => (105.0, 330.0),
        ("m", 16..65) => (72.0, 185.0),
        ("m", _) => (80.0, 210.0),
        ("f", 13..16) => (155.0, 350.0),
        ("f", 16..65) => (150.0, 310.0),
        ("f", _) => (130.0, 290.0),
        _ => return None,
    };
    let d = &def.description;
    let hi = if has(d, &["high", "squeak", "falsetto", "shrill", "cracking", "boyish", "nasal", "reedy"]) { hi * 1.3 } else { hi };
    // shouting and excitement raise the pitch
    let hi = if has(d, &["shout", "yell", "command", "battle", "booming", "energetic", "excited", "hearty", "marine", "general"]) { hi * 1.2 } else { hi };
    let lo = if has(d, &["deep", "low", "husky", "smoky", "gravel", "baritone", "contralto"]) { lo * 0.9 } else { lo };
    Some((lo, hi))
}

/// Score (1 = nothing odd) and issues of a voice.
pub fn assess(x: &Input) -> Quality {
    let mut issues: Vec<QualityIssue> = Vec::new();
    let mut add = |code: &str, value: f32, weight: f32, extra: Option<String>| {
        let weight = weight.clamp(0.0, 0.9);
        if weight >= 0.05 {
            issues.push(QualityIssue { code: code.into(), value: (value * 100.0).round() / 100.0, weight: (weight * 100.0).round() / 100.0, extra });
        }
    };
    let r = x.reference;
    let d = &x.def.description;

    // loudness
    let quiet_ok = has(d, &["whisper", "hushed", "low volume", "quiet", "soft-spoken", "murmur"]);
    match r.level {
        None => add("silent", 0.0, 0.9, None),
        Some(db) if db < if quiet_ok { -34.0 } else { -30.0 } => add("quiet", db, 0.2 + (-30.0 - db) * 0.08, None),
        Some(db) if db > -11.0 => add("loud", db, 0.2 + (db + 11.0) * 0.05, None),
        _ => {}
    }
    if r.clipped > 0.002 {
        add("clipping", r.clipped * 100.0, 0.3, None);
    }
    // Calibrated on 420 real voices: p1 = 26 dB. Only clear hiss counts.
    if let Some(snr) = r.snr
        && snr < 24.0
    {
        add("noise", snr, (24.0 - snr) * 0.05, None);
    }

    // pitch against gender and age
    if let Some(f0) = r.f0 {
        let st = outside(x.def, f0);
        if st > 1.0 {
            let hi = pitch_range(x.def).is_some_and(|(_, hi)| f0 > hi);
            add(if hi { "pitch_high" } else { "pitch_low" }, f0.round(), (st - 1.0) * 0.12, None);
        }
    }

    // pauses and pace
    // A pause between two sentences is natural (p99 of 420 voices: 1.4 s).
    let pause_ok = has(d, &["pause", "hesitant", "stammer", "halting", "deliberate"]);
    let sentences = x.reference_text.trim_end_matches(['.', '!', '?', '。', '！', '？']).contains(['.', '!', '?', '。', '！', '？']);
    let max_pause = if pause_ok { 1.8 } else if sentences { 1.5 } else { 1.0 };
    if r.pause > max_pause {
        add("pause", r.pause, 0.15 + (r.pause - max_pause) * 0.3, None);
    }
    let typical = x.lang.letters_per_sec;
    if r.talk > 0.5 && typical > 0.0 {
        let pace = letters(x.reference_text) as f32 / r.talk / typical;
        let (slow_ok, fast_ok) = (has(d, &["slow", "deliberate", "drawl", "unhurried", "measured", "languid", "leisurely"]), has(d, &["fast", "rapid", "quick", "breathless", "hurried", "rattling"]));
        let (min, max) = (if slow_ok { 0.55 } else { 0.68 }, if fast_ok { 1.6 } else { 1.38 });
        if pace < min {
            add("slow", pace, 0.15 + (min - pace) * 1.5, None);
        } else if pace > max {
            add("fast", pace, 0.15 + (pace - max) * 1.2, None);
        }
    }

    // understood?
    if let Some(w) = x.reference_wer
        && w > 0.0
    {
        add("misheard", w, w * 2.0, None);
    }

    // how it speaks a new sentence
    if let Some((n, text)) = x.new {
        if let Some(w) = x.new_wer
            && w > 0.0
        {
            add("new_misheard", w, 0.1 + w * 2.5, None);
        }
        if r.talk > 0.5 && n.talk > 0.2 {
            let expected = letters(text) as f32 * r.talk / letters(x.reference_text).max(1) as f32;
            let ratio = n.talk / expected.max(0.1);
            if !(0.55..=1.75).contains(&ratio) {
                add("new_length", ratio, 0.35, None);
            }
        }
        if let (Some(a), Some(b)) = (r.f0, n.f0) {
            // The priming sentence is an exclamation: +4–6 semitones is natural
            // (measured on 120 real voices).
            let st = 12.0 * (b / a).log2();
            if st.abs() > 6.0 {
                add("new_pitch", st, (st.abs() - 6.0) * 0.08 + 0.15, None);
            }
        }
        if let (Some(a), Some(b)) = (r.level, n.level)
            && (b - a).abs() > 7.0
        {
            add("new_level", b - a, 0.15, None);
        }
    }

    let mut q = Quality { score: 1.0, issues, f0: r.f0.map(|f| f.round()), checked: chrono::Utc::now().timestamp(), new_text: x.new.is_some(), kept: false };
    rescore(&mut q);
    q
}

fn rescore(q: &mut Quality) {
    let score = q.issues.iter().fold(1.0, |s, i| s * (1.0 - i.weight));
    q.score = (score * 1000.0).round() / 1000.0;
}

/// Semitones by which a pitch lies outside the plausible range (0 = inside).
fn outside(def: &VoiceDef, f0: f32) -> f32 {
    match pitch_range(def) {
        Some((_, hi)) if f0 > hi => 12.0 * (f0 / hi).log2(),
        Some((lo, _)) if f0 < lo => 12.0 * (lo / f0).log2(),
        _ => 0.0,
    }
}

/// The same character should sound alike in every language: compares the
/// pitch of all languages of one definition and (re)sets their
/// "languages_differ" issue. With three or more languages, each one is
/// compared with the group's median – only the odd one out is noted, not the
/// many that agree. With two, each is compared with the other; if only the
/// other version is out of range for the description (`def`), that one is the
/// odd one (it has its own pitch issue). Returns whether anything changed.
pub fn relate(def: Option<&VoiceDef>, group: &mut [(String, Quality)]) -> bool {
    let pitches: Vec<(String, f32)> = group.iter().filter_map(|(l, q)| Some((l.clone(), q.f0?))).collect();
    let median = (pitches.len() >= 3).then(|| {
        let mut v: Vec<f32> = pitches.iter().map(|p| p.1).collect();
        v.sort_by(f32::total_cmp);
        let m = v.len() / 2;
        if v.len() % 2 == 1 { v[m] } else { (v[m - 1] * v[m]).sqrt() }
    });
    let mut changed = false;
    for (lang, q) in group.iter_mut() {
        let before = q.issues.clone();
        q.issues.retain(|i| i.code != "languages_differ");
        let compared = q.f0.and_then(|f0| match median {
            // the language closest to the median stands for the group
            Some(m) => pitches
                .iter()
                .filter(|(l, _)| l != lang)
                .min_by(|a, b| (a.1 / m).log2().abs().total_cmp(&(b.1 / m).log2().abs()))
                .map(|(l, p)| (f0, m, l.clone(), *p, false)),
            None => pitches
                .iter()
                .filter(|(l, _)| l != lang)
                .max_by(|a, b| (a.1 / f0).log2().abs().total_cmp(&(b.1 / f0).log2().abs()))
                .map(|(l, p)| (f0, *p, l.clone(), *p, def.is_some_and(|d| outside(d, f0) <= 1.0 && outside(d, *p) > 1.0))),
        });
        if let Some((f0, reference, other_lang, other, only_other_odd)) = compared {
            let st = 12.0 * (f0 / reference).log2().abs();
            if st > 6.0 && !only_other_odd {
                let weight = ((st - 6.0) * 0.04 + 0.1).min(0.35);
                q.issues.push(QualityIssue { code: "languages_differ".into(), value: other.round(), weight: (weight * 100.0).round() / 100.0, extra: Some(other_lang.clone()) });
            }
        }
        if q.issues != before {
            rescore(q);
            changed = true;
        }
    }
    changed
}

/// How good a design take is, from its sample alone (1 = nothing odd): used
/// to pick the best of 2–3 takes, so odd ones lose before they're saved.
pub fn take_score(def: &VoiceDef, lang: &crate::langs::DesignLang, a: &Analysis, wer: Option<f32>) -> f32 {
    let q = assess(&Input { def, lang, reference: a, reference_text: lang.text, reference_wer: wer, new: None, new_wer: None });
    q.score - (a.duration - 7.0).abs() * 0.01
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, secs: f32, amp: f32, rate: u32) -> Vec<f32> {
        // A sawtooth-ish voice: fundamental + harmonics, like speech.
        (0..(secs * rate as f32) as usize)
            .map(|i| {
                let t = i as f32 / rate as f32;
                (1..6).map(|h| (t * hz * h as f32 * std::f32::consts::TAU).sin() / h as f32).sum::<f32>() * amp * 0.5
            })
            .collect()
    }

    #[test]
    fn measures_pitch_level_and_pauses() {
        let rate = 24_000;
        let mut s = tone(120.0, 2.0, 0.2, rate);
        s.extend(std::iter::repeat_n(0.0, (1.5 * rate as f32) as usize)); // 1.5 s pause
        s.extend(tone(120.0, 2.0, 0.2, rate));
        let a = analyze(&s, rate);
        let f0 = a.f0.unwrap();
        assert!((f0 - 120.0).abs() < 3.0, "f0 {f0}");
        assert!(a.f0_spread.unwrap() < 0.5);
        assert!((a.pause - 1.5).abs() < 0.05, "pause {}", a.pause);
        assert!((a.span - 5.5).abs() < 0.05);
        assert!((a.talk - 4.0).abs() < 0.05, "talk {}", a.talk);
        assert_eq!(a.snr, None, "digital silence");
        let high = analyze(&tone(240.0, 2.0, 0.2, rate), rate);
        assert!((high.f0.unwrap() - 240.0).abs() < 5.0);
    }

    #[test]
    fn hears_noise_and_clipping() {
        let rate = 24_000;
        let mut s = tone(200.0, 2.0, 2.5, rate);
        for x in &mut s {
            *x = x.clamp(-1.0, 1.0);
        }
        // a noisy pause: random hiss at −40 dBFS
        let mut seed = 1u32;
        s.extend((0..rate).map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed as f32 / u32::MAX as f32 - 0.5) * 0.035
        }));
        let a = analyze(&s, rate);
        assert!(a.clipped > 0.01);
        assert!(a.snr.unwrap() < 40.0, "{:?}", a.snr);
    }

    fn def(gender: &str, age: u8, description: &str) -> VoiceDef {
        VoiceDef { name: "X".into(), gender: gender.into(), age, description: description.into(), fx: None }
    }

    #[test]
    fn flags_a_baritone_that_sounds_like_a_child() {
        let en = crate::langs::get("en").unwrap();
        let text = en.text;
        let good = Analysis { level: Some(-20.0), f0: Some(115.0), span: 7.0, talk: 6.8, pause: 0.3, duration: 7.5, snr: None, ..Default::default() };
        let d = def("m", 35, "Punchy, confident action-hero baritone.");
        let ok = assess(&Input { def: &d, lang: en, reference: &good, reference_text: text, reference_wer: Some(0.0), new: None, new_wer: None });
        assert!(ok.score > 0.95 && ok.issues.is_empty(), "{ok:?}");
        let child = Analysis { f0: Some(350.0), ..good.clone() };
        let bad = assess(&Input { def: &d, lang: en, reference: &child, reference_text: text, reference_wer: Some(0.0), new: None, new_wer: None });
        assert!(bad.score < FLAG && bad.issues.iter().any(|i| i.code == "pitch_high"), "{bad:?}");
        // The German version is fine, the English one sounds like a child:
        // only the odd one gets "languages differ".
        let mut group = vec![("de".to_string(), ok.clone()), ("en".to_string(), bad.clone())];
        assert!(relate(Some(&d), &mut group));
        assert!(group[0].1.issues.is_empty(), "{:?}", group[0].1);
        assert!(group[1].1.issues.iter().any(|i| i.code == "languages_differ" && i.extra.as_deref() == Some("de")));
        assert!(group[1].1.score < bad.score);
        assert!(!relate(Some(&d), &mut group), "stable");
        // Both plausible but far apart: both get a mild note.
        let low = Analysis { f0: Some(80.0), ..good.clone() };
        let high = Analysis { f0: Some(170.0), ..good.clone() };
        let mk = |a: &Analysis| assess(&Input { def: &d, lang: en, reference: a, reference_text: text, reference_wer: Some(0.0), new: None, new_wer: None });
        let mut pair = vec![("de".to_string(), mk(&low)), ("en".to_string(), mk(&high))];
        relate(Some(&d), &mut pair);
        assert!(pair.iter().all(|(_, q)| q.issues.iter().any(|i| i.code == "languages_differ")));
        // Many languages that agree and one that doesn't: only that one.
        let mid = Analysis { f0: Some(118.0), ..good.clone() };
        let mut many: Vec<(String, Quality)> = ["de", "en", "fr", "es"].iter().map(|l| (l.to_string(), mk(&good))).collect();
        many.push(("it".to_string(), mk(&mid)));
        many.push(("ko".to_string(), mk(&high)));
        relate(None, &mut many);
        let flagged: Vec<&str> = many.iter().filter(|(_, q)| q.issues.iter().any(|i| i.code == "languages_differ")).map(|(l, _)| l.as_str()).collect();
        assert_eq!(flagged, vec!["ko"]);
    }

    #[test]
    fn description_words_allow_intended_traits() {
        let en = crate::langs::get("en").unwrap();
        let whisper = Analysis { level: Some(-32.0), f0: Some(110.0), span: 7.0, talk: 6.8, duration: 7.5, ..Default::default() };
        let plain = def("m", 40, "Calm, warm voice.");
        let hushed = def("m", 40, "Hushed, conspiratorial whisper.");
        let a = assess(&Input { def: &plain, lang: en, reference: &whisper, reference_text: en.text, reference_wer: None, new: None, new_wer: None });
        let b = assess(&Input { def: &hushed, lang: en, reference: &whisper, reference_text: en.text, reference_wer: None, new: None, new_wer: None });
        assert!(a.issues.iter().any(|i| i.code == "quiet"));
        assert!(b.issues.is_empty(), "{b:?}");
    }

    #[test]
    fn new_text_problems_count() {
        let en = crate::langs::get("en").unwrap();
        let r = Analysis { level: Some(-20.0), f0: Some(200.0), span: 7.0, talk: 6.8, duration: 7.5, ..Default::default() };
        // babbles: the short sentence takes as long as the long one
        let n = Analysis { level: Some(-20.0), f0: Some(200.0), span: 7.0, talk: 6.8, duration: 7.5, ..Default::default() };
        let d = def("f", 30, "Bright, friendly voice.");
        let q = assess(&Input { def: &d, lang: en, reference: &r, reference_text: en.text, reference_wer: Some(0.0), new: Some((&n, en.verify)), new_wer: Some(0.4) });
        assert!(q.score < FLAG && q.new_text);
        assert!(q.issues.iter().any(|i| i.code == "new_misheard") && q.issues.iter().any(|i| i.code == "new_length"), "{q:?}");
    }
}
