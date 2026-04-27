use chrono::Local;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

/// Formats tracing timestamps in local time via chrono.
/// Drop-in replacement for the default UTC SystemTime formatter.
pub struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> fmt::Result {
        write!(w, "{}", Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%z"))
    }
}

/// Rolling file writer that rotates at local midnight and names files
/// with local dates. Replaces `tracing_appender::rolling::daily` which
/// uses UTC for both.
pub struct LocalRollingWriter {
    log_dir: PathBuf,
    prefix: String,
    current_file: Option<File>,
    current_date: String,
}

impl LocalRollingWriter {
    pub fn new(log_dir: impl Into<PathBuf>, prefix: impl Into<String>) -> Self {
        let log_dir = log_dir.into();
        let prefix = prefix.into();
        let _ = fs::create_dir_all(&log_dir);
        let today = local_date_string();
        let file = open_log_file(&log_dir, &prefix, &today);
        Self {
            log_dir,
            prefix,
            current_file: file,
            current_date: today,
        }
    }

    #[cfg(test)]
    pub fn force_date_for_testing(&mut self, date: String) {
        self.current_date = date;
    }
}

impl Write for LocalRollingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let today = local_date_string();
        if today != self.current_date {
            if let Some(new_file) = open_log_file(&self.log_dir, &self.prefix, &today) {
                self.current_file = Some(new_file);
                self.current_date = today;
            }
            // On open failure: keep writing to the previous day's file
            // rather than silently discarding. Next write retries the rotation.
        }
        match self.current_file {
            Some(ref mut f) => f.write(buf),
            None => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.current_file {
            Some(ref mut f) => f.flush(),
            None => Ok(()),
        }
    }
}

fn local_date_string() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn open_log_file(dir: &Path, prefix: &str, date: &str) -> Option<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{}.{}", prefix, date)))
        .ok()
}
