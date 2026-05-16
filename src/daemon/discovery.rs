//! Daemon discovery primitives for the daemon-split architecture.
//!
//! This module hosts the kernel-held advisory lock that enforces
//! "only one daemon process per JULIE_HOME". The discovery file
//! (`daemon-mcp-transport.json` identity validation) is added in A1.3.
//!
//! ## DaemonLockGuard
//!
//! `DaemonLockGuard` owns a long-lived file descriptor on `daemon.lock`
//! with an OS-native advisory lock acquired non-blockingly:
//!
//!   - **POSIX (macOS / Linux)**: `flock(LOCK_EX | LOCK_NB)` via
//!     `fs2::FileExt::try_lock_exclusive`. Advisory — only cooperating
//!     processes see it. The lock is tied to the open-file-description,
//!     so the kernel releases it on process exit (clean or crash).
//!   - **Windows**: `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK |
//!     LOCKFILE_FAIL_IMMEDIATELY)` via the same `fs2` wrapper.
//!     Mandatory at the handle level; released by the kernel on
//!     process exit.
//!
//! `fs2` was chosen over direct `nix`/`rustix`/`windows-sys` calls
//! because Julie already depends on it and `src/daemon/singleton.rs`
//! is the proven precedent for the same pattern in this codebase.
//! The plan-specified invariants — drop releases, kernel releases on
//! crash, second concurrent acquire fails — all hold with the `fs2`
//! wrapper exactly as they would with the direct syscalls.
//!
//! ## Lock file persistence
//!
//! The lock file at `~/.julie/daemon.lock` persists across daemon
//! lifetimes; only the lock state itself is per-process. We never
//! truncate, unlink, or recreate the file. Unlinking would break the
//! stable-inode invariant that makes `flock` contention reliable
//! across processes.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use fs2::FileExt;

/// Holds an exclusive OS-native advisory lock on the daemon lock file.
///
/// On `Drop`, the lock is released by closing the file (the kernel
/// releases the descriptor-bound lock automatically). We also call
/// `unlock` explicitly so any error surfaces in the trace layer.
///
/// The lock file is NOT unlinked on drop — see module docs.
#[derive(Debug)]
pub struct DaemonLockGuard {
    file: File,
    path: PathBuf,
}

/// Error returned when another holder already owns the daemon lock for
/// the given path. Distinct from any other I/O error so callers can
/// take the "another daemon is running" branch unambiguously.
#[derive(Debug)]
pub struct LockAlreadyHeld {
    pub path: PathBuf,
}

impl DaemonLockGuard {
    /// Try to acquire an exclusive OS-native advisory lock on
    /// `path`, creating the file if it doesn't exist. Never truncates
    /// and never unlinks.
    ///
    /// Returns `Err(LockAlreadyHeld)` immediately if another holder has
    /// the lock — does not block. Other filesystem or kernel failures
    /// (e.g. EACCES, permission denied, parent dir missing) are
    /// reported via `io::Error`.
    pub fn try_acquire(path: &Path) -> Result<Self, AcquireError> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|source| AcquireError::Io {
                path: path.to_path_buf(),
                source,
            })?;

        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Self {
                file,
                path: path.to_path_buf(),
            }),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                Err(AcquireError::AlreadyHeld(LockAlreadyHeld {
                    path: path.to_path_buf(),
                }))
            }
            Err(source) => Err(AcquireError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Path of the lock file this guard holds.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for DaemonLockGuard {
    fn drop(&mut self) {
        // Closing the file (when `self.file` drops) releases the
        // descriptor-bound lock at the kernel level. Calling `unlock`
        // explicitly makes the intent unambiguous and surfaces any
        // unlock error through the trace layer.
        if let Err(e) = FileExt::unlock(&self.file) {
            tracing::warn!(
                path = %self.path.display(),
                error = %e,
                "Failed to release daemon lock cleanly; file close will still release",
            );
        }
    }
}

/// Full failure surface for `DaemonLockGuard::try_acquire`.
///
/// Callers that only care about the "another daemon is running"
/// outcome can match on `AcquireError::AlreadyHeld(_)`. Tests use
/// `From<AcquireError>` to peel the `LockAlreadyHeld` variant directly
/// — see the conversion impl below.
#[derive(Debug)]
pub enum AcquireError {
    /// Another holder owns the lock for `path`. Carries the typed
    /// error so callers can propagate it without re-wrapping.
    AlreadyHeld(LockAlreadyHeld),
    /// Filesystem or kernel error while opening or locking the file.
    Io { path: PathBuf, source: io::Error },
}

impl From<LockAlreadyHeld> for AcquireError {
    fn from(value: LockAlreadyHeld) -> Self {
        AcquireError::AlreadyHeld(value)
    }
}

impl std::fmt::Display for LockAlreadyHeld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "another daemon already holds the lock at {}",
            self.path.display()
        )
    }
}

