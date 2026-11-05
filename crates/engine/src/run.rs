//! One batch of a job: computing new voices, improving odd ones, or checking
//! the quality of computed ones. Runs in phases so VCP swaps its models as
//! rarely as possible: design (voice designer) → check (speech recognition)
//! → save + prime (speaking model) → review (speech recognition).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use base64::Engine as _;
use ttv_link::{JobKind, JobRequest, JobState, LibraryEntry, Quality, VoiceDef};

use crate::quality::{self, Analysis, FLAG};
use crate::models::Listening;
use crate::{ATTEMPTS, BATCH, DESIGNING, Inner, LISTENING, MAX_WER, MAX_WER_CJK, Spoken, Trouble, audio, design, instruct, langs, mix_levels, save_voice, transcribe};

pub(crate) struct Take {
    pub wav: Vec<u8>,
    pub duration: f32,
    pub wer: Option<f32>,
    pub analysis: Analysis,
}

pub(crate) struct Item {
    index: u32,
    def: VoiceDef,
    lang: &'static langs::DesignLang,
    key: String,
    takes: Vec<Take>,
    /// Improve and review: the voice as it is.
    old: Option<LibraryEntry>,
}

/// A voice that was saved (or, when improving, a candidate) and still waits
/// for its review.
struct Fresh {
    item: usize,
    entry: LibraryEntry,
    take: Take,
    spoken: Spoken,
}

/// Takes per voice at most: a take that shows a defect right after designing
/// gets a successor while the design model is loaded. 68 % of all recomputed
/// voices were odd in their sample already, and were only redone a job later.
const MAX_TAKES: usize = 5;
/// A take this clean needs no comparison with the other takes.
const CLEAN_WER: f32 = 0.02;

fn max_wer(lang: &langs::DesignLang) -> f32 {
    if matches!(lang.code, "ja" | "zh" | "ko") { MAX_WER_CJK } else { MAX_WER }
}

/// Reference samples must be 3–10.5 s long (VCP clones best from that).
fn plausible(t: &Take) -> bool {
    (3.0..=10.5).contains(&t.duration)
}

fn label(it: &Item) -> String {
    format!("{} · {}", it.def.name, it.lang.code)
}

/// The next voices of a job.
fn select(inner: &Inner, job: &crate::StoredJob) -> Vec<Item> {
    let req = &job.request;
    let only: HashSet<&str> = req.only.iter().map(String::as_str).collect();
    let size = inner.batch_size(req.options.archive);
    let mut items = Vec::new();
    for d in &req.defs {
        for code in &req.languages {
            let Some(lang) = langs::get(code) else { continue };
            let key = d.def.key(code);
            if (!only.is_empty() && !only.contains(key.as_str())) || job.failed.contains(&key) || job.processed.contains(&key) {
                continue;
            }
            let old = inner.library.get(&key);
            match req.kind {
                JobKind::Compute if old.is_some() || inner.archive.contains(&key) => continue,
                // deleted meanwhile: nothing to improve or check
                JobKind::Improve | JobKind::Review if old.is_none() => continue,
                _ => {}
            }
            items.push(Item { index: d.index, def: d.def.clone(), lang, key, takes: Vec::new(), old });
            if items.len() >= size {
                return items;
            }
        }
    }
    items
}

/// What's left of a batch that stopped halfway.
fn resume(inner: &Inner, job: &crate::StoredJob, items: Vec<Item>) -> Vec<Item> {
    let kind = job.request.kind;
    items
        .into_iter()
        .filter(|it| !job.failed.contains(&it.key) && !job.processed.contains(&it.key))
        .filter(|it| kind != JobKind::Compute || !(inner.library.contains(&it.key) || inner.archive.contains(&it.key)))
        .filter_map(|mut it| {
            if kind == JobKind::Improve {
                it.old = Some(inner.library.get(&it.key)?);
            }
            Some(it)
        })
        .collect()
}

/// Voices of this job that are in the library but not in the job: saved just
/// before the proxy stopped (restart, update, crash) – the library writes a
/// voice at once, the job its count only after the batch. The rerun skipped
/// them as "already there": never counted, never quality-checked. Count them
/// and keep them for the job's check. Only voices from after the job's start
/// (older ones belong to other jobs). Returns whether it took any.
fn adopt_saved(inner: &Inner, id: &str, job: &crate::StoredJob) -> bool {
    let req = &job.request;
    let only: HashSet<&str> = req.only.iter().map(String::as_str).collect();
    let known: HashSet<&str> = job.failed.iter().chain(&job.processed).chain(&job.created).map(String::as_str).collect();
    let mut keys = Vec::new();
    for d in &req.defs {
        for code in &req.languages {
            let key = d.def.key(code);
            if (!only.is_empty() && !only.contains(key.as_str())) || known.contains(key.as_str()) {
                continue;
            }
            if inner.library.get(&key).is_some_and(|e| e.created >= job.view.created) {
                keys.push(key);
            }
        }
    }
    if keys.is_empty() {
        return false;
    }
    log::info!("job {id}: {} voices saved before the proxy stopped – counted, and they get checked", keys.len());
    if req.options.archive {
        inner.pending.lock().extend(keys.iter().cloned());
        inner.save_pending();
    }
    let n = keys.len() as u32;
    inner.update(id, |j| {
        j.view.done += n;
        j.created.extend(keys);
    });
    inner.save_jobs();
    true
}

/// Runs one batch of a job. Ok(()) also when the job was paused/cancelled.
pub(crate) async fn batch(inner: &Inner, vcp: &ttv_vcp::Client, id: &str) -> Result<(), String> {
    let Some(job) = inner.jobs.lock().iter().find(|j| j.view.id == id).cloned() else { return Ok(()) };
    if job.request.kind == JobKind::Import {
        return import(inner, vcp, id, &job).await;
    }
    if job.request.kind == JobKind::Restore {
        return restore(inner, vcp, id, &job).await;
    }
    let job = if job.request.kind == JobKind::Compute && adopt_saved(inner, id, &job) {
        let Some(j) = inner.jobs.lock().iter().find(|j| j.view.id == id).cloned() else { return Ok(()) };
        j
    } else {
        job
    };
    let parked = inner.parked.lock().remove(id);
    let mut items = parked.map(|p| resume(inner, &job, p)).filter(|p| !p.is_empty()).unwrap_or_else(|| select(inner, &job));
    if items.is_empty() {
        finish(inner, id, &job);
        return Ok(());
    }
    let n = items.len() as u32;
    inner.update(id, |j| {
        j.view.state = JobState::Running;
        j.view.error = None;
        j.view.batch = n;
    });
    let t0 = Instant::now();
    let result = match job.request.kind {
        JobKind::Review => review(inner, vcp, id, items).await,
        _ => {
            let r = compute(inner, vcp, id, &job.request, &mut items).await;
            // VCP restarted or the job was paused: what's designed stays
            if r.is_err() || inner.state_of(id) == Some(JobState::Paused) {
                inner.parked.lock().insert(id.to_string(), items);
            }
            r
        }
    };
    let finished = result?;
    if finished > 0 {
        inner.measured(id, job.request.kind, t0.elapsed().as_secs_f32() / finished as f32);
    }
    inner.save_jobs();
    Ok(())
}

