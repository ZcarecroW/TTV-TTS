//! The automatic plan: all presets in all languages, without anybody's help.
//!
//! Queueing hundreds of jobs up front made a list nobody could read, and a
//! failed voice stayed failed. The plan holds what should exist, and the
//! worker asks it what's next whenever nothing else is open:
//!
//! language by language: every preset's missing voices, then in one job all
//! voices of that language that were never checked, then all that may sound
//! odd. One long run per model keeps VCP from switching models. Voices that
//! failed get more tries with more takes. Last, odd voices from before the
//! plan come back from the archive. Then the plan is done.

use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ttv_link::{JobDef, JobKind, JobOptions, JobRequest, JobState, LibraryEntry, PlanCell, PlanRequest, PlanRow, PlanState, PlanView};

use crate::quality::FLAG;
use crate::{Inner, langs};

/// Jobs a voice gets before it counts as impossible (each with its attempts).
const MAX_TRIES: u32 = 3;
/// Improve jobs per voice; each designs it up to twice more.
const MAX_IMPROVES: u32 = 2;
const ROUNDS: u8 = 2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Plan {
    request: PlanRequest,
    state: PlanState,
    started: i64,
    finished: Option<i64>,
    /// key -> jobs that ended without a result for this voice
    tries: HashMap<String, u32>,
    /// key -> improve jobs it went through
    improves: HashMap<String, u32>,
    /// Listed only for one test sentence, and got a second opinion.
    rechecked: std::collections::HashSet<String>,
}

impl Plan {
    pub(crate) fn running(&self) -> bool {
        self.state == PlanState::Running
    }
}

fn path(inner: &Inner) -> std::path::PathBuf {
    inner.dir.join("plan.json")
}

pub(crate) fn load(dir: &std::path::Path) -> Option<Plan> {
    crate::read_json(&dir.join("plan.json"))
}

fn save(inner: &Inner) {
    if let Some(p) = inner.plan.lock().as_ref() {
        crate::write_json(&path(inner), p);
    }
}

fn odd(e: &LibraryEntry) -> bool {
    e.quality.as_ref().is_some_and(|q| q.score < FLAG && !q.kept)
}

fn voices<'a>(plan: &'a Plan, lang: &'a str) -> impl Iterator<Item = (&'a JobDef, String)> + 'a {
    plan.request.presets.iter().flat_map(|p| p.defs.iter()).map(move |d| (d, d.def.key(lang)))
}

/// Starts or replaces the plan. Archive jobs that haven't started yet go: the
/// plan covers them (a long queue is what it replaces), checks and
/// improvements too, in one run per language.
pub(crate) fn start(inner: &Inner, req: PlanRequest) -> Result<PlanView, String> {
    if req.presets.is_empty() || req.presets.iter().all(|p| p.defs.is_empty()) {
        return Err("nothing to compute".into());
    }
    if req.languages.is_empty() {
        return Err("Pick at least one language.".into());
    }
    if let Some(l) = req.languages.iter().find(|l| langs::get(l).is_none()) {
        return Err(format!("language {l} can't be computed"));
    }
    let mut removed = 0;
    inner.jobs.lock().retain(|j| {
        let unstarted = j.request.kind != JobKind::Import && j.request.options.archive && j.view.done == 0 && j.created.is_empty() && matches!(j.view.state, JobState::Queued | JobState::Paused);
        removed += u32::from(unstarted);
        !unstarted
    });
    if removed > 0 {
        log::info!("plan: {removed} jobs that hadn't started are covered by the plan");
    }
    *inner.plan.lock() = Some(Plan { request: req, started: Utc::now().timestamp(), ..Default::default() });
    save(inner);
    inner.save_jobs();
    inner.plan_rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    inner.wake_up();
    log::info!("the automatic plan starts");
    view(inner).ok_or_else(|| "no plan".to_string())
}

pub(crate) fn control(inner: &Inner, action: &str) -> Result<(), String> {
    {
        let mut plan = inner.plan.lock();
        let p = plan.as_mut().ok_or("There is no plan.")?;
        match action {
            "pause" if p.state == PlanState::Running => p.state = PlanState::Paused,
            "resume" if p.state == PlanState::Paused => p.state = PlanState::Running,
            "stop" => *plan = None,
            "pause" | "resume" => return Ok(()),
            _ => return Err("unknown action".into()),
        }
    }
    if inner.plan.lock().is_none() {
        let _ = std::fs::remove_file(path(inner));
    }
    save(inner);
    inner.plan_rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    inner.wake_up();
    Ok(())
}

