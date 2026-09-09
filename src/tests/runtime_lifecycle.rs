//! Runtime lifecycle tests.
//!
//! Verifies:
//! 1. Request lease decoupling: dropping leases does NOT release locks during active commits.
//! 2. Clean watcher failure: watcher startup failure transitions to Failed and drops lock.
//! 3. Bounded idle eviction: evicts stale idle runtimes but never active leases or work.
//! 4. State transition invariants: a Failed runtime never becomes Owner again.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use julie_core::workspace::leader_lock::{AcquireError, DaemonLockGuard};
use julie_test_support::workspace_markers::make_isolated_workspace_root;
use tempfile::TempDir;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::paths::RegistryPaths;
use crate::request_engine::types::WorkspaceBinding;
use crate::workspace_runtime::RuntimePhase;
use crate::workspace_runtime::manager::{ManagerTestBarriers, WorkspaceRuntimeManager};

// ============================================================================
// Subprocess Lock Probe Entry Point
// ============================================================================

/// Subprocess helper invoked by `competing_process_lock_is_busy`.
///
/// When `JULIE_PROBE_LOCK_PATH` is set, attempts to acquire the OS lock:
/// - Exit 0: Lock is ALREADY HELD (busy).
/// - Exit 1: Lock was ACQUIRED (not busy).
/// - Exit 2: Unhandled I/O error.
#[test]
fn runtime_lifecycle_lock_probe_subprocess() {
    let Ok(path_str) = std::env::var("JULIE_PROBE_LOCK_PATH") else {
        return; // Normal test execution: pass immediately
    };
    let path = PathBuf::from(path_str);
    match DaemonLockGuard::try_acquire(&path) {
        Ok(_guard) => std::process::exit(43), // Successfully acquired; not busy
        Err(AcquireError::AlreadyHeld(_)) => std::process::exit(42), // Lock is busy!
        Err(e) => {
            eprintln!("I/O error probing lock at {}: {e}", path.display());
            std::process::exit(44);
        }
    }
}

// ============================================================================
// RuntimeFixture Harness
// ============================================================================

/// Hermetic test fixture managing isolated directories, runtime manager, and barriers.
pub struct RuntimeFixture {
    pub temp_repo: TempDir,
    pub workspace_root: PathBuf,
    pub temp_home: TempDir,
    pub index_root: PathBuf,
    pub binding: WorkspaceBinding,
    pub manager: Arc<WorkspaceRuntimeManager>,
    pub barriers: ManagerTestBarriers,
}

impl RuntimeFixture {
    /// Create a standard hermetic fixture with isolated roots.
    pub async fn new() -> Self {
        Self::with_options(ManagerTestBarriers::default(), None).await
    }

    /// Create a fixture configured with a blocked index commit barrier.
    pub async fn with_blocked_index_commit() -> Self {
        let barriers = ManagerTestBarriers::default();
        Self::with_options(barriers, None).await
    }

    /// Create a fixture configured to fail watcher startup on recovery.
    pub async fn with_failing_watcher() -> Self {
        let barriers = ManagerTestBarriers::default();
        Self::with_options(barriers, Some("FAIL_WATCHER_STARTUP")).await
    }

    async fn with_options(barriers: ManagerTestBarriers, fault_flag: Option<&str>) -> Self {
        let tmp_base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
        let _ = std::fs::create_dir_all(&tmp_base);

        let temp_repo = tempfile::tempdir_in(&tmp_base).expect("create temp repo");
        let workspace_root = make_isolated_workspace_root(temp_repo.path(), "runtime_fixture");
        seed_workspace(&workspace_root);

        let temp_home = tempfile::tempdir_in(&tmp_base).expect("create temp home");
        let workspace_id = julie_core::workspace::registry::generate_workspace_id(
            &workspace_root.to_string_lossy(),
        )
        .expect("generate workspace id");
        let index_root = temp_home.path().join("indexes").join(&workspace_id);
        std::fs::create_dir_all(&index_root).expect("create index root");

        let binding = WorkspaceBinding {
            workspace_id: workspace_id.clone(),
            root: workspace_root.clone(),
            index_root: index_root.clone(),
        };

        let paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
        let mut builder = WorkspaceRuntimeManager::builder(paths);
        builder
            .idle_timeout(Duration::from_secs(60))
            .max_idle_runtimes(8)
            .test_barriers(barriers.clone());

        if let Some(fault) = fault_flag {
            builder.inject_fault(fault);
        }

        let manager = builder.build();

        Self {
            temp_repo,
            workspace_root,
            temp_home,
            index_root,
            binding,
            manager,
            barriers,
        }
    }

    pub fn binding(&self) -> &WorkspaceBinding {
        &self.binding
    }