fn finish(inner: &Inner, id: &str, job: &crate::StoredJob) {
    inner.update(id, |j| {
        j.view.state = JobState::Done;
        j.view.phase.clear();
        j.view.current = None;
        j.view.batch = 0;
        j.view.step = 0;
        j.view.steps = 0;
        j.view.eta_secs = Some(0);
        j.view.finished = Some(chrono::Utc::now().timestamp());
    });
    log::info!("job {id} finished");
    let req = &job.request;
    // the plan decides what comes next; a finished job needs no voice list
    if req.plan {
        crate::plan::job_finished(inner, req.kind, &job.failed, &req.only);
        inner.update(id, |j| {
            j.request.defs = Vec::new();
            j.request.only = Vec::new();
            j.created = Vec::new();
            j.processed = Vec::new();
            j.attempts = Default::default();
        });
        inner.save_jobs();
        return;
    }
    if !req.options.quality || job.followed || !matches!(req.kind, JobKind::Compute | JobKind::Review) {
        inner.save_jobs();
        return;
    }
    inner.update(id, |j| j.followed = true);
    let mine: &[String] = if req.kind == JobKind::Compute { &job.created } else { &job.processed };
    // Voices saved but never checked (the proxy was restarted or updated in
    // the middle of a batch): check them now; that check improves the odd
    // ones afterwards if asked to.
    let unchecked: Vec<String> = mine.iter().filter(|k| inner.library.get(k).is_some_and(|e| e.quality.is_none())).cloned().collect();
    if !unchecked.is_empty() {
        let follow = JobRequest { kind: JobKind::Review, only: unchecked, ..req.clone() };
        match inner.submit(follow) {
            Ok(v) => log::info!("job {id}: {} voices weren't checked, checking them in {}", v.total, v.id),
            Err(e) => log::warn!("job {id}: can't check the unchecked voices: {e}"),
        }
    }
    // Odd voices: design them again automatically.
    if req.options.improve {
        let odd: Vec<String> = mine.iter().filter(|k| inner.library.get(k).and_then(|e| e.quality).is_some_and(|q| q.score < FLAG && !q.kept)).cloned().collect();
        if !odd.is_empty() {
            let follow = JobRequest { kind: JobKind::Improve, only: odd, rounds: 2, ..req.clone() };
            match inner.submit(follow) {
                Ok(v) => log::info!("job {id}: {} odd voices, improving them in {}", v.total, v.id),
                Err(e) => log::warn!("job {id}: can't improve the odd voices: {e}"),
            }
        }
    }
    inner.save_jobs();
}

/// The speech check of a batch. False if the job was paused or cancelled.
async fn listen_to_takes(inner: &Inner, vcp: &ttv_vcp::Client, id: &str, req: &JobRequest, items: &mut [Item], listening: &Listening, todo: usize) -> Result<bool, String> {
    let (mut heard_any, mut last_failure) = (false, None);
    for it in items.iter_mut() {
        if !it.lang.asr {
            continue;
        }
        let (name, lang) = (label(it), it.lang);
        // The most promising take first: once one is clean, the others
        // need no listening (when the streamer picks, all do).
        let mut order: Vec<(usize, f32)> = it.takes.iter().enumerate().filter(|(_, t)| plausible(t) && t.wer.is_none()).map(|(i, t)| (i, quality::take_score(&it.def, lang, &t.analysis, None))).collect();
        order.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut clean = false;
        for (i, _) in order {
            if clean {
                inner.update(id, |j| j.view.step += 1);
                continue;
            }
            if !inner.gap_for(id, crate::SWITCH_GAP).await {
                return Ok(false);
            }
            let l = name.clone();
            inner.update(id, |j| j.view.current = Some(l));
            listening.keep_ready().await;
            let take = &mut it.takes[i];
            let t = Instant::now();
            match transcribe_retry(vcp, &take.wav, lang).await {
                Ok(heard) => {
                    inner.speed_sample(LISTENING, t.elapsed().as_secs_f32());
                    heard_any = true;
                    let wer = langs::error_rate(lang.text, &heard, lang.code);
                    take.wer = Some(wer);
                    clean = !req.options.pick && wer <= CLEAN_WER;
                }
                Err(Trouble::Down(e)) => return Err(format!("speech check: {e}")),
                Err(Trouble::Failed(e)) => {
                    log::warn!("speech check {name}: {e}");
                    take.wer = Some(1.0);
                    last_failure = Some(e);
                }
            }
            inner.update(id, |j| j.view.step += 1);
        }
    }
    // Speech recognition failed for every take: it's broken, not the voices.
    if todo > 0 && !heard_any {
        return Err(format!("VCP's speech recognition doesn't work: {}", last_failure.unwrap_or_default()));
    }
    Ok(true)
}

