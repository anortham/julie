use std::path::{Path, PathBuf};

/// Marker directories/files that identify a *version-control repository root*.
///
/// In their modern layouts these are outer boundaries: each VCS writes its
/// metadata once, at the checkout root, not in every sub-directory. That
/// non-nesting property is what makes them safe to treat as "stop the upward
/// workspace-root walk here" boundaries — unlike build manifests (`Cargo.toml`,
/// `package.json`, `go.mod`, `pyproject.toml`), which appear at BOTH a monorepo
/// root AND every member package and would falsely halt discovery inside a
/// Cargo-workspace member crate / npm-workspace package.
///
/// Caveat: Subversion BEFORE 1.7 (2011) wrote a `.svn` dir in every versioned
/// sub-directory. On such a (now-obsolete) checkout the discovery walk would stop
/// at the first sub-directory it visits; the only observable effect is that a
/// first-time index started from a sub-dir creates `.julie/` there instead of at
/// the working-copy root (once indexed at the root, the `.julie`-before-boundary
/// ordering finds the root on subsequent walks). Modern SVN (1.7+) keeps a single
/// root `.svn`. We accept this rare residual rather than drop SVN-root detection.
///
/// Authoritative source consumed by both workspace-root resolvers:
/// - `crate::tools::workspace::ManageWorkspaceTool::find_workspace_root`
///   (explicit-path resolver — returns the marker directory as the root)
/// - `crate::workspace::JulieWorkspace::find_workspace_root`
///   (upward `.julie` discovery walk — uses these as STOP boundaries)
///
/// Keep `crate::tools::shared::BLACKLISTED_DIRECTORIES` in parity with this list
/// (enforced by `test_vcs_root_markers_are_all_blacklisted_from_indexing`).
pub const VCS_ROOT_MARKERS: &[&str] = &[".git", ".hg", ".svn", ".jj", ".bzr", "_darcs"];

/// Centralized path resolution for Julie daemon infrastructure.
///
/// All daemon-related paths derive from `julie_home`. The default is `~/.julie/`,
/// which can be overridden by setting the `JULIE_HOME` environment variable to
/// an absolute path. An empty `JULIE_HOME` is rejected as a misconfiguration
/// (rather than silently falling back to `~/.julie/`).
#[derive(Clone)]
pub struct DaemonPaths {
    julie_home: PathBuf,
}

