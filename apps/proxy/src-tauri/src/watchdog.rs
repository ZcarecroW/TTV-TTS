//! Starts the proxy again when computing stands still: a job runs, nothing
//! about it moved for `STILL`, VCP is idle and nobody else used VCP for
//! `QUIET`. Every step of a job takes seconds to a few minutes, and a hanging
//! VCP is restarted elsewhere (`restart_stuck_vcp`) – this catches a wedge
//! inside the proxy itself (like the turn that was never given back in
//! 2.3.1), so computing unattended for days goes on by itself. At most one
//! restart per `AGAIN`; another stand-still within it is only logged.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::AppHandle;
use ttv_link::{EngineStatus, JobState};

use crate::state::AppState;

const CHECK: Duration = Duration::from_secs(60);
const STILL: Duration = Duration::from_secs(20 * 60);
const QUIET: Duration = Duration::from_secs(10 * 60);
const AGAIN: Duration = Duration::from_secs(6 * 3600);

/// What the running job looks like; any change counts as progress.
/// `None`: no job is running (waiting for a quiet VCP or nothing to do).
pub fn progress(status: &EngineStatus) -> Option<String> {
    status.jobs.iter().find(|j| j.state == JobState::Running).map(|j| {
        format!("{} {} {}/{} {} {} {} {} {} {:?}", j.id, j.phase, j.step, j.steps, j.batch, j.done, j.failed, j.flagged, j.improved, j.current)
    })
}

/// How long the running job has stood still.
#[derive(Default)]
pub struct Watch {
    last: Option<String>,
    since: Option<Instant>,
}

impl Watch {
    pub fn look(&mut self, now: Option<String>, at: Instant) -> Duration {
        if now.is_none() || now != self.last {
            self.since = now.as_ref().map(|_| at);
            self.last = now;
        }
        self.since.map_or(Duration::ZERO, |s| at.saturating_duration_since(s))
    }
}

/// Stuck: the job stood still long enough while VCP had nothing to do and
/// nobody else (chat, OBS links, friends) used VCP lately.
pub fn stuck(still: Duration, vcp_idle: bool, others_quiet: bool) -> bool {
    still >= STILL && vcp_idle && others_quiet
}

fn mark_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("watchdog.json")
}

/// When the watchdog last restarted the proxy (unix seconds).
fn last_restart(data_dir: &Path) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(mark_path(data_dir)).ok()?).ok()?;
    v["restarted_at"].as_i64()
}

pub fn may_restart(last: Option<i64>, now: i64) -> bool {
    last.is_none_or(|t| now - t >= AGAIN.as_secs() as i64 || now < t)
}

pub async fn run(app: AppHandle, state: Arc<AppState>) {
    let mut watch = Watch::default();
    let mut told = false;
    loop {
        tokio::time::sleep(CHECK).await;
        // dev builds: `.work/vcp` tries the restart path with this file
        let trigger = state.data_dir.join("dev-restart");
        if crate::DEV_BUILD.is_some() && std::fs::remove_file(&trigger).is_ok() {
            log::warn!("dev: restart requested – starting the proxy again");
            restart(&app);
            return;
        }
        // the benchmark has VCP to itself: jobs wait on purpose
        if ttv_vcp::exclusive() {
            continue;
        }
        let still = watch.look(progress(&state.engine.status()), Instant::now());
        let vcp_idle = {
            let v = state.vcp_state.read();
            v.reachable && !v.generating && v.queue_size == 0
        };
        let others_quiet = crate::forward::last_work().is_none_or(|t| t.elapsed() >= QUIET);
        if !stuck(still, vcp_idle, others_quiet) {
            told = false;
            continue;
        }
        let now = chrono::Utc::now().timestamp();
        if !may_restart(last_restart(&state.data_dir), now) {
            if !told {
                log::warn!("computing has stood still for {} minutes while VCP is idle – restarted once already within 6 hours, not again", still.as_secs() / 60);
                told = true;
            }
            continue;
        }
        log::warn!("computing has stood still for {} minutes while VCP is idle – starting the proxy again", still.as_secs() / 60);
        let _ = crate::storage::write_json(&mark_path(&state.data_dir), &serde_json::json!({ "restarted_at": now }));
        restart(&app);
        return;
    }
}

