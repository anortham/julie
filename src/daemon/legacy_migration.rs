//! A1.5: Hard legacy-migration gate.
//!
//! The daemon split (julie-server → julie-adapter + julie-daemon) ships in
//! a release where existing users may have a legacy `julie-server daemon`
//! running on the host. Both legacy and new daemons read/write the same
//! workspace SQLite + Tantivy index files; if both run concurrently, those
//! indexes corrupt silently. This module is the safety gate.
//!
//! ## Behavior
//!
//! - `check_or_refuse(paths)` — call this at `julie-daemon start` time.
//!   Inspects every legacy file on disk (`daemon.singleton.lock`,
//!   `daemon.pid`, `daemon.state`, `daemon.lock`). If ANY of them is owned
//!   by a live process, returns `LegacyDaemonAlive` so the caller can refuse
//!   to start. Otherwise returns `ProceedAndUnlink` with a list of stale
//!   files for the caller to remove.
//!
//! - `detect_and_attach(paths)` — call this at `julie-adapter` startup. If
//!   `daemon-mcp-transport.json` is readable AND `daemon.pid` corresponds
//!   to a live process, returns a `TransportEndpoint` pointing at the legacy daemon's
//!   HTTP server. The adapter can then forward MCP traffic to it without
//!   spawning a duplicate daemon.
//!
//! ## Design bias
//!
//! Conservative: prefer false-positives (refuse when uncertain) over
//! false-negatives (start alongside a live daemon and corrupt indexes).
//! Any `Indeterminate` PID status is treated as legacy-alive. Any I/O error
//! probing the singleton lock that isn't a clean `WouldBlock` or "file
//! doesn't exist" is treated as legacy-alive.
//!
//! ## Path collision: `daemon.lock`
//!
//! A1.2's `DaemonLockGuard` uses the SAME path (`~/.julie/daemon.lock`) as
//! the legacy adapter launcher's startup-serialization lock. This is
//! intentional: during the cutover period, if `daemon.lock` is held by ANY
//! process, the gate must refuse — either a legacy daemon holds it (so we
//! must not start) or the new daemon holds it (so we cannot start either
//! way, by definition). Treating "held by anything" as legacy-alive is
//! always safe in this window.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use tracing::debug;

use crate::daemon::pid::{PidFile, PidFileStatus};
use crate::daemon::singleton::{SingletonLock, SingletonLockError};
use crate::daemon::transport::TransportEndpoint;
use crate::paths::DaemonPaths;

/// Outcome of `check_or_refuse`. Either a live legacy daemon is present
/// (the new daemon must NOT start) or all legacy signals are dead/absent
/// (the new daemon may proceed after unlinking the listed files).
#[derive(Debug)]
pub enum MigrationDecision {
    /// A live legacy daemon owns one or more legacy files. The new daemon
    /// must refuse to start. The pid is the owner of whichever signal
    /// gave the strongest evidence (PID file > singleton lock > other).
    /// The hint string is a one-line operator-facing diagnostic.
    LegacyDaemonAlive { pid: u32, hint: String },

    /// No live legacy daemon detected. Safe to proceed after cleaning up
    /// the listed files. The list contains the files that still exist on
    /// disk after the gate's classification side-effects (some legacy
    /// files, e.g. dead `daemon.pid`, are already unlinked by
    /// `PidFile::check_status` as a side effect — those won't appear here).
    ProceedAndUnlink { files_to_clean: Vec<PathBuf> },
}

