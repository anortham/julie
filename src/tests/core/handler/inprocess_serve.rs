//! T8 gate-invariant tests for `run_in_process_server` / `new_in_process` + F2.
//!
//! Two assertions required by the task:
//!
//! 1. **F2 inode test** — `paths.workspace_leader_lock(ws)`, the workspace db
//!    path, and the tantivy path all resolve to the SAME `{index_root}` parent
//!    (`~/.julie/indexes/{ws}/`). A second `try_acquire` on the same lock path
//!    while the first guard is held → `AlreadyHeld`.
//!
//! 2. **Serve build test** — A handler built via the `run_in_process_server`
//!    build sequence (factored for testability: construct handler → initialize
//!    workspace) stores db/tantivy under the daemon shared index dir (not the
//!    project-local `.julie/` dir), proving F2 in practice.
//!
//! Transport-level coverage note: a true stdio-transport round-trip (feeding
//! JSON-RPC over a pipe and reading responses) is infeasible in the current
//! in-process test harness — `rmcp::transport::stdio()` binds the real process
//! stdin/stdout, which would deadlock a unit test.  Coverage provided instead:
//!   - Handler construction and workspace initialization via the exact code path
//!     used by `run_in_process_server`.
//!   - DB file existence at the daemon-shared path (proves F2 storage coupling).
//!   - Lock contention assertion (proves the lock inode is shared).
//! The `InProcessDaemonBuilder` harness in `src/tests/harness/in_process.rs`
//! provides full MCP round-trip coverage for the daemon HTTP path; a future task
//! can add a stdio pipe harness using `tokio::io::duplex`.

use crate::daemon::discovery::{AcquireError, DaemonLockGuard};
use crate::handler::JulieServerHandler;
use crate::leadership::LeadershipState;
use crate::paths::DaemonPaths;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

// ---------------------------------------------------------------------------
// Test 1: F2 inode coupling — lock and storage share one index_root dir tree
// ---------------------------------------------------------------------------

/// Gate invariant (F2 hard gate):
/// `paths.workspace_leader_lock(ws)`, `workspace_db_path(ws)`, and
/// `workspace_tantivy_path(ws)` all resolve under the SAME
/// `~/.julie/indexes/{ws}/` tree.
///
/// A second `try_acquire` on the same lock path while the first guard is held
/// returns `AlreadyHeld` — proving both cross-process OS lock contention and
/// in-process dedup fire on the same inode.
#[tokio::test]
async fn test_f2_inode_coupling_same_index_tree() {
    let home_dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(home_dir.path().to_path_buf());

    let project_dir = tempfile::tempdir().unwrap();
    let workspace_id =
        generate_workspace_id(&project_dir.path().to_string_lossy()).unwrap();

    let index_root = paths.workspace_index_dir(&workspace_id);
    let lock_path = paths.workspace_leader_lock(&workspace_id);
    let db_path = paths.workspace_db_path(&workspace_id);
    let tantivy_path = paths.workspace_tantivy_path(&workspace_id);

    // F2 assertion 1: lock is a direct child of index_root.
    assert_eq!(
        lock_path.parent().unwrap(),
        index_root.as_path(),
        "leader.lock must be a direct child of index_root; got {}",
        lock_path.display()
    );

    // F2 assertion 2: db and tantivy live under index_root.
    assert!(
        db_path.starts_with(&index_root),
        "db path must be under index_root;\n  db:    {}\n  root:  {}",
        db_path.display(),
        index_root.display()
    );
    assert!(
        tantivy_path.starts_with(&index_root),
        "tantivy path must be under index_root;\n  tantivy: {}\n  root:    {}",
        tantivy_path.display(),
        index_root.display()
    );

    // F2 assertion 3: two `try_acquire` calls on the same lock path contend —
    // the second gets `AlreadyHeld` while the first guard is alive.
    std::fs::create_dir_all(&index_root).unwrap();
    let guard = DaemonLockGuard::try_acquire(&lock_path)
        .expect("first try_acquire must succeed on a fresh lock path");

    let result = DaemonLockGuard::try_acquire(&lock_path);
    assert!(
        matches!(result, Err(AcquireError::AlreadyHeld(_))),
        "second try_acquire on the same lock path must return AlreadyHeld; \
         got: {:?}",
        result.map(|_| "Ok(guard)")
    );

    // Release and verify a third acquire succeeds (guard was not leaked).
    drop(guard);
    let _third = DaemonLockGuard::try_acquire(&lock_path)
        .expect("after drop, lock must be re-acquirable");
}

// ---------------------------------------------------------------------------
// Test 2: Serve build — handler + initialize_workspace_with_force uses F2 path
// ---------------------------------------------------------------------------

