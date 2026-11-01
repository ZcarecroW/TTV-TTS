//! Logging: a small `log` backend that keeps the last lines in memory (for the
//! window and for apps), appends to a log file (rotated at 2 MB) and forwards
//! every line to listeners. Lean on purpose: one lock per line, no formatting
//! work for filtered-out levels.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use parking_lot::Mutex;
use tokio::sync::broadcast;
use ttv_link::LogLine;

const KEEP: usize = 500;
const MAX_FILE: u64 = 2 * 1024 * 1024;

struct Inner {
    lines: VecDeque<LogLine>,
    file: Option<File>,
    path: Option<PathBuf>,
    written: u64,
}

pub struct LogBuf {
    inner: Mutex<Inner>,
    tx: broadcast::Sender<LogLine>,
}

static LOG: OnceLock<LogBuf> = OnceLock::new();

pub fn get() -> &'static LogBuf {
    LOG.get_or_init(|| LogBuf {
        inner: Mutex::new(Inner { lines: VecDeque::with_capacity(KEEP), file: None, path: None, written: 0 }),
        tx: broadcast::channel(256).0,
    })
}

impl LogBuf {
    pub fn lines(&self) -> Vec<LogLine> {
        self.inner.lock().lines.iter().cloned().collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LogLine> {
        self.tx.subscribe()
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.inner.lock().path.clone()
    }

    fn push(&self, line: LogLine) {
        {
            let mut g = self.inner.lock();
            if g.lines.len() == KEEP {
                g.lines.pop_front();
            }
            let text = format!(
                "{} {:5} {}\n",
                chrono::DateTime::from_timestamp_millis(line.t).map(|d| d.format("%Y-%m-%d %H:%M:%S%.3f").to_string()).unwrap_or_default(),
                line.level.to_uppercase(),
                line.message
            );
            if g.written > MAX_FILE {
                rotate(&mut g);
            }
            if let Some(f) = g.file.as_mut()
                && f.write_all(text.as_bytes()).is_ok()
            {
                g.written += text.len() as u64;
            }
            g.lines.push_back(line.clone());
        }
        let _ = self.tx.send(line);
    }
}

fn rotate(g: &mut Inner) {
    if let Some(path) = g.path.clone() {
        g.file = None;
        let _ = std::fs::rename(&path, path.with_extension("old.log"));
        g.file = OpenOptions::new().create(true).append(true).open(&path).ok();
        g.written = 0;
    }
}

struct Logger {
    max: log::LevelFilter,
}

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        // Our own crates log at info; the HTTP/TLS stack only from warn up.
        let ours = m.target().starts_with("ttv");
        m.level() <= if ours { self.max } else { log::LevelFilter::Warn }
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let level = match r.level() {
            log::Level::Error => "error",
            log::Level::Warn => "warn",
            log::Level::Info => "info",
            _ => "debug",
        };
        let message = r.args().to_string();
        #[cfg(debug_assertions)]
        eprintln!("[{level}] {message}");
        get().push(LogLine { t: chrono::Utc::now().timestamp_millis(), level: level.into(), message });
    }

    fn flush(&self) {}
}

/// Installs the logger; `dir` = the app's log folder.
pub fn init(dir: Option<PathBuf>) {
    if let Some(dir) = dir {
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("proxy.log");
        let file = OpenOptions::new().create(true).append(true).open(&path).ok();
        let written = file.as_ref().and_then(|f| f.metadata().ok()).map(|m| m.len()).unwrap_or(0);
        let mut g = get().inner.lock();
        g.file = file;
        g.path = Some(path);
        g.written = written;
    }
    let max = if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    if log::set_boxed_logger(Box::new(Logger { max })).is_ok() {
        log::set_max_level(max);
    }
}
