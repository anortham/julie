//! Per-project log writer for daemon mode.
//!
//! The daemon's tracing subscriber writes to `~/.julie/daemon.log` (everything).
//! This module writes user-facing highlights (tool calls, indexing, session lifecycle)
//! to `{project}/.julie/logs/julie.log.{date}` so `tail -f .julie/logs/julie.log.*`
//! works from the project directory.

use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Internal state guarded by the Mutex.
#[derive(Debug)]
struct LogState {
    file: Option<File>,
    /// Date string (YYYY-MM-DD) of the currently-open file, for midnight rotation.
    date: String,
}

/// Writes formatted log lines to a project's `.julie/logs/` directory.
/// Thread-safe via interior Mutex on the file handle.
#[derive(Debug)]
pub struct ProjectLog {
    log_dir: PathBuf,
    state: Mutex<LogState>,
}

impl ProjectLog {
    /// Create a project logger for the given workspace root.
    /// Creates the log directory if it doesn't exist.
    pub fn new(workspace_root: &Path) -> Self {
        let log_dir = workspace_root.join(".julie").join("logs");
        let _ = fs::create_dir_all(&log_dir);

        let today = Self::today_date();
        let file = Self::open_for_date(&log_dir, &today);

        Self {
            log_dir,
            state: Mutex::new(LogState { file, date: today }),
        }
    }

    /// Write a log line with timestamp, level, and message.
    pub fn log(&self, level: &str, message: &str) {
        let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%z");
        let line = format!("{} {:>5} {}\n", timestamp, level, message);
        let today = Self::today_date();

        if let Ok(mut state) = self.state.lock() {
            // Rotate at midnight: re-open when the date has changed.
            if state.date != today {
                state.file = Self::open_for_date(&self.log_dir, &today);
                state.date = today;
            } else if state.file.is_none() {
                state.file = Self::open_for_date(&self.log_dir, &state.date);
            }

            if let Some(ref mut f) = state.file {
                let _ = f.write_all(line.as_bytes());
                let _ = f.flush();
            }
        }
    }

    /// Log a tool call with timing and result summary.
    pub fn tool_call(&self, tool_name: &str, duration_ms: f64, output_bytes: u64) {
        self.log(
            "INFO",
            &format!(
                "tool_call: {} ({:.1}ms, {} bytes output)",
                tool_name, duration_ms, output_bytes
            ),
        );
    }

    /// Log session start with a pointer to daemon logs.
    pub fn session_start(&self, session_id: &str) {
        self.log(
            "INFO",
            &format!(
                "Session {} connected (daemon mode). Daemon logs at ~/.julie/daemon.log.*",
                session_id
            ),
        );
    }

    /// Log session end.
    pub fn session_end(&self, session_id: &str) {
        self.log("INFO", &format!("Session {} disconnected", session_id));
    }

    /// Log indexing activity.
    pub fn indexing(&self, message: &str) {
        self.log("INFO", &format!("indexing: {}", message));
    }

    fn today_date() -> String {
        Local::now().format("%Y-%m-%d").to_string()
    }

    fn open_for_date(log_dir: &Path, date: &str) -> Option<File> {
        let path = log_dir.join(format!("julie.log.{}", date));
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
    }
}