/// Verify that a handler built via the `run_in_process_server` construction
/// sequence (new_in_process with `Some(index_root)`) stores the workspace
/// database under the daemon-shared index directory, not the project-local
/// `.julie/indexes/` directory.
///
/// This proves F2 in practice: the leader lock and the workspace storage share
/// one inode tree, making `DaemonLockGuard::try_acquire` the exclusive write gate.
#[tokio::test]
async fn test_inprocess_handler_f2_storage_under_index_root() {
    let home_dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(home_dir.path().to_path_buf());

    let project_dir = tempfile::tempdir().unwrap();
    let workspace_id =
        generate_workspace_id(&project_dir.path().to_string_lossy()).unwrap();

    // Mirror the run_in_process_server build sequence exactly.
    let index_root = paths.workspace_index_dir(&workspace_id);
    std::fs::create_dir_all(&index_root).unwrap();

    let lock_path = paths.workspace_leader_lock(&workspace_id);
    let guard =
        DaemonLockGuard::try_acquire(&lock_path).expect("lock must be acquirable on fresh path");

    let startup_hint = WorkspaceStartupHint {
        path: project_dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };

    let handler = JulieServerHandler::new_in_process(
        startup_hint.clone(),
        /*embedding_provider=*/ None,
        LeadershipState::leader(guard),
        Some(index_root.clone()),
    )
    .await
    .expect("handler must build via new_in_process");

    // Initialize workspace — this is the path that goes through
    // JulieWorkspace::initialize_with_index_root when in_process_index_root is set.
    handler
        .initialize_workspace_with_force(
            Some(project_dir.path().to_string_lossy().to_string()),
            /*force=*/ false,
        )
        .await
        .expect("workspace initialization must succeed");

    // F2 storage assertion: db must exist under the daemon-shared index_root,
    // NOT under the project-local .julie/indexes/ path.
    let expected_db = paths.workspace_db_path(&workspace_id);
    assert!(
        expected_db.starts_with(&index_root),
        "db path must be under daemon index_root (F2);\n  db:   {}\n  root: {}",
        expected_db.display(),
        index_root.display()
    );
    assert!(
        expected_db.exists(),
        "db file must exist at the daemon-shared path (F2);\n  path: {}",
        expected_db.display()
    );

    // Confirm the project-local .julie/indexes/ path does NOT hold the db.
    let project_local_indexes = project_dir.path().join(".julie").join("indexes");
    let project_local_db = project_local_indexes
        .join(&workspace_id)
        .join("db")
        .join("symbols.db");
    assert!(
        !project_local_db.exists(),
        "db must NOT be at project-local path (F2 violated);\n  path: {}",
        project_local_db.display()
    );

    // Confirm handler is a leader (lock is held).
    assert!(
        handler.is_leader(),
        "handler built with leader guard must report is_leader() == true"
    );
}

// ---------------------------------------------------------------------------
// Test 3: F-B (codex pre-merge) — force reindex preserves F2 storage coupling
// ---------------------------------------------------------------------------

/// A FORCE reindex on an in-process leader must keep db/tantivy under the
/// daemon-shared `index_root` (NOT fall back to project-local `.julie/indexes`)
/// and must preserve the held `leader.lock` file.
///
/// Before the F-B fix the force branch of `initialize_workspace_with_force`
/// ignored `in_process_index_root` and called `JulieWorkspace::initialize`,
/// writing project-local storage while the leader lock sat in the daemon path —
/// silently breaking the F2 inode coupling that the non-force branch establishes.
#[tokio::test]
async fn test_inprocess_force_reindex_keeps_f2_storage_and_preserves_lock() {
    let home_dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(home_dir.path().to_path_buf());

    let project_dir = tempfile::tempdir().unwrap();
    let workspace_id = generate_workspace_id(&project_dir.path().to_string_lossy()).unwrap();

    let index_root = paths.workspace_index_dir(&workspace_id);
    std::fs::create_dir_all(&index_root).unwrap();

    let lock_path = paths.workspace_leader_lock(&workspace_id);
    let guard =
        DaemonLockGuard::try_acquire(&lock_path).expect("lock must be acquirable on fresh path");

    let startup_hint = WorkspaceStartupHint {
        path: project_dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };

    let handler = JulieServerHandler::new_in_process(
        startup_hint.clone(),
        None,
        LeadershipState::leader(guard),
        Some(index_root.clone()),
    )
    .await
    .expect("handler must build via new_in_process");

    // Initial (non-force) init lands storage under the daemon-shared index_root.
    handler
        .initialize_workspace_with_force(
            Some(project_dir.path().to_string_lossy().to_string()),
            /*force=*/ false,
        )
        .await
        .expect("initial workspace init must succeed");

    let expected_db = paths.workspace_db_path(&workspace_id);
    assert!(
        expected_db.exists(),
        "db must exist under index_root after the initial init;\n  db: {}",
        expected_db.display()
    );
    assert!(
        lock_path.exists(),
        "leader.lock must exist after the initial init;\n  lock: {}",
        lock_path.display()
    );

    // FORCE reindex — the path that regressed before the F-B fix.
    handler
        .initialize_workspace_with_force(
            Some(project_dir.path().to_string_lossy().to_string()),
            /*force=*/ true,
        )
        .await
        .expect("force reindex must succeed");

    // F-B assertion 1: storage STAYS under the daemon-shared index_root.
    assert!(
        expected_db.starts_with(&index_root) && expected_db.exists(),
        "after force reindex, db must still exist under the daemon index_root (F2);\n  db:   {}\n  root: {}",
        expected_db.display(),
        index_root.display()
    );

    // F-B assertion 2: force must NOT create project-local storage.
    let project_local_db = project_dir
        .path()
        .join(".julie")
        .join("indexes")
        .join(&workspace_id)
        .join("db")
        .join("symbols.db");
    assert!(
        !project_local_db.exists(),
        "force reindex must NOT write project-local storage (F2 violated);\n  path: {}",
        project_local_db.display()
    );

    // F-B assertion 3: the held leader.lock survives the force clear (the clear
    // targets db/tantivy only — never index_root wholesale, which would orphan
    // the lock and let a second process acquire a duplicate leader lock).
    assert!(
        lock_path.exists(),
        "force reindex must preserve the held leader.lock file;\n  lock: {}",
        lock_path.display()
    );

    assert!(
        handler.is_leader(),
        "handler must still report is_leader() after a force reindex"
    );
}
