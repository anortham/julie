# v6 Phase 1: Daemon Core + Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Julie MCP tools are MANDATORY for all code investigation.** Use `fast_search`, `deep_dive`, `get_symbols`, `fast_refs` before modifying any symbol. This is the dogfooding contract.
>
> **Test rules for subagents:** Run ONLY your specific test: `cargo test --lib <test_name> 2>&1 | tail -10`. Do NOT run `cargo xtask test dev` or any broad test suite. The orchestrator handles regression checks.

**Goal:** Enable multiple MCP clients to use Julie on the same project without Tantivy lock conflicts, redundant resources, or startup failures.

**Architecture:** Persistent daemon process owns all expensive resources (indexes, embedding model, file watchers). Thin stdio adapters auto-start the daemon and forward JSON-RPC bytes over IPC. Daemon also exposes Streamable HTTP for direct connections. Centralized index storage at `~/.julie/indexes/` eliminates per-project duplication.

**Tech Stack:** Rust, rmcp 1.x (Streamable HTTP), axum (HTTP server), tokio (async runtime), fs2 (file locking), Unix domain sockets / Windows named pipes (IPC)

**Spec:** `docs/superpowers/specs/2026-03-22-v6-daemon-adapter-architecture-design.md`

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `src/daemon/mod.rs` | Daemon entry point: bind IPC + HTTP, accept loop, graceful shutdown |
| `src/daemon/workspace_pool.rs` | `WorkspacePool`: shared workspace instances across sessions |
| `src/daemon/pid.rs` | PID file lifecycle: create, validate, stale cleanup |
| `src/daemon/ipc.rs` | Cross-platform IPC transport abstraction (Unix socket / named pipe) |
| `src/daemon/session.rs` | Session tracking: active connections, idle detection |
| `src/adapter/mod.rs` | Adapter mode: auto-start daemon, byte-forward stdio to IPC |
| `src/adapter/launcher.rs` | Daemon launcher: advisory lock, spawn detached, wait for socket |
| `src/migration.rs` | Safe index migration: copy, validate, delete originals |
| `src/paths.rs` | Centralized path resolution: `~/.julie/` layout, daemon socket, PID file |
| `src/tests/daemon/mod.rs` | Daemon test module |
| `src/tests/daemon/workspace_pool.rs` | WorkspacePool tests |
| `src/tests/daemon/pid.rs` | PID file lifecycle tests |
| `src/tests/daemon/ipc.rs` | IPC transport tests |
| `src/tests/daemon/session.rs` | Session tracking tests |
| `src/tests/adapter/mod.rs` | Adapter tests |
| `src/tests/adapter/launcher.rs` | Launcher tests |
| `src/tests/migration.rs` | Migration tests |
| `src/tests/integration/daemon_lifecycle.rs` | End-to-end daemon integration tests |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` | Bump rmcp to 1.x, add axum, fs2, interprocess dependencies |
| `src/lib.rs` | Add `pub mod daemon`, `pub mod adapter`, `pub mod migration`, `pub mod paths` |
| `src/cli.rs` | Add clap subcommands: `daemon`, `stop`, `status`, default = adapter |
| `src/main.rs` | Route subcommands to daemon/adapter/stop/status entry points |
| `src/handler.rs` | Add `new_with_shared_workspace()`, guard `on_initialized` auto-indexing |
| `src/workspace/mod.rs` | Use centralized paths from `src/paths.rs`, expose shared init |
| `src/tests/mod.rs` | Register new test modules |

---

## Task 1: rmcp Version Bump (0.12 -> 1.x)

**Files:**
- Modify: `Cargo.toml` (line 37)
- Modify: `src/main.rs` (lines 48-51, rmcp imports)
- Modify: `src/handler.rs` (rmcp imports, ServerHandler trait)
- Test: Existing test suite (`cargo xtask test dev`)

This task is a prerequisite for everything else. rmcp 1.x provides `transport-streamable-http-server`.

- [ ] **Step 1: Research rmcp 1.x API changes**

Check the rmcp changelog and docs for breaking changes from 0.12 to 1.x:
```bash
cargo search rmcp --limit 1
```
Read: https://docs.rs/rmcp/latest/rmcp/
Focus on: `ServerHandler` trait, `ServiceExt`, `transport::stdio`, feature flag names.

- [ ] **Step 2: Update Cargo.toml**

```toml
# Change from:
rmcp = { version = "0.12", features = ["server", "transport-io", "macros"] }
# To (verify exact feature names):
rmcp = { version = "1", features = ["server", "transport-io", "transport-streamable-http-server", "macros"] }
```

Also add new dependencies:
```toml
axum = "0.8"
fs2 = "0.4"
```

- [ ] **Step 3: Fix compilation errors**

Expect changes in:
- Import paths (`rmcp::transport::stdio` may have moved)
- `ServerHandler` trait methods (signatures may differ)
- `ServerCapabilities` builder API
- `ProtocolVersion` enum variants

Fix each error, keeping the existing stdio behavior working.

- [ ] **Step 4: Verify existing tests pass**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All existing tests green. If failures, fix them before proceeding.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/
git commit -m "chore: bump rmcp to 1.x, add axum and fs2 dependencies"
```

---

## Task 2: Centralized Paths Module

**Files:**
- Create: `src/paths.rs`
- Test: `src/tests/daemon/mod.rs` (create), `src/tests/daemon/paths.rs` (create)
- Modify: `src/lib.rs` (add `pub mod paths`)
- Modify: `src/tests/mod.rs` (register test module)

All daemon infrastructure needs to agree on where things live. Define this once.

- [ ] **Step 1: Write failing tests for path resolution**

Create `src/tests/daemon/mod.rs`:
```rust
pub mod paths;
```

Create `src/tests/daemon/paths.rs`:
```rust
use crate::paths::DaemonPaths;
use std::path::PathBuf;

#[test]
fn test_julie_home_uses_home_dir() {
    let paths = DaemonPaths::new();
    let home = dirs::home_dir().unwrap();
    assert_eq!(paths.julie_home(), home.join(".julie"));
}

#[test]
fn test_indexes_dir() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("indexes");
    assert_eq!(paths.indexes_dir(), expected);
}

#[test]
fn test_workspace_index_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir()
        .unwrap()
        .join(".julie")
        .join("indexes")
        .join("myproject_abc12345");
    assert_eq!(
        paths.workspace_index_dir("myproject_abc12345"),
        expected
    );
}

#[test]
fn test_daemon_socket_path() {
    let paths = DaemonPaths::new();
    let julie_home = dirs::home_dir().unwrap().join(".julie");
    #[cfg(unix)]
    assert_eq!(paths.daemon_socket(), julie_home.join("daemon.sock"));
    #[cfg(windows)]
    assert_eq!(
        paths.daemon_pipe_name(),
        r"\\.\pipe\julie-daemon"
    );
}

#[test]
fn test_daemon_pid_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("daemon.pid");
    assert_eq!(paths.daemon_pid(), expected);
}

#[test]
fn test_daemon_lock_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("daemon.lock");
    assert_eq!(paths.daemon_lock(), expected);
}

#[test]
fn test_project_log_dir() {
    let project = PathBuf::from("/Users/murphy/source/julie");
    let paths = DaemonPaths::new();
    assert_eq!(
        paths.project_log_dir(&project),
        project.join(".julie").join("logs")
    );
}

#[test]
fn test_custom_julie_home_via_env() {
    // DaemonPaths should respect JULIE_HOME env var if set
    let paths = DaemonPaths::with_home(PathBuf::from("/tmp/test-julie"));
    assert_eq!(paths.julie_home(), PathBuf::from("/tmp/test-julie"));
    assert_eq!(
        paths.indexes_dir(),
        PathBuf::from("/tmp/test-julie/indexes")
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_julie_home_uses_home_dir 2>&1 | tail -10`
Expected: FAIL (module `paths` not found)