/// Computes new voices (Compute) or candidates for odd ones (Improve).
/// Returns how many voices were finished.
async fn compute(inner: &Inner, vcp: &ttv_vcp::Client, id: &str, req: &JobRequest, items: &mut [Item]) -> Result<u32, String> {
    let improving = req.kind == JobKind::Improve;
    let takes = if improving { req.options.takes.clamp(2, 3) } else { req.options.takes.clamp(1, 3) };

    // 1. design all takes (design checkpoint); a batch that goes on keeps
    // the takes it has
    inner.phase(id, "design", items.iter().map(|it| (takes as usize).saturating_sub(it.takes.len())).sum());
    let mut last_error = None;
    let spare = req.options.quality && !req.options.pick;
    for it in items.iter_mut() {
        for n in it.takes.len()..MAX_TAKES {
            if n >= takes as usize {
                let best = it.takes.iter().filter(|t| plausible(t)).map(|t| quality::take_score(&it.def, it.lang, &t.analysis, None)).fold(f32::MIN, f32::max);
                if !spare || best >= FLAG {
                    break;
                }
                inner.update(id, |j| j.view.steps += 1);
            }
            if !inner.gap_for(id, crate::SWITCH_GAP).await {
                return Ok(0);
            }
            let l = label(it);
            inner.update(id, |j| j.view.current = Some(l));
            let t = Instant::now();
            match design(vcp, &instruct(&it.def), it.lang).await {
                Ok(wav) => {
                    inner.speed_sample(DESIGNING, t.elapsed().as_secs_f32());
                    let duration = ttv_vcp::wav::duration_secs(&wav).unwrap_or(0.0);
                    let analysis = audio::decode(&wav).map(|(s, r)| quality::analyze(&s, r)).unwrap_or_default();
                    it.takes.push(Take { wav, duration, wer: None, analysis });
                }
                Err(e) => {
                    log::warn!("design {}: {e}", label(it));
                    // VCP hangs: don't wait for every take of the batch
                    if e.contains("timed out") {
                        return Err(format!("design: {e}"));
                    }
                    last_error = Some(e);
                }
            }
            inner.update(id, |j| j.view.step += 1);
        }
    }
    // Nothing designed at all: VCP is down or restarting. Try the batch again
    // later (the worker waits) instead of counting a failed attempt per voice.
    if items.iter().all(|it| it.takes.is_empty()) {
        return Err(format!("Voice Creator Pro designed nothing: {}", last_error.unwrap_or_else(|| "no answer".into())));
    }

    // 2. speech check (speech recognition model). A take only counts as
    // checked when VCP heard it; one VCP couldn't transcribe is not accepted.
    if req.options.speech_check {
        let todo: usize = items.iter().filter(|it| it.lang.asr).map(|it| it.takes.iter().filter(|t| plausible(t) && t.wer.is_none()).count()).sum();
        inner.phase(id, "check", todo);
        if todo > 0 {
            let first = items.iter().filter(|it| it.lang.asr).find_map(|it| it.takes.iter().find(|t| plausible(t)).map(|t| (t.wav.clone(), it.lang)));
            let listening = Listening::begin(inner, vcp, first.as_ref().map(|f| f.0.as_slice()), first.as_ref().map_or(items[0].lang, |f| f.1)).await;
            let heard = listen_to_takes(inner, vcp, id, req, items, &listening, todo).await;
            listening.end(inner).await;
            if !heard? {
                return Ok(0);
            }
        }
    }

    // "let me pick": every usable take waits for the streamer's choice
    if req.options.pick {
        let mut stored = 0;
        for it in items.iter() {
            let max_wer = max_wer(it.lang);
            let checked = req.options.speech_check && it.lang.asr;
            let usable: Vec<(usize, f32)> = it
                .takes
                .iter()
                .enumerate()
                .filter(|(_, t)| plausible(t) && if checked { t.wer.is_some_and(|w| w <= max_wer) } else { true })
                .map(|(i, t)| (i, quality::take_score(&it.def, it.lang, &t.analysis, t.wer)))
                .collect();
            let best = usable.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|x| x.0);
            if usable.is_empty() {
                inner.attempt_failed(id, &it.key, Some(format!("{}: no take was usable", label(it))));
                continue;
            }
            let infos = usable
                .iter()
                .enumerate()
                .map(|(n, (i, s))| ttv_link::TakeInfo { n: n as u32, duration: it.takes[*i].duration, wer: it.takes[*i].wer, score: *s, recommended: Some(*i) == best })
                .collect();
            let wavs: Vec<&[u8]> = usable.iter().map(|(i, _)| it.takes[*i].wav.as_slice()).collect();
            let choice = ttv_link::TakeChoice {
                key: it.key.clone(),
                name: it.def.name.clone(),
                lang: it.lang.code.to_string(),
                preset_id: req.preset_id.clone(),
                index: it.index,
                created: chrono::Utc::now().timestamp(),
                takes: infos,
            };
            match inner.store_takes(choice, &it.def, &wavs, req.options.backup) {
                Ok(()) => {
                    stored += 1;
                    let key = it.key.clone();
                    inner.update(id, |j| {
                        j.view.done += 1;
                        j.view.step += 1;
                        j.processed.push(key);
                    });
                }
                Err(e) => inner.attempt_failed(id, &it.key, Some(format!("{}: {e}", label(it)))),
            }
        }
        return Ok(stored);
    }

    // 3. pick the best take, save it, prime it (speaking checkpoint)
    inner.phase(id, "save", items.len());
    let mut fresh: Vec<Fresh> = Vec::new();
    for (i, it) in items.iter_mut().enumerate() {
        let max_wer = max_wer(it.lang);
        // The last attempt of a new voice whose takes all failed only the
        // speech check (whispers, accents the recognizer doesn't like): keep
        // the best one anyway; the quality check flags it if it sounds odd.
        let last_chance = !improving && inner.attempts(id, &it.key) + 1 >= ATTEMPTS;
        let checked = req.options.speech_check && it.lang.asr;
        let pick = |strict: bool| {
            it.takes
                .iter()
                .enumerate()
                .filter(|(_, t)| plausible(t) && if checked && strict { t.wer.is_some_and(|w| w <= max_wer) } else { true })
                .map(|(i, t)| (i, quality::take_score(&it.def, it.lang, &t.analysis, t.wer)))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        };
        // Best take: understood, and the least odd (pitch for its gender and
        // age, loudness, pauses, pace – see quality::take_score).
        let best = pick(true).or_else(|| {
            let b = pick(false).filter(|_| last_chance && checked);
            if b.is_some() {
                log::info!("{}: no take passed the speech check in {ATTEMPTS} attempts - keeping the best one", label(it));
            }
            b
        });
        let Some(bi) = best else {
            if improving {
                inner.round_done(id, req, &it.key, false, None);
            } else {
                inner.attempt_failed(id, &it.key, None);
            }
            continue;
        };
        let take = it.takes.swap_remove(bi);
        if !inner.gap(id).await {
            delete_candidates(vcp, &fresh, improving).await;
            return Ok(0);
        }
        let l = label(it);
        inner.update(id, |j| {
            j.view.current = Some(l);
            j.view.phase = "prime".into();
        });
        // an improve job can hold voices of several presets
        let preset_id = it.old.as_ref().map_or(req.preset_id.as_str(), |o| o.preset_id.as_str()).to_string();
        let voice_name = format!("TTV {} {} {:03} {}", preset_id, it.lang.code, it.index + 1, it.def.name);
        let save = serde_json::json!({
            "name": voice_name,
            "audio_base64": base64::engine::general_purpose::STANDARD.encode(&take.wav),
            "transcript": it.def.description,
            "ref_text": it.lang.text,
            "voice_type": "design",
            "transcript_manual": true,
        });
        let (voice, spoken) = match save_voice(vcp, &save, it.lang).await {
            Ok(v) => v,
            Err(Trouble::Down(e)) => {
                delete_candidates(vcp, &fresh, improving).await;
                return Err(e);
            }
            Err(Trouble::Failed(e)) => {
                log::warn!("{voice_name}: {e}");
                let why = Some(format!("{}: {e}", label(it)));
                if improving {
                    inner.round_done(id, req, &it.key, false, why);
                } else {
                    inner.attempt_failed(id, &it.key, why);
                }
                continue;
            }
        };
        let level = take.analysis.level;
        let entry = LibraryEntry {
            key: it.key.clone(),
            def_hash: it.def.hash(),
            lang: it.lang.code.to_string(),
            preset_id,
            index: it.index,
            name: it.def.name.clone(),
            voice_id: voice.id.clone(),
            voice_name,
            created: chrono::Utc::now().timestamp(),
            duration: take.duration,
            wer: take.wer,
            gain: audio::gain_from_db(mix_levels(level, spoken.analysis.level).unwrap_or(-20.0)),
            backup: false,
            level,
            quality: None,
            ref_text: None,
        };
        if improving {
            // a candidate: it replaces the old voice only if it's better
            fresh.push(Fresh { item: i, entry, take, spoken });
        } else {
            let mut entry = entry;
            entry.backup = req.options.backup && inner.backup(&it.key, &take.wav);
            inner.change(vec![entry.clone()], &[]);
            inner.update(id, |j| {
                j.view.done += 1;
                j.view.step += 1;
                j.view.phase = "save".into();
                j.created.push(entry.key.clone());
            });
            if req.options.archive {
                inner.pending.lock().insert(entry.key.clone());
                inner.save_pending();
            }
            fresh.push(Fresh { item: i, entry, take, spoken });
        }
    }
    let saved = fresh.len() as u32;

    // 4. review: how does each voice speak a new sentence? (speech recognition)
    if req.options.quality || improving {
        inner.phase(id, "review", fresh.len());
        let first = fresh.iter().find(|f| items[f.item].lang.asr).map(|f| (f.spoken.wav.clone(), items[f.item].lang));
        let listening = Listening::begin(inner, vcp, first.as_ref().map(|f| f.0.as_slice()), first.as_ref().map_or(&langs::LANGS[0], |f| f.1)).await;
        let items = &*items;
        let outcome: Result<bool, String> = async {
        for k in 0..fresh.len() {
            if !inner.gap_for(id, crate::SWITCH_GAP).await {
                delete_candidates(vcp, &fresh[k..], improving).await;
                return Ok(false);
            }
            let it = &items[fresh[k].item];
            let l = label(it);
            inner.update(id, |j| j.view.current = Some(l));
            listening.keep_ready().await;
            let new_wer = if it.lang.asr {
                match transcribe_retry(vcp, &fresh[k].spoken.wav, it.lang).await {
                    Ok(heard) => Some(langs::error_rate(it.lang.verify, &heard, it.lang.code)),
                    Err(Trouble::Down(e)) => {
                        delete_candidates(vcp, &fresh[k..], improving).await;
                        return Err(format!("quality check: {e}"));
                    }
                    Err(Trouble::Failed(e)) => {
                        log::warn!("quality check {}: {e}", label(it));
                        None
                    }
                }
            } else {
                None
            };
            let f = &fresh[k];
            let q = quality::assess(&quality::Input {
                def: &it.def,
                lang: it.lang,
                reference: &f.take.analysis,
                reference_text: it.lang.text,
                reference_wer: f.take.wer,
                new: Some((&f.spoken.analysis, it.lang.verify)),
                new_wer,
            });
            if improving {
                let old_score = it.old.as_ref().and_then(|e| e.quality.as_ref()).map_or(0.5, |q| q.score);
                if q.score > old_score + 0.03 {
                    let mut entry = LibraryEntry { quality: Some(q.clone()), ..f.entry.clone() };
                    entry.backup = req.options.backup && inner.backup(&it.key, &f.take.wav);
                    inner.change(vec![entry], &[]);
                    if let Some(old) = &it.old
                        && let Err(e) = vcp.delete_voice(&old.voice_id).await
                    {
                        log::warn!("improve: can't delete the old {}: {e}", old.voice_name);
                    }
                    log::info!("improved {}: {old_score:.2} → {:.2}", label(it), q.score);
                    // still odd: another round, if there is one
                    inner.round_done(id, req, &it.key, true, None);
                } else {
                    let _ = vcp.delete_voice(&f.entry.voice_id).await;
                    inner.round_done(id, req, &it.key, false, None);
                }
            } else {
                let entry = LibraryEntry { quality: Some(q), ..f.entry.clone() };
                inner.change(vec![entry], &[]);
            }
            inner.update(id, |j| j.view.step += 1);
        }
        Ok(true)
        }
        .await;
        listening.end(inner).await;
        if !outcome? {
            return Ok(0);
        }
        if !improving {
            let suspects: Vec<Suspect> = fresh.iter().map(|f| Suspect { key: f.entry.key.clone(), voice_id: f.entry.voice_id.clone(), lang: items[f.item].lang, label: label(&items[f.item]), reference: f.take.analysis.clone() }).collect();
            double_check(inner, vcp, id, suspects).await?;
        }
        inner.relate(items.iter().map(|it| &it.def));
        inner.recount(id);
    }
    Ok(saved)
}