/// A job of the plan ended: what it couldn't do counts as a try.
pub(crate) fn job_finished(inner: &Inner, kind: JobKind, failed: &[String], only: &[String]) {
    {
        let mut plan = inner.plan.lock();
        let Some(p) = plan.as_mut() else { return };
        match kind {
            JobKind::Compute => {
                for k in failed {
                    *p.tries.entry(k.clone()).or_insert(0) += 1;
                }
            }
            JobKind::Improve => {
                for k in only {
                    *p.improves.entry(k.clone()).or_insert(0) += 1;
                }
            }
            JobKind::Review => {
                for k in only {
                    *p.tries.entry(k.clone()).or_insert(0) += 1;
                    p.rechecked.insert(k.clone());
                }
            }
            _ => {}
        }
    }
    save(inner);
}

/// Whether the plan still needs this voice in VCP: it waits for its check or
/// improvement and may not go to the archive yet.
pub(crate) fn keeps(inner: &Inner, key: &str) -> bool {
    let plan = inner.plan.lock();
    let Some(p) = plan.as_ref().filter(|p| p.state != PlanState::Done) else { return false };
    if !p.request.options.quality {
        return false;
    }
    inner.library.get(key).is_some_and(|e| match &e.quality {
        None => p.tries.get(key).copied().unwrap_or(0) < MAX_TRIES,
        Some(_) => p.request.options.improve && odd(&e) && p.improves.get(key).copied().unwrap_or(0) < MAX_IMPROVES,
    })
}

/// Nothing is open: queues the plan's next job. False if it has none.
pub(crate) fn advance(inner: &Inner) -> bool {
    let Some(plan) = inner.plan.lock().clone() else { return false };
    if plan.state != PlanState::Running {
        return false;
    }
    // a paused job of the plan holds the plan
    if inner.jobs.lock().iter().any(|j| j.request.plan && j.view.state == JobState::Paused) {
        return false;
    }
    match next(inner, &plan) {
        Some(req) => match inner.submit(req) {
            Ok(v) if v.total > 0 => true,
            Ok(v) => {
                // nothing to do after all: don't come back to it at once
                log::info!("plan: {} has nothing to do", v.preset_name);
                false
            }
            Err(e) => {
                log::warn!("plan: {e}");
                false
            }
        },
        None => {
            if let Some(p) = inner.plan.lock().as_mut() {
                p.state = PlanState::Done;
                p.finished = Some(Utc::now().timestamp());
            }
            save(inner);
            log::info!("the automatic plan is finished");
            inner.plan_rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            false
        }
    }
}

fn job(plan: &Plan, kind: JobKind, lang: &str, name: &str, keys: Vec<String>) -> JobRequest {
    let wanted: std::collections::HashSet<&str> = keys.iter().map(String::as_str).collect();
    let defs = voices(plan, lang).filter(|(_, k)| wanted.contains(k.as_str())).map(|(d, _)| d.clone()).collect();
    JobRequest { preset_name: name.to_string(), defs, languages: vec![lang.to_string()], options: plan.request.options.clone(), kind, only: keys, plan: true, ..Default::default() }
}