- [ ] **Step 3: Implement DaemonPaths**

Create `src/paths.rs`:
```rust
use std::path::{Path, PathBuf};

/// Centralized path resolution for Julie daemon infrastructure.
/// All daemon-related paths derive from `julie_home` (~/.julie/ by default).
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
        r"\\.\pipe\julie-daemon".to_string()
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
```

Add to `src/lib.rs`:
```rust
pub mod paths;
```

Register test module in `src/tests/mod.rs`:
```rust
pub mod daemon;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_julie_home 2>&1 | tail -10`
Run: `cargo test --lib test_daemon_socket 2>&1 | tail -10`
Run: `cargo test --lib test_project_log 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/paths.rs src/tests/daemon/ src/lib.rs src/tests/mod.rs
git commit -m "feat(v6): add DaemonPaths for centralized path resolution"
```

---

## Task 3: PID File Lifecycle

**Files:**
- Create: `src/daemon/mod.rs`
- Create: `src/daemon/pid.rs`
- Test: `src/tests/daemon/pid.rs`
- Modify: `src/lib.rs` (add `pub mod daemon`)

PID file management is needed by both the daemon (to write) and the adapter (to read/validate). Build it as an independent module.

- [ ] **Step 1: Write failing tests**

Create `src/tests/daemon/pid.rs`:
```rust
use crate::daemon::pid::PidFile;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_create_pid_file_writes_current_pid() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("daemon.pid");

    let pid_file = PidFile::create(&pid_path).unwrap();
    assert!(pid_path.exists());

    let contents = std::fs::read_to_string(&pid_path).unwrap();
    let stored_pid: u32 = contents.trim().parse().unwrap();
    assert_eq!(stored_pid, std::process::id());

    drop(pid_file); // cleanup
}

#[test]
fn test_read_pid_from_existing_file() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("daemon.pid");
    std::fs::write(&pid_path, "12345\n").unwrap();

    let pid = PidFile::read_pid(&pid_path).unwrap();
    assert_eq!(pid, 12345);
}

#[test]
fn test_read_pid_returns_none_for_missing_file() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("daemon.pid");

    let pid = PidFile::read_pid(&pid_path);
    assert!(pid.is_none());
}

#[test]
fn test_read_pid_returns_none_for_corrupt_file() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("daemon.pid");
    std::fs::write(&pid_path, "not-a-number\n").unwrap();

    let pid = PidFile::read_pid(&pid_path);
    assert!(pid.is_none());
}

#[test]
fn test_is_process_alive_for_current_process() {
    assert!(PidFile::is_process_alive(std::process::id()));
}

#[test]
fn test_is_process_alive_for_nonexistent_pid() {
    // PID 99999999 is extremely unlikely to exist
    assert!(!PidFile::is_process_alive(99_999_999));
}

#[test]
fn test_create_pid_file_is_atomic() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("daemon.pid");

    // Write via atomic (tmp + rename)
    let _pid_file = PidFile::create(&pid_path).unwrap();

    // No .tmp file should remain
    assert!(!dir.path().join("daemon.pid.tmp").exists());
    assert!(pid_path.exists());
}

#[test]
fn test_cleanup_removes_pid_file() {
    let dir = tempdir().unwrap();
    let pid_path = dir.path().join("daemon.pid");

    let pid_file = PidFile::create(&pid_path).unwrap();
    assert!(pid_path.exists());

    pid_file.cleanup();
    assert!(!pid_path.exists());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_create_pid_file_writes 2>&1 | tail -10`
Expected: FAIL (module `daemon::pid` not found)

- [ ] **Step 3: Implement PidFile**

Create `src/daemon/mod.rs`:
```rust
pub mod pid;
```

Create `src/daemon/pid.rs`:
```rust
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Manages the daemon PID file for lifecycle tracking.
/// Created by the daemon on startup; read by adapters to check if daemon is alive.
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Create a new PID file atomically (write to .tmp, rename to final path).
    /// Returns the PidFile handle for cleanup on shutdown.
    pub fn create(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = path.with_extension("pid.tmp");
        let mut file = fs::File::create(&tmp_path)?;
        write!(file, "{}\n", std::process::id())?;
        file.sync_all()?;
        drop(file);

        fs::rename(&tmp_path, path)?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Read the PID from an existing PID file. Returns None if file doesn't exist
    /// or contains invalid data.
    pub fn read_pid(path: &Path) -> Option<u32> {
        fs::read_to_string(path)
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// Check if a process with the given PID is alive.
    #[cfg(unix)]
    pub fn is_process_alive(pid: u32) -> bool {
        // kill(pid, 0) checks existence without sending a signal
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(windows)]
    pub fn is_process_alive(pid: u32) -> bool {
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        use windows_sys::Win32::Foundation::CloseHandle;
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                false
            } else {
                CloseHandle(handle);
                true
            }
        }
    }

    /// Check if a daemon is running based on the PID file.
    /// Returns Some(pid) if alive, None if not running or stale.
    pub fn check_running(path: &Path) -> Option<u32> {
        let pid = Self::read_pid(path)?;
        if Self::is_process_alive(pid) {
            Some(pid)
        } else {
            // Stale PID file; clean it up
            let _ = fs::remove_file(path);
            None
        }
    }

    /// Remove the PID file (called on graceful shutdown).
    pub fn cleanup(self) {
        let _ = fs::remove_file(&self.path);
    }
}
```

Add to `src/lib.rs`:
```rust
pub mod daemon;
```

Add to `src/tests/daemon/mod.rs`:
```rust
pub mod pid;
```