/// A voice that may need a second opinion.
struct Suspect {
    key: String,
    voice_id: String,
    lang: &'static langs::DesignLang,
    label: String,
    reference: Analysis,
}

/// Voices listed only for how they spoke the one test sentence say what
/// their sample says. Most of them are fine: computing them again would be
/// wasted. Each model does its part in one go.
async fn double_check(inner: &Inner, vcp: &ttv_vcp::Client, id: &str, voices: Vec<Suspect>) -> Result<(), String> {
    let voices: Vec<Suspect> = voices.into_iter().filter(|v| inner.library.get(&v.key).and_then(|e| e.quality).is_some_and(|q| quality::only_new_sentence(&q))).collect();
    if voices.is_empty() {
        return Ok(());
    }
    inner.phase(id, "confirm", voices.len() * 2);
    let mut spoken = Vec::with_capacity(voices.len());
    for v in &voices {
        if !inner.gap(id).await {
            return Ok(());
        }
        let l = v.label.clone();
        inner.update(id, |j| j.view.current = Some(l));
        spoken.push(match crate::speak(vcp, &v.voice_id, v.lang.text, v.lang).await {
            Ok(s) => Some(s),
            Err(Trouble::Down(e)) => return Err(e),
            Err(Trouble::Failed(e)) => {
                log::warn!("second opinion {}: {e}", v.label);
                None
            }
        });
        inner.update(id, |j| j.view.step += 1);
    }
    let first = voices.iter().zip(&spoken).find_map(|(v, s)| s.as_ref().filter(|_| v.lang.asr).map(|s| (s.wav.clone(), v.lang)));
    let listening = Listening::begin(inner, vcp, first.as_ref().map(|f| f.0.as_slice()), first.as_ref().map_or(voices[0].lang, |f| f.1)).await;
    let outcome: Result<(), String> = async {
    for (v, s) in voices.iter().zip(spoken) {
        if !inner.gap_for(id, crate::SWITCH_GAP).await {
            return Ok(());
        }
        let Some(s) = s else { continue };
        let l = v.label.clone();
        inner.update(id, |j| j.view.current = Some(l));
        listening.keep_ready().await;
        let wer = if v.lang.asr {
            match transcribe_retry(vcp, &s.wav, v.lang).await {
                Ok(heard) => Some(langs::error_rate(v.lang.text, &heard, v.lang.code)),
                Err(Trouble::Down(e)) => return Err(e),
                Err(Trouble::Failed(_)) => None,
            }
        } else {
            None
        };
        if let Some(e) = inner.library.get(&v.key)
            && let Some(mut q) = e.quality.clone()
            && quality::second_opinion(&mut q, &v.reference, &s.analysis, wer, max_wer(v.lang))
        {
            log::info!("{}: the test sentence was a fluke", v.label);
            inner.change(vec![LibraryEntry { quality: Some(q), ..e }], &[]);
        }
        inner.update(id, |j| j.view.step += 1);
    }
    Ok(())
    }
    .await;
    listening.end(inner).await;
    outcome
}