/// Examine all legacy daemon files in `paths` and decide whether the new
/// daemon may start.
///
/// **Pure-ish**: this function does NOT unlink files itself (caller does).
/// It DOES inherit `PidFile::check_status`'s side-effect of unlinking a
/// clearly-dead `daemon.pid`; that's a pre-existing semantic of that
/// helper and outside this function's contract.
///
/// Returns `Err` only for filesystem errors so unexpected that no decision
/// can be made (e.g. JULIE_HOME directory unreadable). Most error modes
/// are absorbed into the `LegacyDaemonAlive` branch with a diagnostic hint
/// — refusing on unknown errors is the safe default.
pub fn check_or_refuse(paths: &DaemonPaths) -> Result<MigrationDecision> {
    let mut files_to_clean: Vec<PathBuf> = Vec::new();

    // 1. daemon.singleton.lock — fcntl-probe via SingletonLock::try_acquire.
    //
    // try_acquire opens the file (creating if absent — but we only probe
    // the existing-file case, so guard with .exists() first to avoid
    // creating a phantom lock file in an otherwise-clean JULIE_HOME).
    let singleton_lock_path = paths.daemon_singleton_lock();
    if singleton_lock_path.exists() {
        match SingletonLock::try_acquire(&singleton_lock_path) {
            Ok(guard) => {
                // We acquired the lock — nobody else held it. The file is
                // a stale leftover. Drop the guard immediately so we don't
                // hold the lock ourselves; the file stays on disk for
                // cleanup.
                drop(guard);
                files_to_clean.push(singleton_lock_path.clone());
                debug!(
                    "legacy_migration: daemon.singleton.lock at {} is unowned, marked for cleanup",
                    singleton_lock_path.display()
                );
            }
            Err(SingletonLockError::AlreadyHeld { .. }) => {
                // Live legacy daemon owns the singleton lock. Refuse.
                let hint = format!(
                    "legacy daemon holds {} (fcntl); investigate via `ps` and stop it before \
                     starting the new daemon",
                    singleton_lock_path.display()
                );
                // We don't have a PID from the lock probe alone. Try to
                // recover one from daemon.pid for the operator diagnostic;
                // fall back to 0 if unreadable.
                let pid = PidFile::read_pid(&paths.daemon_pid()).unwrap_or(0);
                return Ok(MigrationDecision::LegacyDaemonAlive { pid, hint });
            }
            Err(SingletonLockError::Io { source, .. }) => {
                // Unknown I/O failure probing the lock. Conservative: refuse.
                let hint = format!(
                    "could not probe legacy singleton lock at {}: {}; refusing to start to avoid \
                     index corruption — fix the I/O issue or remove the file by hand",
                    singleton_lock_path.display(),
                    source
                );
                let pid = PidFile::read_pid(&paths.daemon_pid()).unwrap_or(0);
                return Ok(MigrationDecision::LegacyDaemonAlive { pid, hint });
            }
        }
    }

    // 2. daemon.pid — PidFile::check_status is the authoritative probe.
    //
    // Note: check_status side-effect-deletes the file when its verdict is
    // Dead. We still mark it for cleanup if it's still on disk at the time
    // we inspect (the caller's unlink loop is a no-op on missing files).
    let pid_path = paths.daemon_pid();
    if pid_path.exists() {
        match PidFile::check_status(&pid_path) {
            PidFileStatus::Alive(pid) => {
                let hint = format!(
                    "legacy daemon running (PID {} via {}); stop it first via \
                     `julie-server stop` or `kill {}`",
                    pid,
                    pid_path.display(),
                    pid
                );
                return Ok(MigrationDecision::LegacyDaemonAlive { pid, hint });
            }
            PidFileStatus::Indeterminate => {
                // Conservative bias (per plan): treat Indeterminate as
                // legacy-alive. A racing legacy daemon may be mid-write of
                // its PID file; refusing protects against the worst case.
                let hint = format!(
                    "legacy daemon.pid at {} is in an indeterminate state (may be mid-write or \
                     a recycled PID owned by a privileged process); refusing to start — retry \
                     after a few seconds, or remove the file by hand if you're sure",
                    pid_path.display()
                );
                let pid = PidFile::read_pid(&pid_path).unwrap_or(0);
                return Ok(MigrationDecision::LegacyDaemonAlive { pid, hint });
            }
            PidFileStatus::Dead => {
                // check_status already unlinked the file, but be defensive
                // and add it to the cleanup list anyway (the caller's
                // best-effort unlink is a no-op if the file is gone).
                if pid_path.exists() {
                    files_to_clean.push(pid_path);
                }
            }
        }
    }

    // 3. daemon.state — informational only. The PID/lock probes above are
    // authoritative; the state file is a hint about lifecycle but doesn't
    // independently signal "alive". If it exists alongside dead PID/lock,
    // it's a stale leftover.
    let state_path = paths.daemon_state();
    if state_path.exists() {
        if let Ok(state) = fs::read_to_string(&state_path) {
            debug!(
                "legacy_migration: daemon.state at {} contains {:?} (informational; pid/lock probes are authoritative)",
                state_path.display(),
                state.trim()
            );
        }
        files_to_clean.push(state_path);
    }

    // 4. daemon.lock — adapter startup-serialization advisory lock.
    //
    // See the path-collision note in the module docs. We probe with
    // SingletonLock::try_acquire (same fcntl semantics). If something
    // holds it, the new daemon cannot start regardless — either legacy
    // owns it (refuse) or another instance of the new daemon owns it
    // (the kernel-held lock from A1.2 will refuse anyway downstream).
    // Either way, returning LegacyDaemonAlive here surfaces the issue
    // earlier with a clearer diagnostic than letting it fail deeper.
    let daemon_lock_path = paths.daemon_lock();
    if daemon_lock_path.exists() {
        match SingletonLock::try_acquire(&daemon_lock_path) {
            Ok(guard) => {
                drop(guard);
                files_to_clean.push(daemon_lock_path.clone());
                debug!(
                    "legacy_migration: daemon.lock at {} is unowned, marked for cleanup",
                    daemon_lock_path.display()
                );
            }
            Err(SingletonLockError::AlreadyHeld { .. }) => {
                let hint = format!(
                    "another process holds the daemon lock at {}; this is either a legacy daemon \
                     or another julie-daemon instance — investigate before starting",
                    daemon_lock_path.display()
                );
                let pid = PidFile::read_pid(&paths.daemon_pid()).unwrap_or(0);
                return Ok(MigrationDecision::LegacyDaemonAlive { pid, hint });
            }
            Err(SingletonLockError::Io { source, .. }) => {
                let hint = format!(
                    "could not probe daemon.lock at {}: {}; refusing to start to avoid races",
                    daemon_lock_path.display(),
                    source
                );
                let pid = PidFile::read_pid(&paths.daemon_pid()).unwrap_or(0);
                return Ok(MigrationDecision::LegacyDaemonAlive { pid, hint });
            }
        }
    }

    Ok(MigrationDecision::ProceedAndUnlink { files_to_clean })
}