/// What the plan wants done next.
fn next(inner: &Inner, plan: &Plan) -> Option<JobRequest> {
    let o = &plan.request.options;
    for lang in &plan.request.languages {
        // 1. every voice exists
        for p in &plan.request.presets {
            let missing: Vec<&JobDef> = p.defs.iter().filter(|d| {
                let k = d.def.key(lang);
                !inner.library.contains(&k) && !inner.archive.contains(&k) && plan.tries.get(&k).copied().unwrap_or(0) < MAX_TRIES
            }).collect();
            if missing.is_empty() {
                continue;
            }
            // what failed gets more takes to choose from
            let retry = missing.iter().any(|d| plan.tries.contains_key(&d.def.key(lang)));
            let options = JobOptions { takes: if retry { 3 } else { o.takes }, ..o.clone() };
            return Some(JobRequest {
                preset_id: p.id.clone(),
                preset_name: p.name.clone(),
                defs: missing.into_iter().cloned().collect(),
                languages: vec![lang.clone()],
                options,
                plan: true,
                ..Default::default()
            });
        }
        if !o.quality {
            continue;
        }
        // 2. every voice of the language was checked, in one run
        let entries: Vec<LibraryEntry> = voices(plan, lang).filter_map(|(_, k)| inner.library.get(&k)).collect();
        let unchecked: Vec<String> = entries.iter().filter(|e| e.quality.is_none() && plan.tries.get(&e.key).copied().unwrap_or(0) < MAX_TRIES).map(|e| e.key.clone()).collect();
        if !unchecked.is_empty() {
            return Some(job(plan, JobKind::Review, lang, "", unchecked));
        }
        // a second opinion before computing anything again
        let doubtful: Vec<String> = entries.iter().filter(|e| e.quality.as_ref().is_some_and(crate::quality::only_new_sentence) && !plan.rechecked.contains(&e.key)).map(|e| e.key.clone()).collect();
        if !doubtful.is_empty() {
            return Some(job(plan, JobKind::Review, lang, "", doubtful));
        }
        // 3. odd voices of the language are designed again, in one run
        if o.improve {
            let keys: Vec<String> = entries.iter().filter(|e| odd(e) && plan.improves.get(&e.key).copied().unwrap_or(0) < MAX_IMPROVES).map(|e| e.key.clone()).collect();
            if !keys.is_empty() {
                let mut req = job(plan, JobKind::Improve, lang, "", keys);
                req.rounds = ROUNDS;
                return Some(req);
            }
        }
    }
    // 4. odd voices from before the plan: back into VCP, then step 3 takes
    // them (a voice comes back once: then it counts as improved)
    if o.improve {
        for lang in &plan.request.languages {
            let keys: Vec<String> = voices(plan, lang).filter_map(|(_, k)| inner.archive.get(&k)).filter(|e| odd(e) && !plan.improves.contains_key(&e.key)).map(|e| e.key).collect();
            if !keys.is_empty() {
                return Some(job(plan, JobKind::Restore, lang, "", keys));
            }
        }
    }
    None
}

/// What the plan looks like: cheap enough for every status request, because
/// it's only worked out again when voices or the plan changed.
pub(crate) fn view(inner: &Inner) -> Option<PlanView> {
    let plan = inner.plan.lock().clone()?;
    let rev = (inner.library.rev(), inner.archive.rev(), inner.plan_rev.load(std::sync::atomic::Ordering::Relaxed));
    let mut cache = inner.plan_view.lock();
    if let Some((r, v)) = cache.as_ref()
        && *r == rev
    {
        return Some(with_eta(inner, v.clone()));
    }
    let mut rows = Vec::new();
    let (mut total, mut done, mut odds, mut gave_up) = (0, 0, 0, 0);
    let langs = plan.request.languages.clone();
    let (lib, arc) = (inner.library.quality_map(), inner.archive.quality_map());
    for p in &plan.request.presets {
        let mut cells = Vec::with_capacity(langs.len());
        for lang in &langs {
            let mut c = PlanCell::default();
            for d in &p.defs {
                let k = d.def.key(lang);
                match lib.get(&k).or_else(|| arc.get(&k)) {
                    Some(bad) => {
                        c.done += 1;
                        c.odd += u32::from(*bad);
                    }
                    None => c.gave_up += u32::from(plan.tries.get(&k).copied().unwrap_or(0) >= MAX_TRIES),
                }
            }
            total += p.defs.len() as u32;
            done += c.done;
            odds += c.odd;
            gave_up += c.gave_up;
            cells.push(c);
        }
        rows.push(PlanRow { id: p.id.clone(), name: p.name.clone(), total: p.defs.len() as u32, cells });
    }
    let v = PlanView { state: plan.state, languages: langs, rows, voices_total: total, voices_done: done, voices_odd: odds, gave_up, started: plan.started, finished: plan.finished, eta_secs: None };
    *cache = Some((rev, v.clone()));
    Some(with_eta(inner, v))
}

fn with_eta(inner: &Inner, mut v: PlanView) -> PlanView {
    if v.state != PlanState::Done {
        let left = v.voices_total.saturating_sub(v.voices_done + v.gave_up);
        // about one voice in ten is designed again
        v.eta_secs = inner.stats.lock().secs_per_voice.map(|s| (left as f32 * s * 1.1) as u64);
    }
    v
}