/// When a batch stops halfway, candidates that never made it into the
/// library are deleted again (VCP keeps no voices the library doesn't know).
async fn delete_candidates(vcp: &ttv_vcp::Client, fresh: &[Fresh], improving: bool) {
    if improving {
        for f in fresh {
            let _ = vcp.delete_voice(&f.entry.voice_id).await;
        }
    }
}

async fn transcribe_retry(vcp: &ttv_vcp::Client, wav: &[u8], lang: &langs::DesignLang) -> Result<String, Trouble> {
    // One dropped connection shouldn't cost the batch: try once more.
    let mut heard = transcribe(vcp, wav, lang).await;
    if matches!(heard, Err(Trouble::Down(_))) {
        tokio::time::sleep(Duration::from_secs(2)).await;
        heard = transcribe(vcp, wav, lang).await;
    }
    heard
}

/// What the review heard of one voice in its speaking phase.
struct Heard {
    analysis: Analysis,
    /// what the sample says, if VCP knows it
    text: Option<String>,
    wav: Vec<u8>,
    spoken: Option<Spoken>,
    /// why it couldn't speak a new sentence
    failure: Option<String>,
}

/// Checks computed voices: their reference sample, and how they speak a new
/// sentence. Returns how many voices were checked.
async fn review(inner: &Inner, vcp: &ttv_vcp::Client, id: &str, items: Vec<Item>) -> Result<u32, String> {
    // 1. speak a new sentence with each voice (speaking checkpoint)
    inner.phase(id, "speak", items.len());
    let mut heard: Vec<Option<Heard>> = Vec::new();
    for it in &items {
        if !inner.gap(id).await {
            return Ok(0);
        }
        let l = label(it);
        inner.update(id, |j| j.view.current = Some(l));
        let old = it.old.as_ref().expect("review items exist");
        let sample = match vcp.voice_sample(&old.voice_id).await {
            Ok(s) => s,
            Err(e) => match Trouble::from(e) {
                Trouble::Down(e) => return Err(e),
                Trouble::Failed(e) => {
                    log::warn!("review {}: no reference sample: {e}", label(it));
                    heard.push(None);
                    inner.update(id, |j| j.view.step += 1);
                    continue;
                }
            },
        };
        let wav = base64::engine::general_purpose::STANDARD.decode(sample.audio_base64.as_bytes()).unwrap_or_default();
        let analysis = audio::decode(&wav).map(|(s, r)| quality::analyze(&s, r)).unwrap_or_default();
        let text = sample.spoken_text().map(str::to_string);
        let (spoken, failure) = match crate::prime(vcp, &old.voice_id, it.lang).await {
            Ok(s) => (Some(s), None),
            Err(Trouble::Down(e)) => return Err(e),
            Err(Trouble::Failed(e)) => (None, Some(e)),
        };
        heard.push(Some(Heard { analysis, text, wav, spoken, failure }));
        inner.update(id, |j| j.view.step += 1);
    }

    // 2. listen to both (speech recognition), assess
    inner.phase(id, "review", items.len());
    let mut checked = 0;
    let mut suspects = Vec::new();
    let first = items.iter().zip(&heard).find_map(|(it, h)| h.as_ref().and_then(|h| h.spoken.as_ref()).filter(|_| it.lang.asr).map(|s| (s.wav.clone(), it.lang)));
    let listening = Listening::begin(inner, vcp, first.as_ref().map(|f| f.0.as_slice()), first.as_ref().map_or(items[0].lang, |f| f.1)).await;
    let outcome: Result<bool, String> = async {
    for (it, h) in items.iter().zip(heard) {
        if !inner.gap_for(id, crate::SWITCH_GAP).await {
            return Ok(false);
        }
        let l = label(it);
        inner.update(id, |j| j.view.current = Some(l));
        listening.keep_ready().await;
        let old = it.old.clone().expect("review items exist");
        let Some(Heard { analysis, text, wav, spoken, failure }) = h else {
            inner.update(id, |j| {
                j.processed.push(it.key.clone());
                j.view.done += 1;
                j.view.step += 1;
            });
            continue;
        };
        // what the sample says: TTV-TTS voices know it; 1.x voices are checked now
        let reference_text = text.unwrap_or_else(|| it.lang.text.to_string());
        let mut reference_wer = old.wer;
        if reference_wer.is_none() && it.lang.asr {
            match transcribe_retry(vcp, &wav, it.lang).await {
                Ok(h) => {
                    let w = langs::error_rate(&reference_text, &h, it.lang.code);
                    // way off: VCP's stored text isn't what the sample says
                    reference_wer = (w < 0.6).then_some(w);
                }
                Err(Trouble::Down(e)) => return Err(e),
                Err(Trouble::Failed(_)) => {}
            }
        }
        let new_wer = match &spoken {
            Some(s) if it.lang.asr => match transcribe_retry(vcp, &s.wav, it.lang).await {
                Ok(h) => Some(langs::error_rate(it.lang.verify, &h, it.lang.code)),
                Err(Trouble::Down(e)) => return Err(e),
                Err(Trouble::Failed(_)) => None,
            },
            _ => None,
        };
        let mut q = quality::assess(&quality::Input {
            def: &it.def,
            lang: it.lang,
            reference: &analysis,
            reference_text: &reference_text,
            reference_wer,
            new: spoken.as_ref().map(|s| (&s.analysis, it.lang.verify)),
            new_wer,
        });
        if let Some(f) = failure {
            q.issues.push(ttv_link::QualityIssue { code: "new_failed".into(), value: 0.0, weight: 0.9, extra: Some(f) });
            q.score = (q.score * 0.1 * 1000.0).round() / 1000.0;
        }
        q.kept = old.quality.as_ref().is_some_and(|o| o.kept) && q.score >= old.quality.as_ref().map_or(0.0, |o| o.score) - 0.05;
        let level = analysis.level.or(old.level);
        let gain = mix_levels(level, spoken.as_ref().and_then(|s| s.analysis.level)).map_or(old.gain, audio::gain_from_db);
        suspects.push(Suspect { key: it.key.clone(), voice_id: old.voice_id.clone(), lang: it.lang, label: label(it), reference: analysis.clone() });
        inner.change(vec![LibraryEntry { quality: Some(q), level, gain, wer: reference_wer.or(old.wer), ..old }], &[]);
        checked += 1;
        inner.update(id, |j| {
            j.processed.push(it.key.clone());
            j.view.done += 1;
            j.view.step += 1;
        });
    }
    Ok(true)
    }
    .await;
    listening.end(inner).await;
    if !outcome? {
        return Ok(checked);
    }
    double_check(inner, vcp, id, suspects).await?;
    inner.relate(items.iter().map(|it| &it.def));
    inner.recount(id);
    Ok(checked)
}