**Note on `libc`:** Check if `libc` is already a dependency. If not, add it as `[target.'cfg(unix)'.dependencies]`. On Windows, add `windows-sys` with features `["Win32_System_Threading", "Win32_Foundation"]`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_create_pid_file 2>&1 | tail -10`
Run: `cargo test --lib test_is_process_alive 2>&1 | tail -10`
Run: `cargo test --lib test_cleanup_removes 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/daemon/ src/tests/daemon/ src/lib.rs Cargo.toml
git commit -m "feat(v6): add PID file lifecycle management"
```

---

## Task 4: WorkspacePool

**Files:**
- Create: `src/daemon/workspace_pool.rs`
- Test: `src/tests/daemon/workspace_pool.rs`
- Modify: `src/daemon/mod.rs` (add module)
- Modify: `src/tests/daemon/mod.rs` (add test module)

The WorkspacePool maps workspace IDs to shared `JulieWorkspace` instances. This is the core data structure that enables multi-session workspace sharing.

- [ ] **Step 1: Write failing tests**

Create `src/tests/daemon/workspace_pool.rs`:
```rust
use crate::daemon::workspace_pool::WorkspacePool;
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_get_or_init_creates_workspace_on_first_call() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();

    let ws = pool.get_or_init("test_abc12345", &workspace_root).await.unwrap();
    assert!(ws.db.is_some());
    assert!(ws.search_index.is_some());
}

#[tokio::test]
async fn test_get_or_init_returns_same_instance_on_second_call() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();

    let ws1 = pool.get_or_init("test_abc12345", &workspace_root).await.unwrap();
    let ws2 = pool.get_or_init("test_abc12345", &workspace_root).await.unwrap();

    // Both should point to the same Arc
    assert!(std::sync::Arc::ptr_eq(&ws1.db.as_ref().unwrap(), &ws2.db.as_ref().unwrap()));
}

#[tokio::test]
async fn test_get_returns_none_for_unknown_workspace() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    let ws = pool.get("nonexistent_abc12345");
    assert!(ws.is_none());
}

#[tokio::test]
async fn test_get_returns_some_after_init() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();

    pool.get_or_init("test_abc12345", &workspace_root).await.unwrap();
    let ws = pool.get("test_abc12345");
    assert!(ws.is_some());
}

#[tokio::test]
async fn test_is_indexed_returns_false_before_indexing() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    assert!(!pool.is_indexed("test_abc12345"));
}

#[tokio::test]
async fn test_mark_indexed() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();

    pool.get_or_init("test_abc12345", &workspace_root).await.unwrap();
    pool.mark_indexed("test_abc12345");
    assert!(pool.is_indexed("test_abc12345"));
}