impl DaemonPaths {
    /// Create using the resolved Julie home directory.
    ///
    /// Resolution order:
    /// 1. If `JULIE_HOME` is set and non-empty, use it verbatim (no canonicalization,
    ///    no directory creation). The value MUST be an absolute path; relative paths
    ///    are rejected because they would yield cwd-dependent daemon identities
    ///    across the daemon, adapter, and CLI processes.
    /// 2. If `JULIE_HOME` is set but empty, return `Err(InvalidInput)` — this is a
    ///    misconfiguration, not a silent fallback.
    /// 3. If `JULIE_HOME` is set but relative, return `Err(InvalidInput)` — same
    ///    rationale; operators should see the error and fix their config rather
    ///    than have it silently absolutized.
    /// 4. Otherwise, fall back to `dirs::home_dir().join(".julie")`. Returns `Err`
    ///    if the OS home directory cannot be determined.
    pub fn try_new() -> Result<Self, std::io::Error> {
        if let Some(value) = std::env::var_os("JULIE_HOME") {
            if value.is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "JULIE_HOME is set but empty",
                ));
            }
            let candidate = PathBuf::from(value);
            if !candidate.is_absolute() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "JULIE_HOME must be an absolute path (got '{}')",
                        candidate.display()
                    ),
                ));
            }
            return Ok(Self {
                julie_home: candidate,
            });
        }

        let home = dirs::home_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine home directory",
            )
        })?;
        Ok(Self {
            julie_home: home.join(".julie"),
        })
    }

    /// Create with default home (~/.julie/). Panics if home directory cannot be determined.
    pub fn new() -> Self {
        Self::try_new().expect("Could not determine home directory")
    }

    /// Create with explicit home (for testing or JULIE_HOME override)
    pub fn with_home(julie_home: PathBuf) -> Self {
        Self { julie_home }
    }

    /// Root directory for all Julie daemon state
    pub fn julie_home(&self) -> PathBuf {
        self.julie_home.clone()
    }

    /// Check whether `candidate` resolves to the same directory as the
    /// configured Julie home.
    ///
    /// When both paths exist on disk, `canonicalize` is used for the
    /// comparison. This gives the correct answer on both case-insensitive
    /// (HFS+/APFS default, Windows NTFS) and case-sensitive (APFS optional,
    /// ext4) filesystems, because the OS itself decides whether two paths
    /// refer to the same inode.
    ///
    /// When either path does NOT exist (e.g. a candidate `.julie/` that has
    /// not been created yet), we fall back to raw `PathBuf` equality. This
    /// is deliberately strict: a previous implementation lowercased path
    /// strings on macOS, which is correct on case-insensitive HFS+/APFS but
    /// WRONG on case-sensitive APFS where `Home/` and `home/` are distinct
    /// directories. We prefer false negatives (missing a case-only variant
    /// that does not exist on disk) over false positives (treating two
    /// distinct case-sensitive directories as equal). Never panics. No
    /// environment is read.
    pub fn is_julie_home(&self, candidate: &Path) -> bool {
        match (candidate.canonicalize(), self.julie_home.canonicalize()) {
            // Both paths exist: canonicalize handles case-insensitive
            // filesystems correctly (returns on-disk case) and rejects
            // same-case paths to different dirs on case-sensitive
            // filesystems.
            (Ok(c), Ok(h)) => c == h,
            // Otherwise fall back to raw PathBuf comparison. We do NOT
            // lowercase since that would falsely match distinct paths on
            // case-sensitive filesystems; for non-existent paths we accept
            // the small chance of missing a case-only variant in exchange
            // for correctness.
            _ => candidate == self.julie_home.as_path(),
        }
    }

    /// Check whether `candidate` lives under (or is equal to) the configured
    /// Julie home directory.
    ///
    /// This is the canonical exclusion check for the discovery walker and
    /// file watcher: anything under `julie_home` is daemon state and must
    /// never be indexed, regardless of filename or extension. It defends
    /// against the operator footgun of pointing `JULIE_HOME` *inside* a
    /// workspace tree.
    ///
    /// Best-effort canonicalization is applied to both sides; when either
    /// canonicalization fails (e.g. the candidate doesn't exist yet), the
    /// raw `PathBuf` is used for the prefix check. `Path::starts_with` is
    /// component-wise, so `julie-home/` does NOT match `julie-homework/`.
    pub fn is_under_julie_home(&self, candidate: &Path) -> bool {
        let candidate_canon = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.to_path_buf());
        let home_canon = self
            .julie_home
            .canonicalize()
            .unwrap_or_else(|_| self.julie_home.clone());
        candidate_canon.starts_with(&home_canon)
    }

    /// Walker-safety check that answers "is this path a Julie home we
    /// should always skip?" regardless of env-var state.
    ///
    /// Returns true if `candidate` matches EITHER:
    /// 1. The currently configured Julie home (`try_new` success), OR
    /// 2. The conventional default `~/.julie` (when `dirs::home_dir()`
    ///    resolves).
    ///
    /// This is required because the walker must skip `~/.julie` even when
    /// `JULIE_HOME` is misconfigured (empty / no home directory). Without
    /// this defense, an invalid env makes `try_new()` return `None`, and
    /// the walker can capture `~/.julie` as a workspace and walk into the
    /// daemon state (or OneDrive-synced folders on Windows, triggering
    /// mass file downloads).
    pub fn is_any_known_julie_home(candidate: &Path) -> bool {
        // 1. Configured override (if any & valid).
        if let Ok(configured) = Self::try_new() {
            if configured.is_julie_home(candidate) {
                return true;
            }
        }

        // 2. Conventional default `~/.julie`, ALWAYS checked. This is the
        // belt-and-suspenders defense against a broken JULIE_HOME.
        if let Some(default_home) = dirs::home_dir() {
            let default = default_home.join(".julie");
            if Self::with_home(default).is_julie_home(candidate) {
                return true;
            }
        }

        false
    }

    /// Directory containing all workspace indexes
    pub fn indexes_dir(&self) -> PathBuf {
        self.julie_home.join("indexes")
    }

    /// Directory for a specific workspace's index (SQLite + Tantivy)
    pub fn workspace_index_dir(&self, workspace_id: &str) -> PathBuf {
        self.indexes_dir().join(workspace_id)
    }

    /// SQLite database path for a workspace
    pub fn workspace_db_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_index_dir(workspace_id)
            .join("db")
            .join("symbols.db")
    }

    /// Tantivy index directory for a workspace
    pub fn workspace_tantivy_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_index_dir(workspace_id).join("tantivy")
    }

    /// Named event for graceful daemon shutdown (Windows).
    ///
    /// `julie stop` signals this event; the daemon waits on it alongside
    /// ctrl_c. The event name includes a stable home-dir hash so different
    /// JULIE_HOME values do not share shutdown events.
    #[cfg(windows)]
    pub fn daemon_shutdown_event(&self) -> String {
        format!(
            "Local\\julie-daemon-shutdown-{:016x}",
            self.julie_home_hash()
        )
    }

    /// FNV-1a hash of `julie_home`, used for Windows shutdown event names.
    ///
    /// Stable across Rust versions (unlike `DefaultHasher`).
    ///
    /// Note: this uses `Path::to_string_lossy()`, which replaces non-UTF-8 byte
    /// sequences in the home directory path with U+FFFD. On modern systems this
    /// is essentially never observed -- Windows paths are UTF-16 by definition,
    /// and macOS/Linux home directories are conventionally UTF-8. On legacy
    /// filesystems or unusual locales, two distinct non-UTF-8 home paths could
    /// in theory hash to the same value. The collision is bounded in scope:
    /// the hash only namespaces the per-user `daemon_shutdown_event`, so a
    /// collision would mean two different homes share an event name on the
    /// same machine -- effectively impossible without an attacker who can
    /// already write to both home directories.
    #[cfg(windows)]
    fn julie_home_hash(&self) -> u64 {
        let path_str = self.julie_home.to_string_lossy();
        let mut hash: u64 = 14695981039346656037;
        for byte in path_str.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        hash
    }

    /// PID file for daemon lifecycle
    pub fn daemon_pid(&self) -> PathBuf {
        self.julie_home.join("daemon.pid")
    }

    /// Kernel-held singleton lock for the running daemon.
    pub fn daemon_lock(&self) -> PathBuf {
        self.julie_home.join("daemon.lock")
    }

    /// Short-lived adapter lock for serializing spawn attempts only.
    pub fn daemon_startup_lock(&self) -> PathBuf {
        self.julie_home.join("daemon-startup.lock")
    }

    /// Legacy singleton lock file held by pre-split daemon processes.
    pub fn daemon_singleton_lock(&self) -> PathBuf {
        self.julie_home.join("daemon.singleton.lock")
    }

    /// Daemon lifecycle log
    pub fn daemon_log(&self) -> PathBuf {
        self.julie_home.join("daemon.log")
    }

    /// Per-project log directory (written by daemon, scoped to project)
    pub fn project_log_dir(&self, project_root: &Path) -> PathBuf {
        project_root.join(".julie").join("logs")
    }

    /// Persistent daemon state database (workspaces, codehealth snapshots, tool call history)
    pub fn daemon_db(&self) -> PathBuf {
        self.julie_home.join("daemon.db")
    }

    /// Path to the file storing the dashboard HTTP port.
    pub fn daemon_port(&self) -> PathBuf {
        self.julie_home.join("daemon.port")
    }

    /// Structured discovery file for the daemon MCP Streamable HTTP endpoint.
    pub fn daemon_mcp_transport(&self) -> PathBuf {
        self.julie_home.join("daemon-mcp-transport.json")
    }

    /// Per-daemon bearer token read by local MCP transport clients.
    pub fn daemon_mcp_token(&self) -> PathBuf {
        self.julie_home.join("daemon-mcp.token")
    }

    /// Discovery file — adapter reads this to learn the daemon's HTTP
    /// endpoint, bearer token path, and identity (pid + creation time).
    pub fn discovery_file(&self) -> PathBuf {
        self.julie_home.join("discovery.json")
    }

    /// Daemon bearer token file (mode 0600 on POSIX).  The adapter reads this
    /// to authenticate HTTP requests to the daemon's localhost endpoint.
    pub fn token_file(&self) -> PathBuf {
        self.julie_home.join("daemon.token")
    }

    /// Daemon lifecycle state file (starting/ready/stopping)
    pub fn daemon_state(&self) -> PathBuf {
        self.julie_home.join("daemon.state")
    }

    /// Migration state file
    pub fn migration_state(&self) -> PathBuf {
        self.julie_home.join("migration.json")
    }

    /// Ensure julie_home and indexes directories exist
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.indexes_dir())
    }
}

impl Default for DaemonPaths {
    fn default() -> Self {
        Self::new()
    }
}