/// Saves the next voices of a pack in VCP (no designing): sample → VCP →
/// prime → library. Voices the library has already are skipped.
async fn import(inner: &Inner, vcp: &ttv_vcp::Client, id: &str, job: &crate::StoredJob) -> Result<(), String> {
    let path = std::path::PathBuf::from(&job.request.pack);
    let manifest = match crate::pack::read_manifest(&path) {
        Ok(m) => m,
        Err(e) => {
            inner.update(id, |j| {
                j.view.state = JobState::Failed;
                j.view.error = Some(format!("The pack can't be read: {e}"));
                j.view.finished = Some(chrono::Utc::now().timestamp());
            });
            inner.save_jobs();
            return Ok(());
        }
    };
    let todo: Vec<&ttv_link::PackVoice> = manifest.voices.iter().filter(|v| !inner.library.contains(&v.key) && !job.failed.contains(&v.key) && !job.processed.contains(&v.key)).take(BATCH).collect();
    if todo.is_empty() {
        finish(inner, id, job);
        // an uploaded pack isn't needed any more
        if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("incoming-")) {
            let _ = std::fs::remove_file(&path);
        }
        return Ok(());
    }
    let t0 = Instant::now();
    inner.update(id, |j| {
        j.view.state = JobState::Running;
        j.view.error = None;
        j.view.batch = todo.len() as u32;
    });
    inner.phase(id, "import", todo.len());
    let mut reader = crate::pack::PackReader::open(&path)?;
    let mut done = 0u32;
    for v in todo {
        if !inner.gap(id).await {
            return Ok(());
        }
        let l = format!("{} · {}", v.name, v.lang);
        inner.update(id, |j| j.view.current = Some(l));
        let lang = langs::get(&v.lang).expect("checked by read_manifest");
        let (flac, samples, rate) = match reader.sample(&v.file) {
            Ok(s) => s,
            Err(e) => {
                inner.import_failed(id, &v.key, format!("{}: {e}", v.name));
                continue;
            }
        };
        let duration = samples.len() as f32 / rate.max(1) as f32;
        let voice_name = format!("TTV {} {} {:03} {}", v.preset_id, v.lang, v.index + 1, v.name);
        let save = serde_json::json!({
            "name": voice_name,
            "audio_base64": base64::engine::general_purpose::STANDARD.encode(audio::wav_pcm16(&samples, rate)),
            "transcript": v.description,
            "ref_text": v.ref_text,
            "voice_type": "design",
            "transcript_manual": true,
        });
        let (voice, spoken) = match save_voice(vcp, &save, lang).await {
            Ok(x) => x,
            Err(Trouble::Down(e)) => return Err(e),
            Err(Trouble::Failed(e)) => {
                inner.import_failed(id, &v.key, format!("{}: {e}", v.name));
                continue;
            }
        };
        let level = v.level.or_else(|| crate::audio::loudness_db(&samples));
        let backup = std::fs::write(inner.sample_path(&v.key), &flac).is_ok();
        inner.change(
            vec![LibraryEntry {
                key: v.key.clone(),
                def_hash: v.def_hash.clone(),
                lang: v.lang.clone(),
                preset_id: v.preset_id.clone(),
                index: v.index,
                name: v.name.clone(),
                voice_id: voice.id.clone(),
                voice_name,
                created: chrono::Utc::now().timestamp(),
                duration,
                wer: v.wer,
                gain: audio::gain_from_db(mix_levels(level, spoken.analysis.level).unwrap_or(-20.0)),
                backup,
                level,
                quality: v.quality.clone(),
                ref_text: None,
            }],
            &[],
        );
        done += 1;
        inner.update(id, |j| {
            j.processed.push(v.key.clone());
            j.view.done += 1;
            j.view.step += 1;
        });
    }
    if done > 0 {
        let per = t0.elapsed().as_secs_f32() / done as f32;
        inner.update(id, |j| {
            let left = j.view.total.saturating_sub(j.view.done + j.view.failed);
            j.view.eta_secs = Some((left as f32 * per).round() as u64);
        });
    }
    inner.save_jobs();
    Ok(())
}

