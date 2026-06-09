//! F-A gate tests (codex 3c.2 pre-merge, fixed in 3c.3): in-process handlers
//! PIN their primary workspace to the canonical startup hint and never rebind
//! to client `list_roots`, so the leader lock + storage (acquired at startup
//! from the hint-derived id) and the request-time binding can never diverge.
//!
//! Three invariants:
//!   1. `request_prefers_client_roots()` is `false` for in-process handlers
//!      (leader AND follower) regardless of source; daemon/stdio (`none()`)
//!      keep the source-driven behavior.
//!   2. An in-process FOLLOWER's `run_primary_workspace_repair` is a no-op
//!      (returns `Ok(None)`) — followers never write through ANY entry path,
//!      including the non-deferred `on_initialized` → `run_auto_indexing` path.
//!   3. The in-process workspace_id derivation canonicalizes the hint path, so
//!      non-canonical and canonical forms of one dir yield the same id (the
//!      property that couples the leader lock to the request-time binding).

use crate::handler::JulieServerHandler;
use crate::leadership::LeadershipState;
use crate::registry::discovery::DaemonLockGuard;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

async fn build_in_process(
    dir: &std::path::Path,
    leadership: LeadershipState,
    source: WorkspaceStartupSource,
) -> JulieServerHandler {
    JulieServerHandler::new_in_process(
        WorkspaceStartupHint {
            path: dir.to_path_buf(),
            source: Some(source),
        },
        None,
        leadership,
        None,
    )
    .await
    .unwrap()
}

/// Invariant 1: in-process handlers (leader + follower) never prefer client
/// roots; daemon/stdio `none()` handlers honor the startup source.
#[tokio::test]
async fn test_request_prefers_client_roots_pins_in_process_to_hint() {
    let dir = tempfile::tempdir().unwrap();

    // Leader with Cwd source — would normally prefer client roots, but
    // is_in_process() pins it to the hint → false.
    let lock_path = dir.path().join(".leader.lock");
    let guard = DaemonLockGuard::try_acquire(&lock_path).expect("lock acquirable");
    let leader = build_in_process(
        dir.path(),
        LeadershipState::leader(guard),
        WorkspaceStartupSource::Cwd,
    )
    .await;
    assert!(
        !leader.request_prefers_client_roots(),
        "in-process LEADER must pin to the startup hint (no list_roots rebind), \
         even with Cwd source"
    );

    // Follower with Cwd source — also pinned.
    let dir2 = tempfile::tempdir().unwrap();
    let follower = build_in_process(
        dir2.path(),
        LeadershipState::follower(),
        WorkspaceStartupSource::Cwd,
    )
    .await;
    assert!(
        !follower.request_prefers_client_roots(),
        "in-process FOLLOWER must pin to the startup hint (no list_roots rebind)"
    );

    // none() (daemon/stdio) with Cwd source — keeps source-driven rebind.
    let dir3 = tempfile::tempdir().unwrap();
    let none_cwd = build_in_process(
        dir3.path(),
        LeadershipState::none(),
        WorkspaceStartupSource::Cwd,
    )
    .await;
    assert!(
        none_cwd.request_prefers_client_roots(),
        "daemon/stdio (none()) with Cwd source MUST still prefer client roots \
         (multi-root rebind preserved for the daemon path)"
    );

    // none() with Cli source — pinned by source, not by in-process gate.
    let dir4 = tempfile::tempdir().unwrap();
    let none_cli = build_in_process(
        dir4.path(),
        LeadershipState::none(),
        WorkspaceStartupSource::Cli,
    )
    .await;
    assert!(
        !none_cli.request_prefers_client_roots(),
        "Cli source never prefers client roots"
    );
}

/// Invariant 2: an in-process follower's repair entry point is a no-op. This
/// guards the non-deferred `run_auto_indexing` → `run_primary_workspace_repair`
/// path, which (unlike the deferred path) had no follower guard before the fix.
#[tokio::test]
async fn test_in_process_follower_repair_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    let follower = build_in_process(
        dir.path(),
        LeadershipState::follower(),
        WorkspaceStartupSource::Cwd,
    )
    .await;

    let plan = crate::startup::run_primary_workspace_repair(&follower)
        .await
        .expect("follower repair must not error");
    assert!(
        plan.is_none(),
        "in-process follower must perform NO repair/index work (pure reader); \
         got a repair plan instead of Ok(None)"
    );
}

/// Invariant 3: the workspace_id derivation canonicalizes, so equivalent path
/// spellings of one directory resolve to the same id. This is the property
/// that couples the startup-acquired leader lock id to the request-time
/// binding id (both run through `canonicalize_workspace_path` + `generate_workspace_id`).
#[test]
fn test_in_process_workspace_id_is_canonicalized() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();

    // A non-canonical spelling that resolves to the same directory.
    let non_canonical = dir.path().join("sub").join("..");
    let canonical = dir.path().to_path_buf();

    let id_from_noncanon = generate_workspace_id(
        &JulieServerHandler::canonicalize_workspace_path(non_canonical).to_string_lossy(),
    )
    .unwrap();
    let id_from_canon = generate_workspace_id(
        &JulieServerHandler::canonicalize_workspace_path(canonical).to_string_lossy(),
    )
    .unwrap();

    assert_eq!(
        id_from_noncanon, id_from_canon,
        "canonicalization must collapse equivalent path spellings to one \
         workspace_id (couples leader lock id to binding id)"
    );
}
