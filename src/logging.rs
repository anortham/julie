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
    /// Construct a rolling writer and open today's log file.
    ///
    /// Fails fast if the log directory cannot be created or today's log file
    /// cannot be opened. This is load-bearing for observability: without
    /// fail-fast the writer would silently swallow every `write` (see
    /// `<LocalRollingWriter as Write>::write`'s `None` branch) and callers
    /// like `install_file_tracing` would install a sink that reports
    /// success while dropping every log line — exactly the silent
    /// observability gap an adversarial review caught on the cold-start
    /// diagnostics fix.
    ///
    /// Best-effort behavior is reserved for *mid-run rotation* failures
    /// (next day's file can't open): in that case we keep writing to the
    /// previous day's handle. See `test_rolling_writer_keeps_old_file_on_rotation_failure`.
    pub fn new(log_dir: impl Into<PathBuf>, prefix: impl Into<String>) -> io::Result<Self> {
        let log_dir = log_dir.into();
        let prefix = prefix.into();
        fs::create_dir_all(&log_dir)?;
        let today = local_date_string();
        let file = open_log_file(&log_dir, &prefix, &today).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Failed to open initial log file {}.{} in {}",
                    prefix,
                    today,
                    log_dir.display()
                ),
            )
        })?;
        Ok(Self {
            log_dir,
            prefix,
            current_file: Some(file),
            current_date: today,
        })
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

/// Install a file-writing tracing subscriber for the current process.
///
/// Writes to `<log_dir>/<file_prefix>.<YYYY-MM-DD>`, rotated daily at local
/// midnight. Idempotent within a process: a second call is a no-op. The
/// `WorkerGuard` for the non-blocking appender is parked in a process-wide
/// `OnceLock` so the background writer thread lives for the process lifetime;
/// without it the worker thread would shut down and log writes would silently
/// drop.
///
/// `env_filter_default` is used when the `RUST_LOG` environment variable is
/// unset or unparseable. Pass something like `"julie=info"`.
///
/// Designed to be called from process entry points (`main`) for daemon and
/// adapter alike. Shared across both so the file appender, time formatter,
/// and idempotency contract stay in lockstep.
pub fn install_file_tracing(
    log_dir: &Path,
    file_prefix: &str,
    env_filter_default: &str,
) -> anyhow::Result<()> {
    use std::sync::OnceLock;
    use tracing_appender::non_blocking;
    use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    static TRACING_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

    if TRACING_GUARD.get().is_some() {
        return Ok(());
    }

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(env_filter_default))
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging filter: {}", e))?;

    // Fail-fast on initial directory + file setup. Best-effort here would
    // install a NonBlocking writer that silently drops every log line (see
    // LocalRollingWriter::new docs and its `None` branch in `Write::write`),
    // and callers (adapter, daemon) treat `Ok(())` as "logging is wired".
    // The result would be a silent observability gap — exactly the failure
    // mode this whole change is meant to make impossible.
    let writer = LocalRollingWriter::new(log_dir, file_prefix).map_err(|e| {
        anyhow::anyhow!(
            "Failed to open log file {} in {}: {}",
            file_prefix,
            log_dir.display(),
            e
        )
    })?;
    let (non_blocking_file, file_guard): (NonBlocking, WorkerGuard) = non_blocking(writer);

    let try_init_result = tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_writer(non_blocking_file)
                .with_timer(LocalTimer)
                .with_target(true)
                .with_ansi(false)
                .with_file(true)
                .with_line_number(true),
        )
        .try_init();

    match try_init_result {
        Ok(()) => {
            let _ = TRACING_GUARD.set(file_guard);
            Ok(())
        }
        Err(_) => Ok(()),
    }
}