/// Starts a new proxy that waits until this one has ended (it holds the
/// single-instance lock), then ends this one (its exit handler saves the
/// OBS links and closes the router's ports).
pub fn restart(app: &AppHandle) {
    let started = std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe).args(["--minimized", "--after", &std::process::id().to_string()]).spawn()
    });
    match started {
        Ok(_) => app.exit(0),
        Err(e) => log::warn!("starting the proxy again failed: {e}"),
    }
}

/// `--after <pid>`: a restart – wait (at most a minute) until the old
/// process has ended, so its single-instance lock is free.
pub fn wait_for_predecessor() {
    let mut args = std::env::args();
    let Some(pid) = args.by_ref().position(|a| a == "--after").and_then(|_| args.next()).and_then(|p| p.parse::<u32>().ok()) else {
        return;
    };
    let pid = sysinfo::Pid::from_u32(pid);
    let Ok(mut sys) = sysinfo::System::new() else {
        std::thread::sleep(Duration::from_secs(5));
        return;
    };
    let until = Instant::now() + Duration::from_secs(60);
    while Instant::now() < until {
        sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[pid]), true, sysinfo::ProcessRefreshKind::nothing());
        if sys.process(pid).is_none() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_still_is_measured_from_the_last_change() {
        let t0 = Instant::now();
        let mut w = Watch::default();
        assert_eq!(w.look(None, t0), Duration::ZERO, "no job");
        assert_eq!(w.look(Some("a".into()), t0), Duration::ZERO);
        assert_eq!(w.look(Some("a".into()), t0 + Duration::from_secs(600)), Duration::from_secs(600));
        assert_eq!(w.look(Some("b".into()), t0 + Duration::from_secs(700)), Duration::ZERO, "progress resets");
        assert_eq!(w.look(Some("b".into()), t0 + Duration::from_secs(1900)), Duration::from_secs(1200));
        assert_eq!(w.look(None, t0 + Duration::from_secs(2000)), Duration::ZERO, "waiting/no job resets");
        assert_eq!(w.look(Some("b".into()), t0 + Duration::from_secs(2100)), Duration::ZERO, "same view after a pause starts anew");
    }

    #[test]
    fn only_a_quiet_idle_stand_still_counts() {
        assert!(stuck(STILL, true, true));
        assert!(!stuck(STILL - Duration::from_secs(1), true, true));
        assert!(!stuck(STILL, false, true), "VCP works (a long generation is VCP's business)");
        assert!(!stuck(STILL, true, false), "chat used VCP lately: the job lets it go first");
    }

    #[test]
    fn at_most_one_restart_in_six_hours() {
        let now = 1_800_000_000;
        assert!(may_restart(None, now));
        assert!(!may_restart(Some(now - 3600), now));
        assert!(may_restart(Some(now - 6 * 3600), now));
        assert!(may_restart(Some(now + 3600), now), "a clock that went back doesn't block it for good");
    }

    #[test]
    fn progress_is_the_running_job() {
        let mut st = EngineStatus::default();
        assert_eq!(progress(&st), None);
        let mut j = ttv_link::JobView { id: "j".into(), phase: "design".into(), step: 3, steps: 90, state: JobState::Waiting, ..Default::default() };
        st.jobs.push(j.clone());
        assert_eq!(progress(&st), None, "waiting for a quiet VCP is not standing still");
        j.state = JobState::Running;
        st.jobs = vec![j.clone()];
        let a = progress(&st).unwrap();
        j.step = 4;
        st.jobs = vec![j];
        assert_ne!(progress(&st).unwrap(), a);
    }
}
