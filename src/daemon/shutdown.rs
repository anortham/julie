//! Bounded shutdown drain + recovery markers (A1.7).
//!
//! On shutdown the daemon must wait for in-flight MCP requests to finish so
//! a writer cannot be torn down mid-write. If a request holds the gate past
//! a bounded deadline we abort it and persist a recovery marker so the next
//! daemon startup (and the dashboard / status surface) can flag that the
//! previous run did not drain cleanly.
//!
//! The drain primitive itself is built on top of [`drain_sessions`] from
//! `src/daemon/mod.rs`: the only thing this module adds is the timeout-path
//! observability (DrainOutcome + RecoveryMarker on disk).
//!
//! ## Marker location: single global file
//!
//! Per-workspace markers would be ideal but require the `SessionTracker` to
//! expose which workspaces have *in-flight mutations* (not just attached
//! sessions). Today the tracker reports `current_workspace_id` per session,
//! which can be `None` while a request is being handled. We capture the
//! observable `current_workspace_counts()` snapshot at the moment of timeout
//! as a best-effort `affected_workspaces` list and persist a single global
//! file at `<julie_home>/unclean_shutdown.json`. This matches the plan's
//! fallback ("single global marker is acceptable if per-workspace
//! introspection is genuinely difficult").
//!
//! Multiple unclean shutdowns accumulate as JSON-array entries in the same
//! file so an operator can see history rather than just the most recent.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use super::discovery::{DiscoveryFile, DiscoveryState};
use super::drain_sessions;
use super::session::SessionTracker;
use crate::paths::DaemonPaths;

/// Filename for the recovery-marker JSON file under `julie_home`.
const RECOVERY_MARKER_FILE: &str = "unclean_shutdown.json";

/// Outcome of [`drain_with_markers`].
#[derive(Debug, Clone)]
pub enum DrainOutcome {
    /// All active sessions ended before the timeout. No marker is written.
    Clean,
    /// The drain timer expired with sessions still active. A recovery marker
    /// has been written to disk and the in-flight requests should be aborted.
    TimedOut {
        /// Number of sessions still active at the moment of timeout.
        active_sessions: usize,
    },
}

/// Persistent record of an unclean shutdown.
///
/// Written when [`drain_with_markers`] times out; read at the next daemon
/// startup (via [`read_recovery_markers`]) so operators can see that the
/// previous run aborted in-flight work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryMarker {
    /// UNIX epoch time (microseconds) at the moment of timeout.
    pub shutdown_timestamp_micros: u64,
    /// Configured drain timeout (seconds). May be 0 for sub-second timeouts
    /// (only observed in tests today, where the timeout is set in millis).
    pub drain_timeout_secs: u64,
    /// Session count observed at the moment of timeout — i.e. how many
    /// requests were still in-flight when we gave up waiting.
    pub active_sessions_at_timeout: usize,
    /// Best-effort list of workspace IDs that had sessions attached at the
    /// time of timeout (derived from
    /// `SessionTracker::current_workspace_counts`). Empty if no session was
    /// pinned to a workspace at the moment of timeout.
    #[serde(default)]
    pub affected_workspaces: Vec<String>,
}

/// Path to the global recovery-marker file.
pub fn recovery_marker_path(paths: &DaemonPaths) -> PathBuf {
    paths.julie_home().join(RECOVERY_MARKER_FILE)
}

/// Drain active sessions, bounded by `timeout`. On timeout, persist a
/// `RecoveryMarker` to the daemon home directory before returning
/// `DrainOutcome::TimedOut`.
///
/// This is a thin wrapper around [`drain_sessions`]: it does not itself wake
/// or abort any sessions — it just observes the count, waits, and reports.
/// The caller (`DaemonHandle::shutdown`) is responsible for flipping the
/// HTTP transport into 502-abort mode when this returns `TimedOut`.
// kept for 3d.3 recovery
#[allow(dead_code)]
pub async fn drain_with_markers(
    sessions: &SessionTracker,
    paths: &DaemonPaths,
    timeout: Duration,
) -> DrainOutcome {
    if sessions.is_idle() {
        return DrainOutcome::Clean;
    }
    let drained = drain_sessions(sessions, timeout).await;
    if drained {
        info!("Session drain completed cleanly within timeout");
        return DrainOutcome::Clean;
    }

    // Timeout path: capture the snapshot for the marker before anything has
    // a chance to mutate.
    let active_sessions = sessions.active_count();
    let workspace_counts = sessions.current_workspace_counts();
    let affected_workspaces = workspaces_from_counts(&workspace_counts);

    let shutdown_timestamp_micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);

    let marker = RecoveryMarker {
        shutdown_timestamp_micros,
        drain_timeout_secs: timeout.as_secs(),
        active_sessions_at_timeout: active_sessions,
        affected_workspaces,
    };

    if let Err(e) = append_recovery_marker(paths, &marker) {
        error!(
            error = %e,
            "Failed to persist recovery marker on drain timeout — operators will not see this unclean shutdown surfaced on the next startup"
        );
    } else {
        warn!(
            active_sessions,
            timeout_secs = timeout.as_secs(),
            "Session drain timed out; recovery marker written"
        );
    }

    DrainOutcome::TimedOut { active_sessions }
}

