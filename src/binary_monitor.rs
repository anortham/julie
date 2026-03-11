//! Background task that monitors the daemon's own binary for changes.
//!
//! When a rebuild is detected (binary mtime > daemon start time), the
//! monitor triggers graceful shutdown via `CancellationToken`. The
//! `connect` command's reconnect logic then restarts with the new binary.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Check whether the binary at `path` has been modified after `start_time`.
pub fn is_binary_newer(path: &Path, start_time: SystemTime) -> bool {
    match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(mtime) => mtime > start_time,
        Err(e) => {
            debug!("Could not stat binary {:?}: {}", path, e);
            false
        }
    }
}

/// Poll the binary's mtime and cancel the token when a newer build is detected.
///
/// This function runs until either:
/// - A newer binary is detected (cancels `ct` and returns)
/// - The `ct` is cancelled externally (returns without action)
pub async fn run_monitor(
    binary_path: PathBuf,
    ct: CancellationToken,
    poll_interval: Duration,
) {
    let start_time = SystemTime::now();
    info!(
        "Binary monitor started: watching {:?} every {:?}",
        binary_path, poll_interval
    );

    let mut interval = tokio::time::interval(poll_interval);
    interval.tick().await; // First tick is immediate — skip it

    loop {
        tokio::select! {
            _ = ct.cancelled() => {
                debug!("Binary monitor: shutdown requested, exiting");
                return;
            }
            _ = interval.tick() => {
                if is_binary_newer(&binary_path, start_time) {
                    info!("Binary change detected — initiating graceful shutdown for restart");
                    ct.cancel();
                    return;
                }
            }
        }
    }
}

/// Spawn the binary monitor as a background task.
///
/// Returns `None` if:
/// - `JULIE_NO_BINARY_WATCH=1` is set
/// - The current executable path cannot be determined
pub fn spawn(ct: CancellationToken) -> Option<tokio::task::JoinHandle<()>> {
    if std::env::var("JULIE_NO_BINARY_WATCH").unwrap_or_default() == "1" {
        info!("Binary monitor disabled via JULIE_NO_BINARY_WATCH=1");
        return None;
    }

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            warn!("Cannot monitor binary for changes: {}", e);
            return None;
        }
    };

    let ct_clone = ct.clone();
    Some(tokio::spawn(async move {
        run_monitor(exe_path, ct_clone, DEFAULT_POLL_INTERVAL).await;
    }))
}
