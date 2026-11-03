//! Keeps this PC from going to sleep while the proxy works: computing
//! voices, a TTV-TTS connected, an OBS link playing, a friend connected.
//! Only sleep is held off – the screen still turns off as set. Switchable
//! (on by default); when nothing is going on, the energy settings decide.
//!
//! - Windows: `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`,
//!   from a thread of its own (the request belongs to the thread that made it).
//! - macOS: `caffeinate -i -w <pid>` (ends by itself with the proxy).
//! - Linux: `systemd-inhibit` around a loop that ends with the proxy.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::state::AppState;

/// Why the PC has to stay awake, by priority. `None`: it may sleep.
pub fn decide(enabled: bool, computing: bool, obs: bool, friends: bool, apps: bool) -> Option<&'static str> {
    if !enabled {
        return None;
    }
    if computing {
        Some("computing")
    } else if obs {
        Some("obs")
    } else if friends {
        Some("friends")
    } else if apps {
        Some("app")
    } else {
        None
    }
}

fn reason(state: &AppState) -> Option<&'static str> {
    decide(
        state.cfg.read().keep_awake,
        state.engine.working(),
        state.hosted.listening(),
        crate::internet::guests_connected() > 0,
        state.connections.load(Ordering::Relaxed) > 0,
    )
}

/// Holds the "don't sleep" request while `set(true)`.
struct Hold {
    #[cfg(windows)]
    tx: std::sync::mpsc::Sender<bool>,
    #[cfg(not(windows))]
    child: Option<std::process::Child>,
}

impl Hold {
    #[cfg(windows)]
    fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let _ = std::thread::Builder::new().name("keep-awake".into()).spawn(move || {
            use windows_sys::Win32::System::Power::{ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
            for on in rx {
                // SAFETY: a plain flag call; it only sets this thread's request.
                let ok = unsafe { SetThreadExecutionState(if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS }) };
                if ok == 0 {
                    log::warn!("keeping the PC awake failed");
                }
            }
            // SAFETY: as above; the request ends with the thread anyway.
            unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
        });
        Self { tx }
    }

    #[cfg(not(windows))]
    fn new() -> Self {
        Self { child: None }
    }

    #[cfg(windows)]
    fn set(&mut self, on: bool) {
        let _ = self.tx.send(on);
    }

    #[cfg(not(windows))]
    fn set(&mut self, on: bool) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if !on {
            return;
        }
        let pid = std::process::id().to_string();
        #[cfg(target_os = "macos")]
        let cmd = std::process::Command::new("caffeinate").args(["-i", "-w", &pid]).spawn();
        #[cfg(not(target_os = "macos"))]
        let cmd = std::process::Command::new("systemd-inhibit")
            .args(["--what=idle:sleep", "--who=TTV-TTS Proxy", "--why=Computing voices / serving speech", "--mode=block", "sh", "-c"])
            .arg(format!("while kill -0 {pid} 2>/dev/null; do sleep 20; done"))
            .spawn();
        match cmd {
            Ok(c) => self.child = Some(c),
            Err(e) => log::info!("can't keep the PC awake: {e}"),
        }
    }
}

/// Checks every 15 seconds; tells the window why the PC is kept awake.
pub async fn run(state: Arc<AppState>) {
    let mut hold = Hold::new();
    let mut last: Option<&'static str> = None;
    loop {
        let why = reason(&state);
        if why.is_some() != last.is_some() {
            hold.set(why.is_some());
            match why {
                Some(w) => log::info!("keeping this PC awake ({w})"),
                None => log::info!("this PC may sleep again (nothing to do)"),
            }
        }
        if why != last {
            last = why;
            *state.awake.write() = why.map(str::to_string);
            state.publish();
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_by_priority_and_the_switch() {
        assert_eq!(decide(true, true, true, true, true), Some("computing"));
        assert_eq!(decide(true, false, true, false, true), Some("obs"));
        assert_eq!(decide(true, false, false, true, true), Some("friends"));
        assert_eq!(decide(true, false, false, false, true), Some("app"));
        assert_eq!(decide(true, false, false, false, false), None, "nothing to do: it may sleep");
        assert_eq!(decide(false, true, true, true, true), None, "switched off");
    }

    /// Windows' own view: does the system have a "system required" request?
    #[cfg(windows)]
    fn system_required() -> bool {
        use windows_sys::Win32::System::Power::{CallNtPowerInformation, ES_SYSTEM_REQUIRED, SystemExecutionState};
        let mut state: u32 = 0;
        // SAFETY: SystemExecutionState writes one u32 into the given buffer.
        let r = unsafe { CallNtPowerInformation(SystemExecutionState, std::ptr::null(), 0, (&mut state as *mut u32).cast(), 4) };
        r == 0 && state & ES_SYSTEM_REQUIRED != 0
    }

    /// The real request on this OS: taken (Windows reports it) and given back.
    #[test]
    fn holds_and_releases() {
        let mut h = Hold::new();
        h.set(true);
        std::thread::sleep(Duration::from_millis(200));
        #[cfg(windows)]
        assert!(system_required(), "Windows sees the request");
        h.set(false);
        std::thread::sleep(Duration::from_millis(100));
        #[cfg(not(windows))]
        assert!(h.child.is_none());
    }
}