/// Brings archived voices back into VCP from their samples, so they can be
/// designed again. Their entries stay as they are.
async fn restore(inner: &Inner, vcp: &ttv_vcp::Client, id: &str, job: &crate::StoredJob) -> Result<(), String> {
    let todo: Vec<String> = job.request.only.iter().filter(|k| !job.processed.contains(k) && !job.failed.contains(k)).take(BATCH).cloned().collect();
    if todo.is_empty() {
        finish(inner, id, job);
        return Ok(());
    }
    inner.update(id, |j| {
        j.view.state = JobState::Running;
        j.view.error = None;
        j.view.batch = todo.len() as u32;
    });
    inner.phase(id, "import", todo.len());
    for key in todo {
        if !inner.gap(id).await {
            return Ok(());
        }
        let Some(e) = inner.archive.get(&key) else {
            inner.update(id, |j| {
                j.processed.push(key.clone());
                j.view.done += 1;
                j.view.step += 1;
            });
            continue;
        };
        let name = format!("{} · {}", e.name, e.lang);
        let l = name.clone();
        inner.update(id, |j| j.view.current = Some(l));
        let Some(lang) = langs::get(&e.lang) else {
            inner.import_failed(id, &key, format!("{name}: unknown language"));
            continue;
        };
        let decoded = std::fs::read(inner.sample_path(&key)).map_err(|e| e.to_string()).and_then(|flac| crate::pack::decode_flac(&flac));
        let (samples, rate) = match decoded {
            Ok(s) => s,
            Err(e) => {
                inner.import_failed(id, &key, format!("{name}: {e}"));
                continue;
            }
        };
        let description = job.request.defs.iter().find(|d| d.def.key(&e.lang) == key).map(|d| d.def.description.clone()).unwrap_or_default();
        let save = serde_json::json!({
            "name": e.voice_name,
            "audio_base64": base64::engine::general_purpose::STANDARD.encode(audio::wav_pcm16(&samples, rate)),
            "transcript": description,
            "ref_text": e.ref_text.clone().unwrap_or_else(|| lang.text.to_string()),
            "voice_type": "design",
            "transcript_manual": true,
        });
        let voice = match save_voice(vcp, &save, lang).await {
            Ok((v, _)) => v,
            Err(Trouble::Down(e)) => return Err(e),
            Err(Trouble::Failed(e)) => {
                inner.import_failed(id, &key, format!("{name}: {e}"));
                continue;
            }
        };
        inner.change(vec![LibraryEntry { voice_id: voice.id, ref_text: None, ..e }], &[]);
        inner.archive.apply(vec![], std::slice::from_ref(&key));
        inner.pending.lock().insert(key.clone());
        inner.save_pending();
        inner.update(id, |j| {
            j.processed.push(key.clone());
            j.view.done += 1;
            j.view.step += 1;
        });
    }
    inner.save_jobs();
    Ok(())
}

impl Inner {
    /// A pack voice that can't be imported (broken sample, VCP refuses it).
    fn import_failed(&self, id: &str, key: &str, why: String) {
        log::warn!("import: {why}");
        self.update(id, |j| {
            j.failed.push(key.to_string());
            j.view.failed += 1;
            j.view.step += 1;
            j.view.error = Some(why);
        });
    }

    pub(crate) fn phase(&self, id: &str, name: &str, steps: usize) {
        let name = name.to_string();
        self.update(id, |j| {
            j.view.state = JobState::Running;
            j.view.phase = name;
            j.view.step = 0;
            j.view.steps = steps as u32;
        });
    }

    /// Writes the lossless backup of a sample; true if it worked.
    pub(crate) fn backup(&self, key: &str, wav: &[u8]) -> bool {
        audio::decode(wav).and_then(|(s, r)| audio::flac(&s, r)).is_some_and(|flac| std::fs::write(self.sample_path(key), flac).is_ok())
    }

    /// One improvement round of `key` is over.
    fn round_done(&self, id: &str, req: &JobRequest, key: &str, improved: bool, why: Option<String>) {
        let rounds = req.rounds.clamp(1, 5) as u32;
        let good = improved && self.library.get(key).and_then(|e| e.quality).is_some_and(|q| q.score >= FLAG);
        self.update(id, |j| {
            let a = j.attempts.entry(key.to_string()).or_insert(0);
            *a += 1;
            if improved {
                j.view.improved += 1;
            }
            if good || *a >= rounds {
                j.processed.push(key.to_string());
                j.view.done += 1;
            }
            j.view.step += 1;
            if why.is_some() {
                j.view.error = why;
            }
        });
    }

    /// How many of the job's voices the quality check lists – after relating
    /// languages, which can tip voices of earlier batches too.
    fn recount(&self, id: &str) {
        let keys: Vec<String> = self.jobs.lock().iter().find(|j| j.view.id == id).map(|j| j.processed.iter().chain(&j.created).cloned().collect()).unwrap_or_default();
        let n = keys.iter().filter(|k| self.library.get(k).and_then(|e| e.quality).is_some_and(|q| q.score < FLAG && !q.kept)).count() as u32;
        self.update(id, |j| j.view.flagged = n);
    }