#[tokio::test]
async fn test_active_workspace_count() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    assert_eq!(pool.active_count(), 0);

    let root1 = temp.path().join("project1");
    let root2 = temp.path().join("project2");
    std::fs::create_dir_all(&root1).unwrap();
    std::fs::create_dir_all(&root2).unwrap();

    pool.get_or_init("proj1_abc", &root1).await.unwrap();
    assert_eq!(pool.active_count(), 1);

    pool.get_or_init("proj2_def", &root2).await.unwrap();
    assert_eq!(pool.active_count(), 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_get_or_init_creates 2>&1 | tail -10`
Expected: FAIL (module not found)

- [ ] **Step 3: Implement WorkspacePool**

Create `src/daemon/workspace_pool.rs`:
```rust
use crate::workspace::JulieWorkspace;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::info;

/// Tracks per-workspace indexing state.
struct WorkspaceEntry {
    workspace: Arc<JulieWorkspace>,
    indexed: bool,
}

/// Pool of shared workspace instances. The daemon creates one pool;
/// all sessions get_or_init from it. First caller initializes the
/// workspace (DB + Tantivy); subsequent callers get the same Arc.
pub struct WorkspacePool {
    workspaces: RwLock<HashMap<String, WorkspaceEntry>>,
    indexes_dir: PathBuf,
}

impl WorkspacePool {
    pub fn new(indexes_dir: PathBuf) -> Self {
        Self {
            workspaces: RwLock::new(HashMap::new()),
            indexes_dir,
        }
    }

    /// Get an existing workspace by ID without initializing.
    pub fn get(&self, workspace_id: &str) -> Option<Arc<JulieWorkspace>> {
        self.workspaces
            .read()
            .unwrap()
            .get(workspace_id)
            .map(|entry| Arc::clone(&entry.workspace))
    }

    /// Get or initialize a workspace. Thread-safe with double-checked locking.
    pub async fn get_or_init(
        &self,
        workspace_id: &str,
        workspace_root: &Path,
    ) -> Result<Arc<JulieWorkspace>> {
        // Fast path: read lock
        {
            let guard = self.workspaces.read().unwrap();
            if let Some(entry) = guard.get(workspace_id) {
                return Ok(Arc::clone(&entry.workspace));
            }
        }

        // Slow path: write lock, double-check
        let mut guard = self.workspaces.write().unwrap();
        if let Some(entry) = guard.get(workspace_id) {
            return Ok(Arc::clone(&entry.workspace));
        }

        // Initialize workspace with centralized paths
        info!(
            "Initializing workspace {} at {} (indexes: {})",
            workspace_id,
            workspace_root.display(),
            self.indexes_dir.display()
        );

        let julie_dir = workspace_root.join(".julie");
        let mut workspace = JulieWorkspace::new(workspace_root.to_path_buf(), julie_dir);

        // Override index paths to use centralized location
        workspace.set_index_root(self.indexes_dir.join(workspace_id));
        workspace.initialize_database()?;
        workspace.initialize_search_index()?;

        let ws = Arc::new(workspace);
        guard.insert(
            workspace_id.to_string(),
            WorkspaceEntry {
                workspace: Arc::clone(&ws),
                indexed: false,
            },
        );

        Ok(ws)
    }

    /// Check if a workspace has completed initial indexing.
    pub fn is_indexed(&self, workspace_id: &str) -> bool {
        self.workspaces
            .read()
            .unwrap()
            .get(workspace_id)
            .is_some_and(|entry| entry.indexed)
    }

    /// Mark a workspace as indexed (called after initial indexing completes).
    pub fn mark_indexed(&self, workspace_id: &str) {
        if let Some(entry) = self.workspaces.write().unwrap().get_mut(workspace_id) {
            entry.indexed = true;
        }
    }

    /// Number of active workspaces in the pool.
    pub fn active_count(&self) -> usize {
        self.workspaces.read().unwrap().len()
    }
}
```

**Important:** This requires adding a `set_index_root` method to `JulieWorkspace` and a `new` constructor that doesn't immediately initialize. Check `src/workspace/mod.rs` for the current `JulieWorkspace` construction pattern and add the minimal changes needed.

Add to `src/daemon/mod.rs`:
```rust
pub mod workspace_pool;
```

- [ ] **Step 4: Add `set_index_root` to JulieWorkspace**

Use `deep_dive(symbol="JulieWorkspace")` to understand the current struct, then add:
- A `pub fn new(root: PathBuf, julie_dir: PathBuf) -> Self` constructor (if not already present)
- A `pub fn set_index_root(&mut self, path: PathBuf)` method that overrides where `workspace_index_path`, `workspace_db_path`, and `workspace_tantivy_path` resolve to

This may require extracting the `indexes_root` into a field rather than deriving it from `julie_dir`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib test_get_or_init 2>&1 | tail -10`
Run: `cargo test --lib test_mark_indexed 2>&1 | tail -10`
Run: `cargo test --lib test_active_workspace_count 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/daemon/workspace_pool.rs src/workspace/mod.rs src/tests/daemon/
git commit -m "feat(v6): add WorkspacePool for shared workspace state"
```

---

## Task 5: Handler Refactoring

**Files:**
- Modify: `src/handler.rs` (add `new_with_shared_workspace`, guard `on_initialized`)
- Test: `src/tests/daemon/handler.rs` (create)
- Modify: `src/tests/daemon/mod.rs` (register)

Add the daemon-mode constructor and the auto-indexing guard.

- [ ] **Step 1: Write failing tests**

Create `src/tests/daemon/handler.rs`:
```rust
use crate::handler::JulieServerHandler;
use crate::daemon::workspace_pool::WorkspacePool;
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_new_with_shared_workspace_creates_handler() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();

    let ws = pool.get_or_init("test_abc12345", &workspace_root).await.unwrap();
    let handler = JulieServerHandler::new_with_shared_workspace(
        ws,
        workspace_root.clone(),
    )
    .await;

    assert!(handler.is_ok());
}

#[tokio::test]
async fn test_shared_workspace_handler_has_own_metrics() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();

    let ws = pool.get_or_init("test_abc12345", &workspace_root).await.unwrap();

    let h1 = JulieServerHandler::new_with_shared_workspace(
        ws.clone(), workspace_root.clone(),
    ).await.unwrap();
    let h2 = JulieServerHandler::new_with_shared_workspace(
        ws, workspace_root,
    ).await.unwrap();

    // Each handler should have independent session metrics
    assert!(!std::sync::Arc::ptr_eq(&h1.session_metrics, &h2.session_metrics));
}

#[tokio::test]
async fn test_auto_indexing_skips_when_already_indexed() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = WorkspacePool::new(indexes_dir);

    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();

    let ws = pool.get_or_init("test_abc12345", &workspace_root).await.unwrap();
    pool.mark_indexed("test_abc12345");

    let handler = JulieServerHandler::new_with_shared_workspace(
        ws, workspace_root,
    ).await.unwrap();

    // is_indexed should already be true (inherited from pool)
    let indexed = handler.is_indexed.read().unwrap();
    assert!(*indexed);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_new_with_shared_workspace 2>&1 | tail -10`
Expected: FAIL (method not found)

- [ ] **Step 3: Implement new_with_shared_workspace**

In `src/handler.rs`, use `deep_dive(symbol="JulieServerHandler::new")` to understand the current constructor. Then add:

```rust
impl JulieServerHandler {
    /// Create a handler for daemon mode, backed by a shared workspace from WorkspacePool.
    /// Each handler gets its own session_metrics and indexing_status, but the workspace
    /// (db, search_index, watcher) is shared across sessions.
    pub async fn new_with_shared_workspace(
        workspace: Arc<JulieWorkspace>,
        workspace_root: PathBuf,
    ) -> Result<Self> {
        let tool_router = Self::build_tool_router();
        let is_indexed = {
            // Check if workspace has symbols (already indexed)
            let has_symbols = workspace.db.as_ref()
                .and_then(|db| db.lock().ok()
                    .and_then(|g| g.count_symbols_for_workspace().ok()))
                .is_some_and(|count| count > 0);
            Arc::new(RwLock::new(has_symbols))
        };

        Ok(Self {
            workspace_root,
            workspace: Arc::new(RwLock::new(Some((*workspace).clone()))),
            is_indexed,
            indexing_status: Arc::new(IndexingStatus::new()),
            session_metrics: Arc::new(SessionMetrics::new()),
            embedding_task: Arc::new(tokio::sync::Mutex::new(None)),
            tool_router,
        })
    }
}
```

**Note:** `JulieWorkspace` may need to implement `Clone`. Check with `deep_dive`. If it doesn't, the shared state model may need `Arc` fields inside `JulieWorkspace` instead. Adjust accordingly.

- [ ] **Step 4: Guard on_initialized auto-indexing**

In `src/handler.rs`, modify the `on_initialized` method:

```rust
async fn on_initialized(&self, _context: NotificationContext<RoleServer>) {
    // Skip auto-indexing if workspace is already indexed (daemon mode: another session
    // may have already indexed this workspace)
    {
        let indexed = self.is_indexed.read().unwrap();
        if *indexed {
            info!("Workspace already indexed, skipping auto-indexing");
            return;
        }
    }

    let handler = self.clone();
    tokio::spawn(async move {
        handler.run_auto_indexing().await;
    });
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib test_new_with_shared_workspace 2>&1 | tail -10`
Run: `cargo test --lib test_auto_indexing_skips 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add src/handler.rs src/tests/daemon/handler.rs src/tests/daemon/mod.rs
git commit -m "feat(v6): add shared-workspace handler constructor and auto-indexing guard"
```

---

## Task 6: CLI Subcommands

**Files:**
- Modify: `src/cli.rs` (add subcommands)
- Modify: `src/main.rs` (route subcommands)
- Test: Manual verification (CLI routing is best tested via integration tests later)

- [ ] **Step 1: Update CLI with subcommands**

Modify `src/cli.rs`. Use `get_symbols(file_path="src/cli.rs")` to see current structure first.

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "julie-server", version, about = "Julie - Code Intelligence Server")]
pub struct Cli {
    /// Workspace root directory
    #[arg(long, global = true)]
    pub workspace: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run as persistent daemon (HTTP + IPC transport)
    Daemon {
        /// HTTP port for Streamable HTTP transport (default: 0 = auto-assign)
        #[arg(long, default_value = "0")]
        port: u16,
    },
    /// Stop the running daemon
    Stop,
    /// Check daemon status
    Status,
}

// Keep existing resolve_workspace_root() unchanged
```

- [ ] **Step 2: Update main.rs to route subcommands**

Modify `src/main.rs`. The default (no subcommand) is adapter mode:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let workspace_root = resolve_workspace_root(&cli);

    match cli.command {
        Some(Command::Daemon { port }) => {
            // TODO: Task 8 implements this
            eprintln!("Daemon mode not yet implemented");
            std::process::exit(1);
        }
        Some(Command::Stop) => {
            // TODO: Task 10 implements this
            eprintln!("Stop not yet implemented");
            std::process::exit(1);
        }
        Some(Command::Status) => {
            // TODO: Task 10 implements this
            eprintln!("Status not yet implemented");
            std::process::exit(1);
        }
        None => {
            // Default: adapter mode (or legacy stdio for now)
            // TODO: Task 9 replaces this with adapter
            run_stdio_server(workspace_root).await?;
        }
    }
    Ok(())
}

/// Current stdio server logic (extracted from existing main)
async fn run_stdio_server(workspace_root: PathBuf) -> Result<()> {
    // ... existing main.rs logic moved here
}
```

- [ ] **Step 3: Verify it still works as before**