impl std::error::Error for LockAlreadyHeld {}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcquireError::AlreadyHeld(held) => write!(f, "{}", held),
            AcquireError::Io { path, source } => write!(
                f,
                "failed to acquire daemon lock at {}: {}",
                path.display(),
                source
            ),
        }
    }
}

impl std::error::Error for AcquireError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AcquireError::Io { source, .. } => Some(source),
            AcquireError::AlreadyHeld(_) => None,
        }
    }
}

// =============================================================================
// A1.3: Discovery file — DiscoveryRecord, DiscoveryFile, DiscoveryState
// =============================================================================
//
// The discovery file lives at `~/.julie/discovery.json`. It lets the adapter
// locate the running daemon's HTTP endpoint and verify its identity before
// connecting. The pid + pid_creation_time_micros pair defends against PID
// reuse: a recycled PID with a different creation time is classified as Stale.
//
// ## Atomic write recipe (POSIX + Windows)
//
// 1. Serialize the record to a `.tmp` file alongside the final path.
// 2. `sync_all()` the temp file (fsync data + metadata).
// 3. `fs::rename(tmp, final)` — atomic on POSIX; atomic on NTFS.
// 4. POSIX only: open the parent directory and `sync_all()` on its fd so
//    the rename is durable in the directory entry. Skipped on Windows
//    because NTFS rename is already crash-durable without a directory fsync.
//
// ## Unit semantics: micros (judgment call)
//
// The existing `process_creation_time_micros` helper in pid.rs returns
// microseconds. Renaming the DiscoveryRecord field to `pid_creation_time_micros`
// (instead of the design's `_ns`) keeps units consistent with the existing
// helper without any lossy conversion. The plan explicitly noted this as an
// acceptable lower-risk option.
//
// ## Protocol and schema versioning
//
// `protocol_version`: "1" — bumped when the wire format or semantics change
//   in a way that would break an older adapter reading the file.
// `schema_version`: 1 (u32) — JSON schema version; bumped on field additions/
//   removals that require migration logic.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::daemon::pid::{process_creation_time_micros, PidFile};

/// The current protocol version string written into every discovery record.
/// Bump when the semantics or wire format change incompatibly.
const PROTOCOL_VERSION: &str = "1";

/// The current JSON schema version. Bump on field additions / removals that
/// require migration logic in `read_and_validate`.
const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// DiscoveryRecord
// ---------------------------------------------------------------------------

/// All the information a connecting adapter needs to find and verify the
/// running daemon.
///
/// Serialized as pretty-printed JSON at `~/.julie/discovery.json`. Written
/// atomically via temp-file + rename so readers never see a partial file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryRecord {
    /// OS PID of the running daemon.
    pub pid: u32,

    /// Creation time of the daemon process in microseconds (platform-specific
    /// epoch; used only for equality comparison). Zero if the platform cannot
    /// provide a creation time. Matches the semantics of
    /// `pid::process_creation_time_micros`.
    pub pid_creation_time_micros: u64,

    /// Hostname or IP address the daemon's HTTP server is bound to.
    /// Typically `"127.0.0.1"`.
    pub host: String,

    /// TCP port of the daemon's HTTP server.
    pub port: u16,

    /// Absolute path to the bearer token file (written by A1.4).
    /// The token itself is NOT stored in this file — only the path.
    pub token_path: PathBuf,

    /// Absolute path to the daemon log file for this run.
    pub log_path: PathBuf,

    /// Semver version string of the running daemon binary
    /// (`env!("CARGO_PKG_VERSION")`).
    pub daemon_version: String,

    /// Protocol version string. Adapter must refuse records with an
    /// unrecognised protocol version.
    pub protocol_version: String,

    /// JSON schema version (monotonically increasing integer). Lets future
    /// code distinguish records written by older daemons.
    pub schema_version: u32,

    /// UNIX epoch time in microseconds when the daemon wrote this record.
    pub started_at: u64,

    /// Lifecycle phase string ("running", "stopping"). Optional for forward
    /// compatibility — records written by daemons predating A1.7 omit this
    /// field; readers must treat `None` as "running".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

impl DiscoveryRecord {
    /// Build a record for the current process.
    ///
    /// `port` is the TCP port the HTTP server is bound to.
    /// `token_path` is where the bearer token will be written (A1.4).
    /// `log_path` is the daemon log path for this run.
    pub fn for_current_process(port: u16, token_path: PathBuf, log_path: PathBuf) -> Self {
        let pid = std::process::id();
        let pid_creation_time_micros =
            process_creation_time_micros(pid).unwrap_or(0);

        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let host = hostname();

        Self {
            pid,
            pid_creation_time_micros,
            host,
            port,
            token_path,
            log_path,
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol_version: PROTOCOL_VERSION.to_owned(),
            schema_version: SCHEMA_VERSION,
            started_at,
            phase: Some("running".to_string()),
        }
    }
}

