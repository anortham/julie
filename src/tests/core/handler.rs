// Tests for src/handler.rs — JulieServerHandler construction and lifecycle.

use crate::dashboard::state::DashboardEvent;
use crate::database::types::FileInfo;
use crate::handler::{JulieServerHandler, metrics_db_path_for_workspace};
use crate::tools::metrics::session::ToolCallReport;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::sync::broadcast;

#[tokio::test(flavor = "multi_thread")]
async fn handler_construction_sets_workspace_root() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;
    // workspace_root should be set to cwd (the default for new_for_test)
    assert!(handler.workspace_root.is_absolute() || handler.workspace_root.as_os_str() == ".");
    assert_eq!(handler.current_workspace_root(), handler.workspace_root);
    assert_eq!(handler.current_workspace_id(), None);
    // workspace should start as None (lazy init)
    let ws = handler.workspace.read().await;
    assert!(
        ws.is_none(),
        "workspace should be None before initialization"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_active_workspace_wal_returns_none_before_initialization() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let checkpoint = crate::startup::checkpoint_active_workspace_wal(&handler).await?;

    assert!(
        checkpoint.is_none(),
        "no workspace should mean no checkpoint"
    );
    Ok(())
}

/// D-H2: Two concurrent on_initialized calls on a shared is_indexed must not
/// both claim the indexing slot. The write-lock check-and-set pattern is atomic.
#[tokio::test(flavor = "multi_thread")]
async fn test_auto_index_write_lock_prevents_double_spawn() {
    let is_indexed = Arc::new(tokio::sync::RwLock::new(false));
    let spawn_count = Arc::new(AtomicUsize::new(0));

    let check_and_maybe_spawn = |flag: Arc<tokio::sync::RwLock<bool>>,
                                 counter: Arc<AtomicUsize>| async move {
        // This is the fixed on_initialized pattern: write-lock + check-and-set.
        let mut guard = flag.write().await;
        if *guard {
            return;
        }
        *guard = true;
        drop(guard);
        counter.fetch_add(1, Ordering::SeqCst);
    };

    tokio::join!(
        check_and_maybe_spawn(Arc::clone(&is_indexed), Arc::clone(&spawn_count)),
        check_and_maybe_spawn(Arc::clone(&is_indexed), Arc::clone(&spawn_count)),
    );

    assert_eq!(
        spawn_count.load(Ordering::SeqCst),
        1,
        "Only one concurrent caller should claim the indexing slot"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_active_workspace_wal_runs_after_workspace_initialization() -> Result<()> {
    let workspace = TempDir::new()?;
    let handler = JulieServerHandler::new(workspace.path().to_path_buf()).await?;
    handler.initialize_workspace(None).await?;

    let checkpoint = crate::startup::checkpoint_active_workspace_wal(&handler).await?;

    assert!(
        checkpoint.is_some(),
        "initialized workspace should expose a database for checkpointing"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_active_workspace_wal_uses_rebound_current_primary_store() -> Result<()> {
    let first_workspace = TempDir::new()?;
    let rebound_workspace = TempDir::new()?;

    let handler = JulieServerHandler::new(first_workspace.path().to_path_buf()).await?;
    handler.initialize_workspace(None).await?;

    let rebound_root = rebound_workspace.path().canonicalize()?;
    let rebound_id =
        crate::workspace::registry::generate_workspace_id(&rebound_root.to_string_lossy())?;
    handler.set_current_primary_binding(rebound_id.clone(), rebound_root);

    let rebound_db_path = handler.workspace_db_file_path_for(&rebound_id).await?;
    std::fs::create_dir_all(rebound_db_path.parent().expect("rebound db parent"))?;
    let _ = crate::database::SymbolDatabase::new(&rebound_db_path)?;

    let rebound_db = handler.get_database_for_workspace(&rebound_id).await?;
    let _rebound_guard = rebound_db.lock().unwrap();

    let checkpoint_err = crate::startup::checkpoint_active_workspace_wal(&handler)
        .await
        .expect_err(
            "checkpoint should target the rebound current-primary db and hit the held lock",
        );
    assert!(
        checkpoint_err
            .to_string()
            .contains("Could not acquire database lock for checkpoint"),
        "checkpoint should use rebound current-primary db, not stale loaded db: {checkpoint_err}"
    );

    Ok(())
}

#[test]
fn metrics_db_path_helper_uses_current_workspace_root_for_local_storage() {
    let current_root = PathBuf::from("/tmp/rebound-primary");
    let db_path = metrics_db_path_for_workspace(None, &current_root, "ref_workspace");

    assert_eq!(
        db_path,
        PathBuf::from("/tmp/rebound-primary/.julie/indexes/ref_workspace/db/symbols.db")
    );
}

#[test]
fn workspace_root_uri_helper_parses_local_file_uri() {
    let path =
        JulieServerHandler::workspace_path_from_root_uri_for_test("file:///tmp/workspace-root")
            .expect("file uri should parse");

    assert_eq!(path, PathBuf::from("/tmp/workspace-root"));
}

#[cfg(windows)]
#[test]
fn workspace_root_uri_helper_parses_unc_file_uri() {
    let path =
        JulieServerHandler::workspace_path_from_root_uri_for_test("file://server/share/project")
            .expect("UNC file uri should parse");

    assert_eq!(path, PathBuf::from(r"\\server\share\project"));
}

#[test]
fn metrics_db_path_helper_uses_shared_index_parent_when_override_exists() {
    let current_root = PathBuf::from("/tmp/rebound-primary");
    let override_root = PathBuf::from("/tmp/shared/indexes/primary_ws");
    let db_path =
        metrics_db_path_for_workspace(Some(&override_root), &current_root, "ref_workspace");

    assert_eq!(
        db_path,
        PathBuf::from("/tmp/shared/indexes/ref_workspace/db/symbols.db")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_record_tool_call_uses_binding_snapshot_for_metrics_attribution() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::workspace::registry::generate_workspace_id;
    use rusqlite::Connection;
    use std::time::Duration;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    std::fs::create_dir_all(original_root.join("src"))?;
    std::fs::create_dir_all(rebound_root.join("src"))?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;
    let original_ws = pool
        .get_or_init(&original_id, original_path.clone())
        .await?;
    let source_file_rel = "src/original.rs".to_string();
    let source_bytes = 321_u64;
    std::fs::write(original_root.join(&source_file_rel), "fn original() {}\n")?;
    {
        let db_arc = original_ws
            .db
            .as_ref()
            .expect("original workspace should have a db");
        let db = db_arc.lock().expect("original workspace db should lock");
        db.store_file_info(&FileInfo {
            path: source_file_rel.clone(),
            language: "rust".to_string(),
            hash: "original-hash".to_string(),
            size: source_bytes as i64,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 0,
            line_count: 1,
            content: Some("fn original() {}\n".to_string()),
        })?;
    }

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    let (dashboard_tx, mut dashboard_rx) = broadcast::channel(8);
    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
        Some(dashboard_tx),
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    let binding_snapshot = handler.require_primary_workspace_binding()?;
    handler.set_current_primary_binding(rebound_id.clone(), rebound_path);
    handler
        .publish_loaded_workspace_swap_teardown_gap_for_test()
        .await;

    let mut report = ToolCallReport::empty();
    report.source_file_paths = vec![source_file_rel.clone()];
    handler.record_tool_call(
        "fast_search",
        Duration::from_millis(5),
        &report,
        Some(&binding_snapshot),
    );

    match dashboard_rx.recv().await? {
        DashboardEvent::ToolCall { workspace, .. } => {
            assert_eq!(
                workspace, original_id,
                "dashboard event should use call-start workspace"
            );
        }
        other => panic!("unexpected dashboard event: {other:?}"),
    }

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let daemon_count: i64 = {
                let conn = daemon_db.conn_for_test();
                conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0))?
            };
            let local_count: i64 = {
                let conn =
                    Connection::open(indexes_dir.join(&original_id).join("db").join("symbols.db"))?;
                conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0))?
            };
            if daemon_count > 0 && local_count > 0 {
                break Ok::<(), rusqlite::Error>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    let recorded_workspace: String = {
        let conn = daemon_db.conn_for_test();
        conn.query_row(
            "SELECT workspace_id FROM tool_calls ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
    };
    assert_eq!(
        recorded_workspace, original_id,
        "daemon metrics row should use call-start workspace"
    );

    let recorded_daemon_source_bytes: Option<u64> = {
        let conn = daemon_db.conn_for_test();
        conn.query_row(
            "SELECT source_bytes FROM tool_calls ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
    };
    assert_eq!(
        recorded_daemon_source_bytes,
        Some(source_bytes),
        "daemon metrics row should preserve source_bytes from the snapshotted workspace db"
    );

    let recorded_local_source_bytes: Option<u64> = {
        let conn = Connection::open(indexes_dir.join(&original_id).join("db").join("symbols.db"))?;
        conn.query_row(
            "SELECT source_bytes FROM tool_calls ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
    };
    assert_eq!(
        recorded_local_source_bytes,
        Some(source_bytes),
        "local workspace metrics row should still write during the teardown gap"
    );
    assert_eq!(
        handler.session_metrics.total_source_bytes(),
        source_bytes,
        "session metrics should include source_bytes resolved from the snapshotted workspace db"
    );

    Ok(())
}