Run: `cargo build 2>&1 | tail -5`
Run: `cargo test --lib 2>&1 | tail -20` (quick check, not full suite)
Expected: Compiles, existing tests pass. `julie-server` with no args still runs stdio mode.

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat(v6): add daemon/stop/status CLI subcommands with stdio fallback"
```

---

## Task 7: IPC Transport Layer

**Files:**
- Create: `src/daemon/ipc.rs`
- Test: `src/tests/daemon/ipc.rs`
- Modify: `src/daemon/mod.rs` (add module)

Cross-platform IPC abstraction: Unix domain sockets on macOS/Linux, named pipes on Windows.

- [ ] **Step 1: Write failing tests**

Create `src/tests/daemon/ipc.rs`:
```rust
use crate::daemon::ipc::{IpcListener, IpcStream};
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn test_unix_socket_roundtrip() {
    let dir = tempdir().unwrap();
    let socket_path = dir.path().join("test.sock");

    let listener = IpcListener::bind(&socket_path).await.unwrap();
    assert!(socket_path.exists());

    // Connect from client side
    let client_task = tokio::spawn({
        let path = socket_path.clone();
        async move {
            let mut stream = IpcStream::connect(&path).await.unwrap();
            stream.write_all(b"hello").await.unwrap();
            let mut buf = [0u8; 5];
            stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"world");
        }
    });

    // Accept on server side
    let mut server_stream = listener.accept().await.unwrap();
    let mut buf = [0u8; 5];
    server_stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"hello");
    server_stream.write_all(b"world").await.unwrap();

    client_task.await.unwrap();
}

#[tokio::test]
async fn test_cleanup_removes_socket() {
    let dir = tempdir().unwrap();
    let socket_path = dir.path().join("test.sock");

    let listener = IpcListener::bind(&socket_path).await.unwrap();
    assert!(socket_path.exists());

    listener.cleanup();
    assert!(!socket_path.exists());
}

#[tokio::test]
async fn test_connect_to_nonexistent_socket_fails() {
    let dir = tempdir().unwrap();
    let socket_path = dir.path().join("nonexistent.sock");

    let result = IpcStream::connect(&socket_path).await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_unix_socket_roundtrip 2>&1 | tail -10`
Expected: FAIL

- [ ] **Step 3: Implement IPC abstraction**

Create `src/daemon/ipc.rs`:

```rust
use std::path::{Path, PathBuf};
use tokio::io::{AsyncRead, AsyncWrite};

// --- Unix implementation ---

#[cfg(unix)]
pub struct IpcListener {
    listener: tokio::net::UnixListener,
    path: PathBuf,
}

#[cfg(unix)]
pub struct IpcStream {
    inner: tokio::net::UnixStream,
}

#[cfg(unix)]
impl IpcListener {
    pub async fn bind(path: &Path) -> std::io::Result<Self> {
        // Remove stale socket if it exists
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let listener = tokio::net::UnixListener::bind(path)?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    pub async fn accept(&self) -> std::io::Result<IpcStream> {
        let (stream, _addr) = self.listener.accept().await?;
        Ok(IpcStream { inner: stream })
    }

    pub fn cleanup(self) {
        let _ = std::fs::remove_file(&self.path);
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
impl IpcStream {
    pub async fn connect(path: &Path) -> std::io::Result<Self> {
        let stream = tokio::net::UnixStream::connect(path).await?;
        Ok(Self { inner: stream })
    }
}

#[cfg(unix)]
impl AsyncRead for IpcStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

#[cfg(unix)]
impl AsyncWrite for IpcStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

// --- Windows implementation ---
// TODO: Named pipe implementation using tokio::net::windows::named_pipe
// For Phase 1 development on macOS, the Unix implementation is sufficient.
// Windows support will be added before release.
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_unix_socket_roundtrip 2>&1 | tail -10`
Run: `cargo test --lib test_cleanup_removes_socket 2>&1 | tail -10`
Expected: All PASS (on macOS/Linux)

- [ ] **Step 5: Commit**

```bash
git add src/daemon/ipc.rs src/tests/daemon/ipc.rs src/daemon/mod.rs
git commit -m "feat(v6): add cross-platform IPC transport layer (Unix sockets)"
```

---

## Task 8: Daemon Server

**Files:**
- Modify: `src/daemon/mod.rs` (daemon entry point)
- Create: `src/daemon/session.rs` (session tracking)
- Test: `src/tests/daemon/session.rs`, `src/tests/integration/daemon_lifecycle.rs`
- Modify: `src/main.rs` (wire up daemon subcommand)

The daemon binds IPC + HTTP, accepts connections, creates a `JulieServerHandler` per connection via `WorkspacePool`.

- [ ] **Step 1: Write session tracker tests**

Create `src/tests/daemon/session.rs`:
```rust
use crate::daemon::session::SessionTracker;
use std::time::Duration;

#[test]
fn test_new_session_increments_count() {
    let tracker = SessionTracker::new();
    assert_eq!(tracker.active_count(), 0);

    let _id1 = tracker.add_session();
    assert_eq!(tracker.active_count(), 1);

    let _id2 = tracker.add_session();
    assert_eq!(tracker.active_count(), 2);
}

#[test]
fn test_remove_session_decrements_count() {
    let tracker = SessionTracker::new();
    let id = tracker.add_session();
    assert_eq!(tracker.active_count(), 1);

    tracker.remove_session(&id);
    assert_eq!(tracker.active_count(), 0);
}

#[test]
fn test_is_idle_when_no_sessions() {
    let tracker = SessionTracker::new();
    assert!(tracker.is_idle());
}

#[test]
fn test_not_idle_when_sessions_active() {
    let tracker = SessionTracker::new();
    let _id = tracker.add_session();
    assert!(!tracker.is_idle());
}
```

- [ ] **Step 2: Implement SessionTracker**

Create `src/daemon/session.rs`:
```rust
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

pub struct SessionTracker {
    sessions: RwLock<HashSet<String>>,
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashSet::new()),
        }
    }

    pub fn add_session(&self) -> String {
        let id = Uuid::new_v4().to_string();
        self.sessions.write().unwrap().insert(id.clone());
        id
    }

    pub fn remove_session(&self, id: &str) {
        self.sessions.write().unwrap().remove(id);
    }

    pub fn active_count(&self) -> usize {
        self.sessions.read().unwrap().len()
    }

    pub fn is_idle(&self) -> bool {
        self.sessions.read().unwrap().is_empty()
    }
}
```

Add `uuid = { version = "1", features = ["v4"] }` to `Cargo.toml` if not already present.

- [ ] **Step 3: Implement daemon entry point**

In `src/daemon/mod.rs`, add the main daemon function:

```rust
pub mod ipc;
pub mod pid;
pub mod session;
pub mod workspace_pool;

use crate::handler::JulieServerHandler;
use crate::paths::DaemonPaths;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tracing::info;

use self::ipc::IpcListener;
use self::pid::PidFile;
use self::session::SessionTracker;
use self::workspace_pool::WorkspacePool;

/// Run Julie in daemon mode. Binds IPC + HTTP, serves multiple clients.
pub async fn run_daemon(workspace_root: PathBuf, http_port: u16) -> Result<()> {
    let paths = DaemonPaths::new();
    paths.ensure_dirs()?;

    // Create PID file
    let pid_file = PidFile::create(&paths.daemon_pid())?;
    info!("Daemon started with PID {}", std::process::id());

    // Create shared resources
    let pool = Arc::new(WorkspacePool::new(paths.indexes_dir()));
    let tracker = Arc::new(SessionTracker::new());

    // Bind IPC listener
    #[cfg(unix)]
    let ipc_listener = IpcListener::bind(&paths.daemon_socket()).await?;
    info!("IPC listening on {:?}", paths.daemon_socket());

    // Accept IPC connections
    let ipc_pool = Arc::clone(&pool);
    let ipc_tracker = Arc::clone(&tracker);
    let ipc_task = tokio::spawn(async move {
        loop {
            match ipc_listener.accept().await {
                Ok(stream) => {
                    let session_id = ipc_tracker.add_session();
                    let pool = Arc::clone(&ipc_pool);
                    let tracker = Arc::clone(&ipc_tracker);
                    info!("New IPC session: {}", session_id);

                    tokio::spawn(async move {
                        if let Err(e) = handle_ipc_session(stream, pool, &session_id).await {
                            tracing::warn!("IPC session {} error: {}", session_id, e);
                        }
                        tracker.remove_session(&session_id);
                        info!("IPC session {} disconnected", session_id);
                    });
                }
                Err(e) => {
                    tracing::error!("IPC accept error: {}", e);
                }
            }
        }
    });

    // TODO: HTTP transport (Streamable HTTP via rmcp + axum)
    // For Phase 1 initial implementation, IPC is the priority.
    // HTTP will be added as a follow-up step in this task.

    // Wait for shutdown signal
    signal::ctrl_c().await?;
    info!("Shutdown signal received");

    // Cleanup
    ipc_task.abort();
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(paths.daemon_socket());
    }
    pid_file.cleanup();
    info!("Daemon shut down cleanly");

    Ok(())
}