/// Adapter-side legacy detection: if a live legacy daemon owns the
/// `daemon.pid` and `daemon.port` is readable, return a
/// `TransportEndpoint` pointing at it. The adapter can attach to this
/// endpoint instead of spawning a new daemon.
///
/// Returns `None` when:
/// - `daemon.port` is missing or unreadable
/// - `daemon.pid` is missing, dead, or indeterminate
/// - the port file contents fail to parse as a u16
///
/// This intentionally does NOT probe the HTTP endpoint for readiness; the
/// caller (adapter launcher) is responsible for that. We only assert
/// "there's something here worth attaching to".
pub fn detect_and_attach(paths: &DaemonPaths) -> Option<TransportEndpoint> {
    // PID must be alive.
    let pid_status = PidFile::check_status(&paths.daemon_pid());
    let _alive_pid = match pid_status {
        PidFileStatus::Alive(pid) => pid,
        PidFileStatus::Dead | PidFileStatus::Indeterminate => return None,
    };

    // Legacy julie-server publishes the MCP endpoint in daemon-mcp-transport.json.
    // daemon.port is the dashboard port, so using it here routes the adapter to
    // a non-MCP HTTP server during upgrade.
    TransportEndpoint::read_discovery(&paths.daemon_mcp_transport()).ok()
}
