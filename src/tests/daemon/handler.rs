use std::sync::Arc;

use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;

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
    )
    .await
    .expect("new_with_shared_workspace should succeed");

    // Handler should have the workspace set
    let ws_guard = handler.workspace.read().await;
    assert!(ws_guard.is_some(), "workspace should be set");
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
    )
    .await
    .expect("handler should succeed");

    let indexed = handler.is_indexed.read().await;
    assert!(
        !*indexed,
        "is_indexed should be false when workspace has no symbols"
    );
}