/// Handle a single IPC session: create an MCP handler and serve it.
async fn handle_ipc_session(
    stream: ipc::IpcStream,
    pool: Arc<WorkspacePool>,
    session_id: &str,
) -> Result<()> {
    // TODO: Read workspace root from the adapter's initial message,
    // then create handler via pool.get_or_init + new_with_shared_workspace,
    // then serve MCP over the IPC stream using rmcp.
    //
    // This requires understanding how rmcp serves over arbitrary AsyncRead+AsyncWrite.
    // The rmcp `transport-async-rw` feature may be needed.
    // Implementation details depend on rmcp 1.x API.
    Ok(())
}
```

- [ ] **Step 4: Wire daemon subcommand in main.rs**

Replace the TODO in `main.rs`:
```rust
Some(Command::Daemon { port }) => {
    // Set up daemon logging to ~/.julie/daemon.log
    let paths = crate::paths::DaemonPaths::new();
    setup_daemon_logging(&paths)?;
    crate::daemon::run_daemon(workspace_root, port).await?;
}
```

- [ ] **Step 5: Run tests and verify compilation**

Run: `cargo test --lib test_new_session 2>&1 | tail -10`
Run: `cargo build 2>&1 | tail -10`
Expected: Tests pass, compiles

- [ ] **Step 6: Commit**

```bash
git add src/daemon/ src/main.rs src/tests/daemon/
git commit -m "feat(v6): add daemon server with IPC accept loop and session tracking"
```

---

## Task 9: Adapter (Auto-Start + Byte Forwarding)

**Files:**
- Create: `src/adapter/mod.rs`
- Create: `src/adapter/launcher.rs`
- Test: `src/tests/adapter/mod.rs`, `src/tests/adapter/launcher.rs`
- Modify: `src/lib.rs` (add `pub mod adapter`)
- Modify: `src/main.rs` (wire adapter as default mode)

- [ ] **Step 1: Write launcher tests**

Create `src/tests/adapter/launcher.rs`:
```rust
use crate::adapter::launcher::DaemonLauncher;
use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;
use tempfile::tempdir;

#[test]
fn test_daemon_not_running_when_no_pid_file() {
    let dir = tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());

    let launcher = DaemonLauncher::new(paths);
    assert!(!launcher.is_daemon_running());
}

#[test]
fn test_daemon_detected_as_running_with_valid_pid() {
    let dir = tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());

    // Write current PID (we know this process is alive)
    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();

    let launcher = DaemonLauncher::new(paths);
    assert!(launcher.is_daemon_running());
}

#[test]
fn test_stale_pid_detected_and_cleaned() {
    let dir = tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());

    // Write a PID that doesn't exist
    std::fs::create_dir_all(dir.path()).unwrap();
    std::fs::write(paths.daemon_pid(), "99999999\n").unwrap();

    let launcher = DaemonLauncher::new(paths.clone());
    assert!(!launcher.is_daemon_running());
    // Stale PID file should be cleaned up
    assert!(!paths.daemon_pid().exists());
}
```

- [ ] **Step 2: Implement DaemonLauncher**

Create `src/adapter/launcher.rs`:
```rust
use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;
use std::time::Duration;
use tracing::{info, warn};

pub struct DaemonLauncher {
    paths: DaemonPaths,
}

impl DaemonLauncher {
    pub fn new(paths: DaemonPaths) -> Self {
        Self { paths }
    }

    /// Check if the daemon is running (valid PID file with alive process).
    pub fn is_daemon_running(&self) -> bool {
        PidFile::check_running(&self.paths.daemon_pid()).is_some()
    }

    /// Ensure the daemon is running. If not, acquire lock and start it.
    pub async fn ensure_daemon_running(&self) -> std::io::Result<()> {
        if self.is_daemon_running() {
            return Ok(());
        }

        // Acquire advisory lock to serialize concurrent adapter starts
        let lock_path = self.paths.daemon_lock();
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock_file = std::fs::File::create(&lock_path)?;
        fs2::FileExt::lock_exclusive(&lock_file)?;

        // Double-check after acquiring lock
        if self.is_daemon_running() {
            fs2::FileExt::unlock(&lock_file)?;
            return Ok(());
        }

        // Spawn daemon as detached process
        info!("Starting daemon...");
        let exe = std::env::current_exe()?;
        let child = std::process::Command::new(&exe)
            .arg("daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        info!("Daemon spawned with PID {}", child.id());

        // Release lock (daemon will create its own PID file)
        fs2::FileExt::unlock(&lock_file)?;

        // Wait for socket to become available
        self.wait_for_socket(Duration::from_secs(5)).await
    }

    /// Poll for the daemon socket with exponential backoff.
    async fn wait_for_socket(&self, timeout: Duration) -> std::io::Result<()> {
        let start = std::time::Instant::now();
        let mut delay = Duration::from_millis(50);

        loop {
            #[cfg(unix)]
            if self.paths.daemon_socket().exists() {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Daemon failed to start within timeout",
                ));
            }

            tokio::time::sleep(delay).await;
            delay = std::cmp::min(delay * 2, Duration::from_millis(500));
        }
    }
}
```

- [ ] **Step 3: Implement adapter byte-forwarding**

Create `src/adapter/mod.rs`:
```rust
pub mod launcher;

