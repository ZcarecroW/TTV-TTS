//! Measures WAV files like the quality check does, one JSON line per file.
//!   cargo run --release -p ttv-engine --example analyze -- <dir>
//!   cargo run --release -p ttv-engine --example analyze -- <dir> --assess calib.json [--new <dir> new.json]
//! With --assess, calib.json lists {file, lang, gender, age, name, description,
//! text, wer} per file and the output is the quality report instead. --new adds
//! a second clip per file (the priming sentence) and its transcripts.
use std::collections::HashMap;
use std::time::Instant;

use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    file: String,
    lang: String,
    gender: String,
    age: u8,
    name: String,
    description: String,
    text: String,
    wer: Option<f32>,
    #[serde(default)]
    key: String,
}

fn load(path: &std::path::Path) -> Option<ttv_engine::quality::Analysis> {
    let bytes = std::fs::read(path).ok()?;
    let (samples, rate) = ttv_engine::decode_wav(&bytes)?;
    Some(ttv_engine::quality::analyze(&samples, rate))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from(args.get(1).expect("usage: analyze <dir> [--assess calib.json] [--new <dir> new.json]"));
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| args[i + 1..].to_vec());
    let t = Instant::now();
    if let Some(rest) = flag("--assess") {
        let rows: Vec<Row> = serde_json::from_slice(&std::fs::read(&rest[0]).expect("calib.json")).expect("calib");
        let new = flag("--new").map(|r| {
            let heard: HashMap<String, Option<String>> = serde_json::from_slice(&std::fs::read(&r[1]).expect("new.json")).expect("new");
            (std::path::PathBuf::from(&r[0]), heard)
        });
        // pitch per definition and language, for "languages differ"
        let mut analyses = HashMap::new();
        for r in &rows {
            if let Some(a) = load(&dir.join(format!("{}.wav", r.file))) {
                analyses.insert(r.file.clone(), a);
            }
        }
        type Report<'a> = (ttv_link::VoiceDef, Vec<(String, ttv_link::Quality, &'a Row, ttv_engine::quality::Analysis)>);
        let mut reports: std::collections::BTreeMap<(String, String), Report> = Default::default();
        for r in &rows {
            let Some(a) = analyses.get(&r.file) else { continue };
            let def = ttv_link::VoiceDef { name: r.name.clone(), gender: r.gender.clone(), age: r.age, description: r.description.clone(), fx: None };
            let lang = ttv_engine::langs::get(&r.lang).expect("lang");
            let new_clip = new.as_ref().and_then(|(d, heard)| {
                let n = load(&d.join(format!("{}.wav", r.file)))?;
                let wer = heard.get(&r.key).cloned().flatten().map(|h| ttv_engine::langs::error_rate(lang.verify, &h, lang.code));
                Some((n, wer))
            });
            let q = ttv_engine::quality::assess(&ttv_engine::quality::Input {
                def: &def,
                lang,
                reference: a,
                reference_text: &r.text,
                reference_wer: r.wer,
                new: new_clip.as_ref().map(|(n, _)| (n, lang.verify)),
                new_wer: new_clip.as_ref().and_then(|(_, w)| *w),
            });
            reports.entry((r.name.clone(), r.description.clone())).or_insert_with(|| (def.clone(), Vec::new())).1.push((r.lang.clone(), q, r, a.clone()));
        }
        for (def, group) in reports.values_mut() {
            let mut qs: Vec<(String, ttv_link::Quality)> = group.iter().map(|(l, q, _, _)| (l.clone(), q.clone())).collect();
            ttv_engine::quality::relate(Some(def), &mut qs);
            for ((_, q, r, a), (_, related)) in group.iter_mut().zip(qs) {
                *q = related;
                println!("{}", serde_json::json!({ "file": r.file, "name": r.name, "lang": r.lang, "gender": r.gender, "age": r.age, "q": q, "a": a }));
            }
        }
        eprintln!("assessed {} voices in {:.2} s", rows.len(), t.elapsed().as_secs_f32());
        return;
    }
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("dir").filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
    files.sort();
    let mut audio_secs = 0.0;
    for f in &files {
        let Some(a) = load(f) else {
            eprintln!("unreadable: {}", f.display());
            continue;
        };
        audio_secs += a.duration;
        println!("{}", serde_json::json!({ "file": f.file_stem().unwrap().to_string_lossy(), "a": a }));
    }
    eprintln!("{} files, {:.0} s of audio in {:.2} s", files.len(), audio_secs, t.elapsed().as_secs_f32());
}
