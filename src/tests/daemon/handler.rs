use std::sync::Arc;

use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

fn temp_indexes_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

fn temp_workspace_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    std::fs::create_dir_all(dir.path().join(".julie")).expect("Failed to create .julie dir");
    dir
}

#[tokio::test]
async fn test_new_with_shared_workspace_creates_handler() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

    let ws = pool
        .get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    let handler = JulieServerHandler::new_with_shared_workspace(
        ws,
        workspace_root.path().to_path_buf(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("new_with_shared_workspace should succeed");

    // Handler should have the workspace set
    let ws_guard = handler.workspace.read().await;
    assert!(ws_guard.is_some(), "workspace should be set");
}

#[tokio::test]
async fn handler_construction_uses_startup_hint_for_current_root() {
    let workspace_root = temp_workspace_root();

    let handler = JulieServerHandler::new(workspace_root.path().to_path_buf())
        .await
        .expect("new should succeed");

    assert_eq!(
        handler.workspace_startup_hint().path,
        workspace_root.path().to_path_buf()
    );
    assert_eq!(handler.workspace_startup_hint().source, None);
    assert_eq!(handler.current_workspace_root(), workspace_root.path());
    assert_eq!(handler.current_workspace_id(), None);
}

#[tokio::test]
async fn test_new_with_shared_workspace_preserves_startup_hint() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);
    let startup_hint = WorkspaceStartupHint {
        path: workspace_root.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };

    let ws = pool
        .get_or_init("primary_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        ws,
        startup_hint.clone(),
        None,
        Some("primary_ws".to_string()),
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("new_with_shared_workspace_startup_hint should succeed");

    assert_eq!(handler.workspace_startup_hint(), startup_hint);
    assert_eq!(
        handler.current_workspace_id(),
        Some("primary_ws".to_string())
    );
    assert_eq!(handler.current_workspace_root(), workspace_root.path());
}