use crate::daemon::ipc::IpcStream;
use crate::paths::DaemonPaths;
use anyhow::Result;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tracing::{error, info};

/// Run Julie in adapter mode (default).
/// Auto-starts daemon if not running, then forwards bytes between stdin/stdout and IPC.
pub async fn run_adapter() -> Result<()> {
    let paths = DaemonPaths::new();
    let launcher = launcher::DaemonLauncher::new(paths.clone());

    // Ensure daemon is running
    launcher.ensure_daemon_running().await?;

    // Connect to daemon via IPC
    #[cfg(unix)]
    let stream = IpcStream::connect(&paths.daemon_socket()).await?;
    info!("Connected to daemon");

    // Split IPC stream and stdio
    let (ipc_read, ipc_write) = tokio::io::split(stream);
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    // Bidirectional byte forwarding
    let stdin_to_ipc = tokio::spawn(async move {
        let mut stdin = stdin;
        let mut ipc_write = ipc_write;
        io::copy(&mut stdin, &mut ipc_write).await
    });

    let ipc_to_stdout = tokio::spawn(async move {
        let mut ipc_read = ipc_read;
        let mut stdout = stdout;
        io::copy(&mut ipc_read, &mut stdout).await
    });

    // Wait for either direction to complete (means connection closed)
    tokio::select! {
        r = stdin_to_ipc => {
            if let Ok(Err(e)) = r {
                error!("stdin->ipc error: {}", e);
            }
        }
        r = ipc_to_stdout => {
            if let Ok(Err(e)) = r {
                error!("ipc->stdout error: {}", e);
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 4: Wire adapter in main.rs**

Replace the default case in `main.rs`:
```rust
None => {
    // Adapter mode: auto-start daemon, forward stdio to IPC
    crate::adapter::run_adapter().await?;
}
```

- [ ] **Step 5: Run tests and verify compilation**

Run: `cargo test --lib test_daemon_not_running 2>&1 | tail -10`
Run: `cargo test --lib test_stale_pid 2>&1 | tail -10`
Run: `cargo build 2>&1 | tail -10`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add src/adapter/ src/tests/adapter/ src/lib.rs src/main.rs
git commit -m "feat(v6): add adapter with auto-start daemon and byte forwarding"
```

---

## Task 10: Stop and Status Commands

**Files:**
- Modify: `src/main.rs` (implement stop/status)
- Test: `src/tests/daemon/lifecycle.rs` (create)

- [ ] **Step 1: Write tests**

Create `src/tests/daemon/lifecycle.rs`:
```rust
use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;
use tempfile::tempdir;

#[test]
fn test_status_reports_not_running() {
    let dir = tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    let status = crate::daemon::check_status(&paths);
    assert_eq!(status, crate::daemon::DaemonStatus::NotRunning);
}

#[test]
fn test_status_reports_running_with_valid_pid() {
    let dir = tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    let _pid = PidFile::create(&paths.daemon_pid()).unwrap();

    let status = crate::daemon::check_status(&paths);
    assert!(matches!(status, crate::daemon::DaemonStatus::Running { .. }));
}
```

- [ ] **Step 2: Implement status and stop**

In `src/daemon/mod.rs`, add:
```rust
#[derive(Debug, PartialEq)]
pub enum DaemonStatus {
    Running { pid: u32 },
    NotRunning,
}

pub fn check_status(paths: &DaemonPaths) -> DaemonStatus {
    match PidFile::check_running(&paths.daemon_pid()) {
        Some(pid) => DaemonStatus::Running { pid },
        None => DaemonStatus::NotRunning,
    }
}

pub fn stop_daemon(paths: &DaemonPaths) -> Result<()> {
    match PidFile::check_running(&paths.daemon_pid()) {
        Some(pid) => {
            info!("Sending shutdown signal to daemon PID {}", pid);
            #[cfg(unix)]
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            #[cfg(windows)]
            {
                // Use taskkill or TerminateProcess
                std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .output()?;
            }
            // Wait briefly for cleanup
            std::thread::sleep(std::time::Duration::from_secs(1));
            // Clean up stale files if daemon didn't clean up
            let _ = std::fs::remove_file(paths.daemon_pid());
            #[cfg(unix)]
            let _ = std::fs::remove_file(paths.daemon_socket());
            Ok(())
        }
        None => {
            info!("Daemon is not running");
            Ok(())
        }
    }
}
```

Wire in `main.rs`:
```rust
Some(Command::Stop) => {
    let paths = crate::paths::DaemonPaths::new();
    crate::daemon::stop_daemon(&paths)?;
    println!("Daemon stopped");
}
Some(Command::Status) => {
    let paths = crate::paths::DaemonPaths::new();
    match crate::daemon::check_status(&paths) {
        crate::daemon::DaemonStatus::Running { pid } => {
            println!("Julie daemon running (PID {})", pid);
        }
        crate::daemon::DaemonStatus::NotRunning => {
            println!("Julie daemon not running");
        }
    }
}
```

- [ ] **Step 3: Run tests and verify**

Run: `cargo test --lib test_status_reports 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/daemon/mod.rs src/main.rs src/tests/daemon/
git commit -m "feat(v6): add daemon stop and status commands"
```

---

## Task 11: Index Migration

**Files:**
- Create: `src/migration.rs`
- Test: `src/tests/migration.rs`
- Modify: `src/lib.rs` (add module)
- Modify: `src/daemon/mod.rs` (call migration on startup)

Safe migration of existing indexes from `{project}/.julie/indexes/` to `~/.julie/indexes/`.

- [ ] **Step 1: Write failing tests**

Create `src/tests/migration.rs`:
```rust
use crate::migration::{MigrationState, migrate_workspace_index};
use tempfile::tempdir;
use std::path::PathBuf;

#[test]
fn test_migrate_copies_and_validates() {
    let temp = tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let central_dir = temp.path().join("central");

    // Create a fake project-local index
    let local_index = project_dir.join(".julie").join("indexes").join("proj_abc12345").join("db");
    std::fs::create_dir_all(&local_index).unwrap();
    std::fs::write(local_index.join("symbols.db"), "fake-db-content").unwrap();

    let tantivy_dir = project_dir.join(".julie").join("indexes").join("proj_abc12345").join("tantivy");
    std::fs::create_dir_all(&tantivy_dir).unwrap();
    std::fs::write(tantivy_dir.join("meta.json"), "{}").unwrap();

    // Run migration
    let result = migrate_workspace_index(
        "proj_abc12345",
        &project_dir.join(".julie").join("indexes").join("proj_abc12345"),
        &central_dir.join("proj_abc12345"),
    );
    assert!(result.is_ok());

    // Verify: central copy exists
    assert!(central_dir.join("proj_abc12345").join("db").join("symbols.db").exists());
    assert!(central_dir.join("proj_abc12345").join("tantivy").join("meta.json").exists());

    // Verify: original deleted
    assert!(!project_dir.join(".julie").join("indexes").join("proj_abc12345").exists());
}

#[test]
fn test_migration_state_tracks_progress() {
    let temp = tempdir().unwrap();
    let state_path = temp.path().join("migration.json");

    let mut state = MigrationState::new(&state_path);
    assert!(!state.is_migrated("proj_abc12345"));

    state.mark_migrated("proj_abc12345");
    state.save().unwrap();

    // Reload and verify
    let state2 = MigrationState::load(&state_path).unwrap();
    assert!(state2.is_migrated("proj_abc12345"));
}

#[test]
fn test_skip_already_migrated() {
    let temp = tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let central_dir = temp.path().join("central");

    // Central index already exists
    let central_index = central_dir.join("proj_abc12345").join("db");
    std::fs::create_dir_all(&central_index).unwrap();
    std::fs::write(central_index.join("symbols.db"), "existing").unwrap();

    // Local index also exists
    let local_index = project_dir.join(".julie").join("indexes").join("proj_abc12345").join("db");
    std::fs::create_dir_all(&local_index).unwrap();
    std::fs::write(local_index.join("symbols.db"), "local").unwrap();

    // Migration should detect central already exists and skip
    let result = migrate_workspace_index(
        "proj_abc12345",
        &project_dir.join(".julie").join("indexes").join("proj_abc12345"),
        &central_dir.join("proj_abc12345"),
    );
    assert!(result.is_ok());

    // Central should still have "existing" (not overwritten)
    let content = std::fs::read_to_string(central_dir.join("proj_abc12345").join("db").join("symbols.db")).unwrap();
    assert_eq!(content, "existing");
}
```

- [ ] **Step 2: Implement migration**

Create `src/migration.rs` with `MigrationState` and `migrate_workspace_index`. Use copy-validate-delete approach per spec. Implement `scan_project_indexes` to discover existing per-project indexes.

- [ ] **Step 3: Wire migration into daemon startup**

In `src/daemon/mod.rs::run_daemon`, call migration before creating the `WorkspacePool`:
```rust
// Migrate existing per-project indexes to centralized location
crate::migration::run_migration(&paths, &workspace_root)?;
```

- [ ] **Step 4: Run tests and verify**

Run: `cargo test --lib test_migrate_copies 2>&1 | tail -10`
Run: `cargo test --lib test_migration_state 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/migration.rs src/tests/migration.rs src/daemon/mod.rs src/lib.rs
git commit -m "feat(v6): add safe index migration from per-project to centralized"
```

---

## Task 12: Integration Tests + End-to-End Verification

**Files:**
- Create: `src/tests/integration/daemon_lifecycle.rs`
- Modify: `src/tests/integration/mod.rs` (register)

- [ ] **Step 1: Write daemon lifecycle integration test**

Create `src/tests/integration/daemon_lifecycle.rs`:
```rust
//! Integration tests for daemon startup, adapter connection, and shutdown.
//!
//! These tests verify the full lifecycle: daemon starts, adapter connects,
//! tool calls work through the pipe, and shutdown is clean.

use tempfile::tempdir;
use std::time::Duration;

/// Test that the daemon starts, creates PID file and socket, and shuts down cleanly.
#[tokio::test(flavor = "multi_thread")]
async fn test_daemon_lifecycle_start_and_stop() {
    let temp = tempdir().unwrap();
    let paths = crate::paths::DaemonPaths::with_home(temp.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    // Start daemon in background
    let daemon_paths = paths.clone();
    let daemon_task = tokio::spawn(async move {
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        crate::daemon::run_daemon(workspace, 0).await
    });

    // Wait for daemon to be ready
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify PID file exists
    assert!(paths.daemon_pid().exists());

    // Verify socket exists
    #[cfg(unix)]
    assert!(paths.daemon_socket().exists());

    // Stop daemon
    crate::daemon::stop_daemon(&paths).unwrap();

    // Wait for shutdown
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify cleanup
    assert!(!paths.daemon_pid().exists());
}

/// Test that WorkspacePool correctly shares workspace across simulated sessions.
#[tokio::test]
async fn test_workspace_pool_sharing() {
    let temp = tempdir().unwrap();
    let indexes_dir = temp.path().join("indexes");
    let pool = crate::daemon::workspace_pool::WorkspacePool::new(indexes_dir);

    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();
    // Create a source file so indexing has something to work with
    std::fs::write(workspace_root.join("main.rs"), "fn main() {}").unwrap();

    // Simulate two sessions attaching to the same workspace
    let ws1 = pool.get_or_init("proj_abc12345", &workspace_root).await.unwrap();
    let ws2 = pool.get_or_init("proj_abc12345", &workspace_root).await.unwrap();

    // Same underlying database
    assert!(std::sync::Arc::ptr_eq(
        ws1.db.as_ref().unwrap(),
        ws2.db.as_ref().unwrap()
    ));
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --lib test_daemon_lifecycle 2>&1 | tail -20`
Run: `cargo test --lib test_workspace_pool_sharing 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 3: Run full dev test tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All green. No regressions from Phase 1 changes.

- [ ] **Step 4: Commit**

```bash
git add src/tests/integration/daemon_lifecycle.rs src/tests/integration/mod.rs
git commit -m "test(v6): add daemon lifecycle and workspace sharing integration tests"
```

---

## Task 13: Dogfood Verification

**Not code; manual verification with the real system.**

- [ ] **Step 1: Build release**

```bash
cargo build --release 2>&1 | tail -5
```

- [ ] **Step 2: Test daemon lifecycle manually**

```bash
# Start daemon
./target/release/julie-server daemon &

# Check status
./target/release/julie-server status

# Stop daemon
./target/release/julie-server stop
```

- [ ] **Step 3: Test adapter auto-start**

Configure `.mcp.json` to point at the new binary (adapter mode, no args).
Start Claude Code. Verify Julie tools work.
Start a second Claude Code session on the same project. Verify both sessions can search without `LockBusy` errors.

- [ ] **Step 4: Verify logs**

Check `~/.julie/daemon.log` for lifecycle events.
Check `{project}/.julie/logs/` for project-scoped tool logs.

- [ ] **Step 5: Verify restart workflow**

```bash
cargo build --release
julie-server stop
# Next tool call should auto-start new binary
```

- [ ] **Step 6: Commit any fixes discovered during dogfooding**

- [ ] **Step 7: Final checkpoint**

Save a Goldfish checkpoint documenting Phase 1 completion, any issues discovered during dogfooding, and readiness for Phase 2.

---

## Dependency Graph

```
Task 1 (rmcp bump)
    |
    v
Task 2 (paths) -----> Task 3 (PID file)
    |                      |
    v                      v
Task 4 (WorkspacePool) -> Task 5 (handler refactoring)
    |
    v
Task 6 (CLI subcommands)
    |
    v
Task 7 (IPC transport) -> Task 8 (daemon server) -> Task 9 (adapter)
                               |
                               v
                          Task 10 (stop/status)
                               |
                               v
                          Task 11 (migration)
                               |
                               v
                          Task 12 (integration tests) -> Task 13 (dogfood)
```

Tasks 1-5 can be parallelized across subagents (no file conflicts). Tasks 6+ are sequential.
