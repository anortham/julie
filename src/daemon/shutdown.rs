//! Legacy unclean-shutdown recovery markers (read surface).
//!
//! Earlier daemon builds wrote an `unclean_shutdown.json` marker when a
//! bounded session drain timed out, so the next startup (and the dashboard /
//! status surface) could flag that the previous run did not drain cleanly.
//!
//! The in-process server has no central session-draining shutdown: recovery
//! after an unclean exit is handled by the startup reconcile (the
//! `projected_revision` stamp), not by drain markers. This module is retained
//! only to *read* and surface any marker file left behind by a pre-upgrade
//! daemon install — the in-process server no longer writes new markers.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::paths::DaemonPaths;

/// Filename for the recovery-marker JSON file under `julie_home`.
const RECOVERY_MARKER_FILE: &str = "unclean_shutdown.json";

/// Persistent record of an unclean shutdown left behind by a pre-upgrade
/// daemon build. Surfaced (read-only) via [`read_recovery_markers`] so
/// operators can still see that an older run aborted in-flight work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryMarker {
    /// UNIX epoch time (microseconds) at the moment of timeout.
    pub shutdown_timestamp_micros: u64,
    /// Configured drain timeout (seconds). May be 0 for sub-second timeouts.
    pub drain_timeout_secs: u64,
    /// Session count observed at the moment of timeout — i.e. how many
    /// requests were still in-flight when the drain gave up waiting.
    pub active_sessions_at_timeout: usize,
    /// Best-effort list of workspace IDs that had sessions attached at the
    /// time of timeout. Empty if no session was pinned to a workspace.
    #[serde(default)]
    pub affected_workspaces: Vec<String>,
}

/// Path to the global recovery-marker file.
pub fn recovery_marker_path(paths: &DaemonPaths) -> PathBuf {
    paths.julie_home().join(RECOVERY_MARKER_FILE)
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
