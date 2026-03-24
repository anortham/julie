use std::path::{Path, PathBuf};

/// Centralized path resolution for Julie daemon infrastructure.
/// All daemon-related paths derive from `julie_home` (~/.julie/ by default).
#[derive(Clone)]
pub struct DaemonPaths {
    julie_home: PathBuf,
}

impl DaemonPaths {
    /// Create with default home (~/.julie/)
    pub fn new() -> Self {
        let home = dirs::home_dir().expect("Could not determine home directory");
        Self {
            julie_home: home.join(".julie"),
        }
    }

    /// Create with explicit home (for testing or JULIE_HOME override)
    pub fn with_home(julie_home: PathBuf) -> Self {
        Self { julie_home }
    }

    /// Root directory for all Julie daemon state
    pub fn julie_home(&self) -> PathBuf {
        self.julie_home.clone()
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

    /// Unix domain socket path (macOS/Linux)
    #[cfg(unix)]
    pub fn daemon_socket(&self) -> PathBuf {
        self.julie_home.join("daemon.sock")
    }

    /// Named pipe name (Windows)
    #[cfg(windows)]
    pub fn daemon_pipe_name(&self) -> String {
        self.daemon_ipc_addr().to_string_lossy().into_owned()
    }

    /// Platform-specific IPC address for the daemon.
    /// Returns socket path on Unix, named pipe path on Windows.
    ///
    /// On Windows, the pipe name incorporates a hash of `julie_home` so that
    /// different installations (or test instances with temp dirs) get isolated
    /// pipe endpoints, matching the Unix behavior where each `julie_home` gets
    /// its own socket file.
    pub fn daemon_ipc_addr(&self) -> PathBuf {
        #[cfg(unix)]
        {
            self.julie_home.join("daemon.sock")
        }
        #[cfg(windows)]
        {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::hash::DefaultHasher::new();
            self.julie_home.hash(&mut hasher);
            let hash = hasher.finish();
            PathBuf::from(format!(r"\\.\pipe\julie-daemon-{:016x}", hash))
        }
    }

    /// PID file for daemon lifecycle
    pub fn daemon_pid(&self) -> PathBuf {
        self.julie_home.join("daemon.pid")
    }

    /// Advisory lock file for adapter startup serialization
    pub fn daemon_lock(&self) -> PathBuf {
        self.julie_home.join("daemon.lock")
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