/// Returns the local hostname, falling back to `"127.0.0.1"` on error.
fn hostname() -> String {
    // Use the system hostname crate-free: read /etc/hostname on Linux,
    // gethostname on POSIX, GetComputerNameExW on Windows. A simple
    // cross-platform approach that avoids an extra dep: delegate to the
    // std environment if available, else fall back.
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        let ret = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if ret == 0 {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            if let Ok(s) = std::str::from_utf8(&buf[..end]) {
                return s.to_owned();
            }
        }
        "127.0.0.1".to_owned()
    }

    #[cfg(windows)]
    {
        // On Windows, fall back to 127.0.0.1 — adapters connect locally.
        "127.0.0.1".to_owned()
    }

    #[cfg(not(any(unix, windows)))]
    {
        "127.0.0.1".to_owned()
    }
}

// ---------------------------------------------------------------------------
// DiscoveryState
// ---------------------------------------------------------------------------

/// The result of `DiscoveryFile::read_and_validate`.
#[derive(Debug)]
pub enum DiscoveryState {
    /// The record is present and the recorded pid is alive with a matching
    /// creation time. Contains the validated record.
    Live(DiscoveryRecord),

    /// The file exists and is well-formed JSON, but the pid it records is
    /// either dead or has a creation-time mismatch (PID reuse). The adapter
    /// must spawn a fresh daemon.
    Stale,

    /// No discovery file exists at the path. The daemon is not running.
    Missing,

    /// The file exists but could not be parsed. Contains the error message.
    /// The adapter should treat this like Stale and attempt to (re)start.
    Corrupt(String),
}

// ---------------------------------------------------------------------------
// DiscoveryFile
// ---------------------------------------------------------------------------

/// Static-method namespace for reading and writing `discovery.json`.
pub struct DiscoveryFile;

impl DiscoveryFile {
    /// Atomically write `record` to `path`.
    ///
    /// Recipe:
    /// 1. Serialize to `<path>.tmp`.
    /// 2. `sync_all()` the temp file.
    /// 3. `rename(tmp, path)`.
    /// 4. POSIX: `sync_all()` the parent directory fd.
    ///
    /// On failure the `.tmp` file may be left behind; callers can ignore it
    /// — `read_and_validate` only reads the canonical `path`.
    pub fn write_atomic(path: &Path, record: &DiscoveryRecord) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        let parent = path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "discovery path has no parent directory",
            )
        })?;

        // 1. Serialize into a temp file.
        let json =
            serde_json::to_string_pretty(record).map_err(|e| std::io::Error::other(e))?;
        let mut tmp_file = File::create(&tmp)?;
        use std::io::Write as _;
        tmp_file.write_all(json.as_bytes())?;

        // 2. fsync the data.
        tmp_file.sync_all()?;
        drop(tmp_file);

        // 3. Atomic rename.
        std::fs::rename(&tmp, path)?;

        // 4. POSIX: fsync the parent directory so the rename is durable.
        #[cfg(unix)]
        {
            let dir_fd = File::open(parent)?;
            dir_fd.sync_all()?;
        }

        // Suppress "unused variable" warning on Windows.
        let _ = parent;

        Ok(())
    }

    /// Read `path` and validate the recorded pid + creation time.
    ///
    /// Returns:
    /// - `Missing`  — file absent.
    /// - `Corrupt`  — file present but not valid JSON / wrong schema.
    /// - `Stale`    — file parseable but pid is dead or creation time mismatches.
    /// - `Live`     — pid alive and creation time matches.
    pub fn read_and_validate(path: &Path) -> DiscoveryState {
        let contents = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return DiscoveryState::Missing;
            }
            Err(e) => {
                return DiscoveryState::Corrupt(format!("read error: {e}"));
            }
        };

        let record: DiscoveryRecord = match serde_json::from_str(&contents) {
            Ok(r) => r,
            Err(e) => {
                return DiscoveryState::Corrupt(format!("JSON parse error: {e}"));
            }
        };

        // PID liveness check.
        if !PidFile::is_process_alive(record.pid) {
            return DiscoveryState::Stale;
        }

        // PID-reuse defense: if creation_time is recorded, it must match.
        if record.pid_creation_time_micros != 0 {
            match process_creation_time_micros(record.pid) {
                Some(actual) if actual == record.pid_creation_time_micros => {
                    // Match — fall through to Live.
                }
                Some(_) => {
                    // Mismatch — PID was recycled.
                    return DiscoveryState::Stale;
                }
                None => {
                    // Cannot determine creation time (platform limitation or
                    // permission denied). Treat as Stale so the adapter does
                    // not silently connect to the wrong process.
                    return DiscoveryState::Stale;
                }
            }
        }

        DiscoveryState::Live(record)
    }
}