/// Read all persisted recovery markers, oldest first. Returns an empty Vec
/// if no marker file exists or it cannot be parsed (the parse failure is
/// logged but not propagated — recovery surfacing is best-effort).
pub fn read_recovery_markers(paths: &DaemonPaths) -> Vec<RecoveryMarker> {
    let path = recovery_marker_path(paths);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            warn!(
                path = %path.display(),
                error = %e,
                "Failed to read recovery marker file; treating as absent",
            );
            return Vec::new();
        }
    };

    match serde_json::from_slice::<Vec<RecoveryMarker>>(&bytes) {
        Ok(markers) => markers,
        Err(e) => {
            warn!(
                path = %path.display(),
                error = %e,
                "Recovery marker file is corrupt; ignoring",
            );
            Vec::new()
        }
    }
}

/// Remove all persisted recovery markers (operator acknowledgment).
///
/// Idempotent: a missing file is not an error.
pub fn clear_recovery_markers(paths: &DaemonPaths) -> Result<()> {
    let path = recovery_marker_path(paths);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e)
            .with_context(|| format!("failed to remove recovery marker at {}", path.display())),
    }
}

/// Append `marker` to the marker file, preserving any earlier records.
///
/// Atomic-ish: read existing, push, write to a temp file alongside the
/// canonical path, rename. A crash mid-write leaves either the prior file
/// or the new one — never a partial. The temp suffix matches the recipe
/// used by `DiscoveryFile::write_atomic`.
// kept for 3d.3 recovery
#[allow(dead_code)]
fn append_recovery_marker(paths: &DaemonPaths, marker: &RecoveryMarker) -> Result<()> {
    let path = recovery_marker_path(paths);
    let mut existing = read_recovery_markers(paths);
    existing.push(marker.clone());

    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("recovery marker path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create parent dir {}", parent.display()))?;

    let json = serde_json::to_vec_pretty(&existing).context("serialize recovery markers")?;
    write_atomic(&path, &json)?;
    Ok(())
}

/// Atomic file write: tmp + fsync + rename. Mirrors
/// `DiscoveryFile::write_atomic` semantics for the recovery marker file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;

    let tmp = path.with_extension("json.tmp");
    let mut tmp_file = std::fs::File::create(&tmp)
        .with_context(|| format!("create temp file {}", tmp.display()))?;
    tmp_file
        .write_all(bytes)
        .with_context(|| format!("write temp file {}", tmp.display()))?;
    tmp_file
        .sync_all()
        .with_context(|| format!("fsync temp file {}", tmp.display()))?;
    drop(tmp_file);

    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;

    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
    }
    Ok(())
}

/// Flatten a workspace-id-to-count map into a sorted Vec of workspace IDs.
/// Sorting is for stable test output; the count is dropped because the
/// marker only carries the "this workspace had at least one session at
/// timeout" signal.
fn workspaces_from_counts(counts: &HashMap<String, usize>) -> Vec<String> {
    let mut ids: Vec<String> = counts.keys().cloned().collect();
    ids.sort();
    ids
}

/// Rewrite `discovery.json` so its `phase` field reflects `phase`.
///
/// Atomic via `DiscoveryFile::write_atomic` (the existing temp + rename +
/// fsync recipe from A1.3). If the discovery file does not yet exist, this
/// is a no-op: A1.7 introduces the *capability* to flip phase on shutdown;
/// the initial publish lives in A1.8. We log a debug message and return
/// `Ok(())` so shutdown does not error out on a fresh daemon that never
/// reached the publish step.
///
/// If the file exists but is corrupt or stale, we still return `Ok(())`
/// with a warning — the shutdown sequence must not be blocked by a bad
/// discovery file. The corrupt record will be cleaned up by
/// `HttpTransportServer::shutdown` removing `daemon-mcp-transport.json`
/// (a sibling file) when the transport tears down.
pub fn publish_discovery_phase(paths: &DaemonPaths, phase: &str) {
    let path = paths.discovery_file();
    match DiscoveryFile::read_and_validate(&path) {
        DiscoveryState::Live(mut record) => {
            record.phase = Some(phase.to_string());
            if let Err(e) = DiscoveryFile::write_atomic(&path, &record) {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to rewrite discovery.json with phase={}; readers may not see lifecycle change",
                    phase,
                );
            } else {
                info!(phase, "Published discovery.json phase={}", phase);
            }
        }
        DiscoveryState::Missing => {
            // A1.8 will add the initial publish; until then this is the
            // normal path. Debug-level so it does not spam logs.
            tracing::debug!(
                path = %path.display(),
                "discovery.json not present; skipping phase={} publish",
                phase,
            );
        }
        DiscoveryState::Stale => {
            warn!(
                path = %path.display(),
                "discovery.json refers to a stale process; skipping phase={} publish",
                phase,
            );
        }
        DiscoveryState::Corrupt(detail) => {
            warn!(
                path = %path.display(),
                error = %detail,
                "discovery.json is corrupt; skipping phase={} publish",
                phase,
            );
        }
    }
}