    /// Re-compares all languages of these definitions ("languages differ").
    pub(crate) fn relate<'a>(&self, defs: impl Iterator<Item = &'a VoiceDef>) {
        let mut seen = HashSet::new();
        let mut entries = self.library.entries();
        let archived: HashSet<String> = self
            .archive
            .entries()
            .into_iter()
            .map(|e| {
                let k = e.key.clone();
                entries.push(e);
                k
            })
            .collect();
        let mut changed = Vec::new();
        for def in defs {
            let hash = def.hash();
            if !seen.insert(hash.clone()) {
                continue;
            }
            let mine: Vec<&LibraryEntry> = entries.iter().filter(|e| e.def_hash == hash && e.quality.is_some()).collect();
            if mine.len() < 2 {
                continue;
            }
            let mut group: Vec<(String, Quality)> = mine.iter().map(|e| (e.lang.clone(), e.quality.clone().unwrap_or_default())).collect();
            if quality::relate(Some(def), &mut group) {
                for (e, (_, q)) in mine.iter().zip(group) {
                    if e.quality.as_ref() != Some(&q) {
                        changed.push(LibraryEntry { quality: Some(q), ..(*e).clone() });
                    }
                }
            }
        }
        let (old, live): (Vec<LibraryEntry>, Vec<LibraryEntry>) = changed.into_iter().partition(|e| archived.contains(&e.key));
        if !live.is_empty() {
            self.change(live, &[]);
        }
        if !old.is_empty() {
            self.archive.apply(old, &[]);
        }
    }

    /// Compares the languages of every definition with three or more of them
    /// again (on start: voices checked by an older version were compared
    /// pairwise, which notes languages that agree with most of the others).
    pub(crate) fn relate_all(&self) -> usize {
        let mut groups: std::collections::HashMap<String, Vec<(LibraryEntry, bool)>> = std::collections::HashMap::new();
        for e in self.library.entries() {
            groups.entry(e.def_hash.clone()).or_default().push((e, false));
        }
        for e in self.archive.entries() {
            groups.entry(e.def_hash.clone()).or_default().push((e, true));
        }
        let (mut live, mut old) = (Vec::new(), Vec::new());
        for members in groups.into_values() {
            let mine: Vec<&(LibraryEntry, bool)> = members.iter().filter(|(e, _)| e.quality.is_some()).collect();
            if mine.len() < 3 {
                continue;
            }
            let mut group: Vec<(String, Quality)> = mine.iter().map(|(e, _)| (e.lang.clone(), e.quality.clone().unwrap_or_default())).collect();
            if quality::relate(None, &mut group) {
                for ((e, archived), (_, q)) in mine.iter().zip(group) {
                    if e.quality.as_ref() != Some(&q) {
                        let x = LibraryEntry { quality: Some(q), ..e.clone() };
                        if *archived { old.push(x) } else { live.push(x) }
                    }
                }
            }
        }
        let n = live.len() + old.len();
        if n > 0 {
            log::info!("quality: compared the languages again, {n} voices changed");
        }
        if !live.is_empty() {
            self.change(live, &[]);
        }
        if !old.is_empty() {
            self.archive.apply(old, &[]);
        }
        n
    }

    /// Attempts used up for `key` in job `id`.
    pub(crate) fn attempts(&self, id: &str, key: &str) -> u32 {
        self.jobs.lock().iter().find(|j| j.view.id == id).and_then(|j| j.attempts.get(key).copied()).unwrap_or(0)
    }

    pub(crate) fn save_pending(&self) {
        let p = self.pending.lock().clone();
        crate::write_json(&self.dir.join("archive-pending.json"), &p);
    }

    /// Archives the voices of archive jobs that no open job needs any more:
    /// not in an open compute job (its quality follow-ups come at its end),
    /// not waiting in a check or improve job.
    pub(crate) async fn archive_finished(&self, vcp: &ttv_vcp::Client) {
        let pending: Vec<String> = self.pending.lock().iter().cloned().collect();
        if pending.is_empty() {
            return;
        }
        let busy: HashSet<String> = {
            let jobs = self.jobs.lock();
            jobs.iter()
                .filter(|j| matches!(j.view.state, JobState::Queued | JobState::Running | JobState::Waiting | JobState::Paused))
                .flat_map(|j| match j.request.kind {
                    JobKind::Compute => j.created.clone(),
                    _ => j.request.only.iter().filter(|k| !j.processed.contains(k) && !j.failed.contains(k)).cloned().collect(),
                })
                .collect()
        };
        let ready: Vec<String> = pending.into_iter().filter(|k| !busy.contains(k) && !crate::plan::keeps(self, k)).collect();
        if ready.is_empty() {
            return;
        }
        match self.archive_keys(vcp, &ready).await {
            Ok(n) => log::info!("archive: {n} finished voices left VCP"),
            Err(e) => log::warn!("archive: {e}"),
        }
    }

    /// Moves voices from the library to the archive: keeps a lossless sample
    /// and what it says, then deletes the voice in VCP.
    pub(crate) async fn archive_keys(&self, vcp: &ttv_vcp::Client, keys: &[String]) -> Result<u32, String> {
        let mut moved = Vec::new();
        let mut gone = Vec::new();
        let mut failure = None;
        for k in keys {
            let Some(e) = self.library.get(k) else {
                // failed or deleted meanwhile
                gone.push(k.clone());
                continue;
            };
            let sample = match vcp.voice_sample(&e.voice_id).await {
                Ok(s) => s,
                Err(err) => {
                    failure = Some(format!("{}: {err}", e.voice_name));
                    break;
                }
            };
            let ref_text = sample.spoken_text().map(str::to_string).or_else(|| langs::get(&e.lang).map(|l| l.text.to_string()));
            if !self.sample_path(k).exists() {
                let wav = base64::engine::general_purpose::STANDARD.decode(sample.audio_base64.as_bytes()).unwrap_or_default();
                if !self.backup(k, &wav) {
                    failure = Some(format!("{}: can't keep its sample", e.voice_name));
                    break;
                }
            }
            if let Err(err) = vcp.delete_voice(&e.voice_id).await {
                failure = Some(format!("VCP refused to delete {}: {err}", e.voice_name));
                break;
            }
            moved.push(LibraryEntry { backup: true, ref_text, ..e });
            gone.push(k.clone());
        }
        let n = moved.len() as u32;
        let keys_moved: Vec<String> = moved.iter().map(|e| e.key.clone()).collect();
        self.archive.apply(moved, &[]);
        self.change(vec![], &keys_moved);
        {
            let mut p = self.pending.lock();
            for k in &gone {
                p.remove(k);
            }
        }
        self.save_pending();
        match failure {
            Some(f) => Err(f),
            None => Ok(n),
        }
    }

    /// Measured seconds per voice of a finished batch.
    fn measured(&self, id: &str, kind: JobKind, per: f32) {
        let spv = {
            let mut s = self.stats.lock();
            let slot = match kind {
                JobKind::Review => &mut s.review_secs,
                _ => &mut s.secs_per_voice,
            };
            let v = slot.map_or(per, |old| old * 0.6 + per * 0.4);
            *slot = Some(v);
            v
        };
        self.save_stats();
        self.update(id, |j| {
            j.view.secs_per_voice = Some(spv);
            let left = j.view.total.saturating_sub(j.view.done + j.view.failed);
            j.view.eta_secs = Some((left as f32 * spv).round() as u64);
        });
    }
}