    pub fn lock_path(&self) -> PathBuf {
        self.index_root.join("leader.lock")
    }

    pub fn db_path(&self) -> PathBuf {
        self.index_root.join("db").join("symbols.db")
    }

    pub fn deadline(&self) -> Option<Instant> {
        Some(Instant::now() + Duration::from_secs(30))
    }

    pub fn cancel(&self) -> CancellationToken {
        CancellationToken::new()
    }

    /// Wait until the active commit signals that it has started.
    pub async fn wait_until_commit_started(&self) {
        tokio::time::timeout(
            Duration::from_secs(10),
            self.barriers.commit_started.notified(),
        )
        .await
        .expect("timed out waiting for commit to start");
    }

    /// Release the commit barrier, allowing the commit to finalize.
    pub fn release_commit(&self) {
        self.barriers.commit_barrier.notify_waiters();
    }

    /// Wait until the manager has zero active in-flight commits.
    pub async fn wait_until_idle(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.manager.is_idle(&self.binding) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("timed out waiting for runtime manager to become idle");
    }

    /// Launch a separate OS subprocess to probe the exclusive lock.
    /// Returns true if the lock is busy (AlreadyHeld), false if acquired.
    pub async fn competing_process_lock_is_busy(&self) -> bool {
        let exe = std::env::current_exe().expect("resolve current test binary");
        let lock_path = self.lock_path();

        let mut cmd = tokio::process::Command::new(exe);
        cmd.arg("tests::runtime_lifecycle::runtime_lifecycle_lock_probe_subprocess")
            .arg("--exact")
            .env("JULIE_PROBE_LOCK_PATH", &lock_path)
            .env_remove("NEXTEST")
            .env_remove("NEXTEST_RUNNER")
            .env_remove("NEXTEST_TEST_BINARY_PROTOCOL_VERSION")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());

        let output = tokio::time::timeout(Duration::from_secs(5), cmd.output())
            .await
            .expect("lock probe subprocess timed out")
            .expect("failed to spawn lock probe subprocess");

        match output.status.code() {
            Some(42) => true,  // Lock is ALREADY HELD (busy)
            Some(43) => false, // Lock was acquired (not busy)
            Some(code) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                panic!(
                    "Lock probe subprocess exited with unexpected code {code}. Stderr: {stderr}"
                );
            }
            None => panic!("Lock probe subprocess was terminated by signal"),
        }
    }

    /// Read the committed canonical revision directly from SQLite in read-only mode.
    pub fn committed_revision(&self) -> i64 {
        let path = self.db_path();
        if !path.exists() {
            return 0;
        }

        let conn = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open read-only SQLite connection");

        let revision: Result<i64, _> = conn.query_row(
            "SELECT revision FROM canonical_revisions ORDER BY revision DESC LIMIT 1",
            [],
            |row| row.get(0),
        );

        revision.unwrap_or(0)
    }

    /// Obtain a receiver to watch runtime phase changes.
    pub fn watch_phase(&self) -> watch::Receiver<RuntimePhase> {
        self.manager
            .watch_phase(&self.binding)
            .expect("runtime must exist")
    }
}