#[tokio::test]
async fn test_shared_workspace_handler_has_own_metrics() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

    let ws = pool
        .get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    let handler1 = JulieServerHandler::new_with_shared_workspace(
        Arc::clone(&ws),
        workspace_root.path().to_path_buf(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("handler1 should succeed");

    let handler2 = JulieServerHandler::new_with_shared_workspace(
        Arc::clone(&ws),
        workspace_root.path().to_path_buf(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("handler2 should succeed");

    // Each handler should have its own session_metrics (different Arc pointers)
    assert!(
        !Arc::ptr_eq(&handler1.session_metrics, &handler2.session_metrics),
        "handlers should have independent session_metrics"
    );

    // Each handler should have its own indexing_status
    assert!(
        !Arc::ptr_eq(&handler1.indexing_status, &handler2.indexing_status),
        "handlers should have independent indexing_status"
    );
}

#[tokio::test]
async fn test_shared_workspace_handler_shares_database() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

    let ws = pool
        .get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    let handler1 = JulieServerHandler::new_with_shared_workspace(
        Arc::clone(&ws),
        workspace_root.path().to_path_buf(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("handler1 should succeed");

    let handler2 = JulieServerHandler::new_with_shared_workspace(
        Arc::clone(&ws),
        workspace_root.path().to_path_buf(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("handler2 should succeed");

    // Both handlers should share the same database Arc
    let ws1 = handler1.workspace.read().await;
    let ws2 = handler2.workspace.read().await;
    let db1 = ws1.as_ref().unwrap().db.as_ref().expect("db1 should exist");
    let db2 = ws2.as_ref().unwrap().db.as_ref().expect("db2 should exist");
    assert!(
        Arc::ptr_eq(db1, db2),
        "handlers should share the same database instance"
    );
}

#[tokio::test]
async fn test_handler_is_indexed_when_workspace_has_symbols() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

    let ws = pool
        .get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    // Insert a dummy symbol so the db reports count > 0
    {
        let db_arc = ws.db.as_ref().expect("db should exist");
        let db = db_arc.lock().unwrap();
        // Insert a file first (foreign key constraint)
        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified)
                 VALUES ('dummy.rs', 'rust', 'abc123', 100, 0)",
                [],
            )
            .expect("file insert should succeed");
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, file_path, start_line, end_line, start_col, end_col, language)
                 VALUES ('dummy_1', 'dummy', 'Function', 'dummy.rs', 1, 1, 0, 0, 'rust')",
                [],
            )
            .expect("symbol insert should succeed");
    }

    let handler = JulieServerHandler::new_with_shared_workspace(
        ws,
        workspace_root.path().to_path_buf(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("handler should succeed");

    let indexed = handler.is_indexed.read().await;
    assert!(
        *indexed,
        "is_indexed should be true when workspace has symbols"
    );
}

#[tokio::test]
async fn test_handler_not_indexed_when_workspace_empty() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

    let ws = pool
        .get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    let handler = JulieServerHandler::new_with_shared_workspace(
        ws,
        workspace_root.path().to_path_buf(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("handler should succeed");

    let indexed = handler.is_indexed.read().await;
    assert!(
        !*indexed,
        "is_indexed should be false when workspace has no symbols"
    );
}

#[tokio::test]
async fn test_active_workspace_set_is_seeded_from_primary() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

    let ws = pool
        .get_or_init("primary_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    let handler = JulieServerHandler::new_with_shared_workspace(
        ws,
        workspace_root.path().to_path_buf(),
        None,
        Some("primary_ws".to_string()),
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("new_with_shared_workspace should succeed");

    let active_workspaces = handler.active_workspace_ids().await;
    assert_eq!(active_workspaces, vec!["primary_ws".to_string()]);
}

#[tokio::test]
async fn test_active_workspace_set_tracks_secondary_activation() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

    let ws = pool
        .get_or_init("primary_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    let handler = JulieServerHandler::new_with_shared_workspace(
        ws,
        workspace_root.path().to_path_buf(),
        None,
        Some("primary_ws".to_string()),
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("new_with_shared_workspace should succeed");

    handler.activate_workspace("secondary_ws").await;

    let mut active_workspaces = handler.active_workspace_ids().await;
    active_workspaces.sort();

    assert!(active_workspaces.contains(&"primary_ws".to_string()));
    assert!(active_workspaces.contains(&"secondary_ws".to_string()));
    assert_eq!(active_workspaces.len(), 2);
    assert!(handler.is_workspace_active("secondary_ws").await);
}

/// Regression guard for the v6.8.0-yank bug: a deferred daemon session that
/// performs a non-force primary initialization (what `run_auto_indexing` does
/// on the first request) must land its primary workspace in the shared
/// `WorkspacePool`, not in the project-local `.julie/indexes/` directory.
///
/// Prior to the fix, `initialize_workspace_with_force(..., false)` in a
/// deferred session took the `JulieWorkspace::initialize` / `detect_and_load`
/// branch because `use_pooled_rebind` required either a loaded-root change or
/// a force flag. A fresh deferred session has `self.workspace == None`, so
/// `loaded_workspace_root_changed` was false; with `force == false` the gate
/// bypassed the pool entirely. Subsequent primary-scoped tool calls then hit
/// Finding #38's pool-membership guard and returned
/// "Current primary workspace ... is not attached in the daemon workspace
/// pool" to the user.
#[tokio::test]
async fn test_deferred_primary_init_without_force_populates_pool() {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::embedding_service::EmbeddingService;
    use std::sync::atomic::AtomicBool;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let canonical_root = workspace_root.path().canonicalize().expect("canonicalize root");

    // Compute the workspace id the way the handler will when it normalizes
    // the target path inside `initialize_workspace_with_force`.
    let workspace_id = crate::workspace::registry::generate_workspace_id(
        &canonical_root.to_string_lossy(),
    )
    .expect("generate_workspace_id");

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(
        DaemonDatabase::open(&daemon_db_path).expect("open daemon.db"),
    );
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
    ));
    let restart_pending = Arc::new(AtomicBool::new(false));

    let handler = JulieServerHandler::new_deferred_daemon_startup_hint(
        WorkspaceStartupHint {
            path: canonical_root.clone(),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&restart_pending)),
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("deferred handler construction");

    // Sanity: deferred session starts with no loaded workspace and the pool empty.
    assert!(
        handler.get_workspace().await.expect("read workspace").is_none(),
        "deferred session should start with no loaded workspace"
    );
    assert!(
        pool.get(&workspace_id).await.is_none(),
        "pool should start empty for this workspace"
    );

    // Simulate what `run_auto_indexing` does on the first request in a
    // deferred session: init the primary at the startup root without force.
    handler
        .initialize_workspace_with_force(Some(canonical_root.to_string_lossy().to_string()), false)
        .await
        .expect("initialize_workspace_with_force should succeed");

    // The bug: the init routed through the project-local JulieWorkspace::initialize
    // path instead of the WorkspacePool, leaving the pool empty while session
    // state ended up marking the workspace as attached. This is the exact
    // precondition that made Finding #38's guard trip on the next primary call.
    assert!(
        pool.get(&workspace_id).await.is_some(),
        "pool must contain the primary workspace after a non-force deferred init in daemon mode"
    );

    // Exercise the user-facing guard that was firing: get_database_for_workspace
    // walks through ensure_primary_pool_membership_for and must succeed for the
    // current primary now that the pool has the entry.
    handler
        .set_current_primary_binding(workspace_id.clone(), canonical_root.clone());
    handler
        .get_database_for_workspace(&workspace_id)
        .await
        .expect("primary DB acquisition must succeed after pool-routed init");
}

/// Same bug class, different angle: a daemon session with an already-pooled
/// primary that calls `initialize_workspace_with_force(same_root, false)` must
/// reuse the existing pool entry via `pool.get`, not re-init via `get_or_init`.
/// This guards against two regressions:
///   (a) double-incrementing the daemon.db `session_count` for one session
///   (b) drifting the loaded workspace from its pooled arcs on a no-op reinit
///
/// Before the widened gate, same-root non-force hit the `detect_and_load`
/// branch which is a different kind of wrong; after the fix it routes through
/// `acquire_pooled_workspace_for_rebind`, and the session-attached fast path
/// there takes `pool.get` (no count bump).
#[tokio::test]
async fn test_same_root_reinit_reuses_pool_entry_without_double_attach() {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::embedding_service::EmbeddingService;
    use std::sync::atomic::AtomicBool;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let canonical_root = workspace_root.path().canonicalize().expect("canonicalize root");

    let workspace_id = crate::workspace::registry::generate_workspace_id(
        &canonical_root.to_string_lossy(),
    )
    .expect("generate_workspace_id");

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(
        DaemonDatabase::open(&daemon_db_path).expect("open daemon.db"),
    );
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
    ));
    let restart_pending = Arc::new(AtomicBool::new(false));

    let handler = JulieServerHandler::new_deferred_daemon_startup_hint(
        WorkspaceStartupHint {
            path: canonical_root.clone(),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&restart_pending)),
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("deferred handler construction");

    // First init: fresh session takes pool.get_or_init, bumping session_count to 1.
    handler
        .initialize_workspace_with_force(Some(canonical_root.to_string_lossy().to_string()), false)
        .await
        .expect("first init");

    let count_after_first = daemon_db
        .get_workspace(&workspace_id)
        .expect("read workspace row")
        .expect("workspace registered after first init")
        .session_count;
    assert_eq!(
        count_after_first, 1,
        "first init must register exactly one session attach"
    );

    // Second init for the same root, still no force: session state now says
    // "attached", so `acquire_pooled_workspace_for_rebind` should take the
    // `pool.get` branch and MUST NOT increment session_count again.
    handler
        .initialize_workspace_with_force(Some(canonical_root.to_string_lossy().to_string()), false)
        .await
        .expect("second init");

    let count_after_second = daemon_db
        .get_workspace(&workspace_id)
        .expect("read workspace row")
        .expect("workspace still registered")
        .session_count;
    assert_eq!(
        count_after_second, 1,
        "same-root non-force reinit must not double-attach the session"
    );

    // And the pool entry is still there and reusable.
    assert!(
        pool.get(&workspace_id).await.is_some(),
        "pool entry must survive a same-root non-force reinit"
    );
}