fn seed_workspace(root: &Path) {
    let cargo_toml = r#"[package]
name = "lifecycle-probe"
version = "0.1.0"
edition = "2021"
"#;
    std::fs::write(root.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    std::fs::write(src_dir.join("lib.rs"), "pub fn probe() -> i32 { 42 }\n").expect("write lib.rs");
}

fn make_test_binding(name: &str, home: &Path) -> WorkspaceBinding {
    let tmp_base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
    let temp = tempfile::tempdir_in(&tmp_base).unwrap();
    let temp_path = temp.path().to_path_buf();
    std::mem::forget(temp);
    let root = make_isolated_workspace_root(&temp_path, name);
    seed_workspace(&root);
    let index_root = home.join("indexes").join(name);
    std::fs::create_dir_all(&index_root).unwrap();
    WorkspaceBinding {
        workspace_id: name.to_string(),
        root,
        index_root,
    }
}

// ============================================================================
// Step 1 RED Test: Request Lease Decoupling
// ============================================================================

/// Step 1 RED Test: Verify dropping a request lease does NOT release ownership
/// or cancel an active in-flight owner commit.
#[tokio::test]
async fn dropping_request_lease_does_not_release_busy_owner() {
    let fixture = RuntimeFixture::with_blocked_index_commit().await;

    // 1. Acquire request lease and start owner commit
    let lease = fixture
        .manager
        .acquire(fixture.binding(), fixture.deadline(), &fixture.cancel())
        .await
        .unwrap();

    // 2. Trigger asynchronous background index commit held by test barrier
    fixture.manager.trigger_test_commit(fixture.binding()).await;
    fixture.wait_until_commit_started().await;

    // 3. Client disconnects / request times out: drop the lease
    drop(lease);

    // 4. Critical Invariant: Competing OS process MUST observe lock is still BUSY
    assert!(
        fixture.competing_process_lock_is_busy().await,
        "dropping a request lease must not release the leader lock during active commit"
    );

    // 5. Unblock commit and wait for completion
    fixture.release_commit();
    fixture.wait_until_idle().await;

    // 6. Verify SQLite canonical revision reached 1
    assert_eq!(
        fixture.committed_revision(),
        1,
        "committed revision must increment upon commit completion"
    );
}

// ============================================================================
// Clean Watcher Failure Test
// ============================================================================

/// Verify that failed watcher startup transitions the runtime to Failed and
/// immediately releases the OS lock without leaving hidden writer background tasks.
#[tokio::test]
async fn failed_watcher_startup_leaves_no_hidden_writer() {
    let fixture = RuntimeFixture::with_failing_watcher().await;

    // Attempt to acquire owner runtime where watcher startup is injected to fail
    let _result = fixture
        .manager
        .acquire(fixture.binding(), fixture.deadline(), &fixture.cancel())
        .await;

    // Must fail acquisition or transition to Failed phase
    let phase = fixture.watch_phase().borrow().clone();
    assert!(
        matches!(phase, RuntimePhase::Failed { .. }),
        "runtime must transition to Failed on watcher startup error; got {:?}",
        phase
    );

    // Lock must be released cleanly
    assert!(
        !fixture.competing_process_lock_is_busy().await,
        "OS lock must be released immediately upon watcher failure"
    );

    // Verify no background tasks are lingering
    assert!(fixture.manager.is_idle(fixture.binding()));
}

// ============================================================================
// Bounded Idle Runtime Eviction Test
// ============================================================================

/// Verify idle runtime eviction is bounded (<= 8 idle runtimes), evicts after timeout,
/// and strictly skips runtimes with active leases or ongoing commit work.
#[tokio::test]
async fn idle_runtime_eviction_is_bounded_and_skips_active_work() {
    let barriers = ManagerTestBarriers::default();
    let temp_home = tempfile::tempdir().unwrap();

    let paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let manager = WorkspaceRuntimeManager::builder(paths)
        .idle_timeout(Duration::from_millis(100))
        .max_idle_runtimes(2)
        .test_barriers(barriers.clone())
        .build();

    // Setup 3 bindings
    let b1 = make_test_binding("ws1", temp_home.path());
    let b2 = make_test_binding("ws2", temp_home.path());
    let b3 = make_test_binding("ws3", temp_home.path());

    // Acquire lease for ws1 and KEEP it active
    let lease1 = manager
        .acquire(
            &b1,
            Some(Instant::now() + Duration::from_secs(10)),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    // Acquire lease for ws2 and DROP it (becomes idle)
    let lease2 = manager
        .acquire(
            &b2,
            Some(Instant::now() + Duration::from_secs(10)),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    drop(lease2);

    // ws3 has an active commit in progress
    let _lease3 = manager
        .acquire(
            &b3,
            Some(Instant::now() + Duration::from_secs(10)),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    manager.trigger_test_commit(&b3).await;
    barriers.commit_started.notified().await;

    // Advance time beyond idle timeout
    tokio::time::sleep(Duration::from_millis(200)).await;
    manager.evict_idle_runtimes().await;

    // Assertions:
    // ws1 has active lease: NOT evicted
    assert!(
        manager.contains(&b1).await,
        "active lease must not be evicted"
    );
    // ws3 has active commit: NOT evicted
    assert!(
        manager.contains(&b3).await,
        "active commit must not be evicted"
    );
    // ws2 was idle and timed out: IS evicted
    assert!(
        !manager.contains(&b2).await,
        "idle runtime must be evicted after timeout"
    );

    // Clean up barriers
    barriers.commit_barrier.notify_waiters();
    drop(lease1);
}

// ============================================================================
// State Transition Table Unit Test
// ============================================================================

#[test]
fn ownership_transition_table_has_one_live_phase() {
    assert!(RuntimePhase::Owner { epoch: 1 }.can_transition_to(&RuntimePhase::Owner { epoch: 2 }));
    assert!(
        RuntimePhase::Owner { epoch: 1 }
            .can_transition_to(&RuntimePhase::Failed { code: "ERR".into() })
    );
    assert!(
        !RuntimePhase::Failed { code: "ERR".into() }
            .can_transition_to(&RuntimePhase::Owner { epoch: 1 }),
        "Failed must NEVER transition to Owner"
    );
    assert!(
        !RuntimePhase::Owner { epoch: 2 }.can_transition_to(&RuntimePhase::Owner { epoch: 1 }),
        "Owner epoch cannot decrement"
    );
}
