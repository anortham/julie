use super::*;
use crate::tests::helpers::workspace::mark_workspace_root;

/// Regression test for the per-tool-call `SymbolDatabase::new` cold-open in
/// the metrics writer (`src/handler/tool_metrics.rs`).
///
/// Before 2026-05-17 the metrics writer opened a fresh `SymbolDatabase` per
/// tool call to record `tool_calls` rows, which silently re-ran WAL setup,
/// pragmas, migration checks, and schema init on every invocation. The fix
/// routes the write through the workspace's pooled `WorkspaceConnectionPool`
/// so the same connection is reused.
///
/// This test asserts that recording multiple tool calls produces ZERO
/// "Initializing SQLite database" log lines from `julie::database` — the
/// pool's cold-open uses a different code path that doesn't emit that log.
#[tokio::test(flavor = "multi_thread")]
async fn test_metrics_writer_reuses_pooled_connection_no_cold_open_per_call() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::watcher::observability::LogCapture;
    use crate::workspace::registry::generate_workspace_id;
    use rusqlite::Connection;
    use std::time::Duration;
    use tracing_subscriber::layer::SubscriberExt;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;

    let workspace_root = temp_dir.path().join("ws");
    std::fs::create_dir_all(workspace_root.join("src"))?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
    ));

    let workspace_path = workspace_root.canonicalize()?;
    let workspace_path_str = workspace_path.to_string_lossy().to_string();
    let workspace_id = generate_workspace_id(&workspace_path_str)?;
    daemon_db.upsert_workspace(&workspace_id, &workspace_path_str, "ready")?;
    let workspace = pool
        .get_or_init(&workspace_id, workspace_path.clone())
        .await?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        workspace,
        workspace_path,
        Some(Arc::clone(&daemon_db)),
        Some(workspace_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    // Drive the workspace pool's initial cold-open via a single explicit
    // acquire so the connection lands in the pool's idle queue. Subsequent
    // metrics writes should reuse this connection, not open a new one.
    {
        let conn_pool = pool
            .connection_pool(&workspace_id)
            .await
            .expect("connection pool should be ready after get_or_init");
        let _conn = conn_pool.acquire().await?;
        // Drop returns the connection to the idle queue.
    }

    // Install LogCapture AFTER the initial pool warm-up so the warm-up's
    // own cold-open isn't counted against the metrics-write path under test.
    let capture = LogCapture::new();
    let subscriber = tracing_subscriber::registry().with(capture.layer());
    let _sub_guard = tracing::subscriber::set_default(subscriber);

    let binding = handler.require_primary_workspace_binding()?;

    // Record 3 tool calls; each should write metrics WITHOUT triggering a
    // fresh `SymbolDatabase::new` (which would log "Initializing SQLite
    // database at: ..." from `julie::database`).
    for _ in 0..3 {
        let report = ToolCallReport::empty();
        handler.record_tool_call(
            "fast_search",
            Duration::from_millis(1),
            &report,
            Some(&binding),
        );
    }

    // Wait until at least 3 tool_calls rows land in the workspace DB.
    let workspace_db_path = indexes_dir
        .join(&workspace_id)
        .join("db")
        .join("symbols.db");
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let count: i64 = {
                let conn = Connection::open(&workspace_db_path)?;
                conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0))?
            };
            if count >= 3 {
                break Ok::<(), rusqlite::Error>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    drop(_sub_guard);

    let entries = capture.entries();
    let cold_opens: Vec<_> = entries
        .iter()
        .filter(|e| {
            e.target == "julie::database" && e.message.contains("Initializing SQLite database")
        })
        .collect();

    assert!(
        cold_opens.is_empty(),
        "Metrics writer must reuse pooled connections; got {} cold-open log line(s): {:?}",
        cold_opens.len(),
        cold_opens.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tool_failure_metrics_records_failed_handler_call() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;
    use std::time::Duration;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    std::fs::create_dir_all(temp_dir.path().join("src"))?;
    std::fs::write(temp_dir.path().join("src/lib.rs"), "pub fn hi() {}\n")?;

    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let request =
        CallToolRequestParams::new("get_symbols").with_arguments(json_object(serde_json::json!({
            "file_path": "src/lib.rs",
            "mode": "not-a-real-mode"
        })));
    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        request,
        RequestContext::new(NumberOrString::Number(2001), service.peer().clone()),
    )
    .await;
    assert!(result.is_err(), "get_symbols should fail for invalid mode");

    let db_arc = {
        let workspace = handler.workspace.read().await;
        workspace
            .as_ref()
            .and_then(|workspace| workspace.db.as_ref())
            .expect("indexed workspace should have a database")
            .clone()
    };

    let recorded = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let row = {
                let db = db_arc.lock().expect("workspace db should lock");
                let mut stmt = db.conn.prepare(
                    "SELECT id, tool_name, success, output_bytes, metadata
                     FROM tool_calls
                     WHERE tool_name = 'get_symbols'
                     ORDER BY id DESC LIMIT 1",
                )?;
                let mut rows = stmt.query([])?;
                rows.next()?
                    .map(|row| {
                        Ok::<(i64, String, i64, Option<i64>, Option<String>), rusqlite::Error>((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    })
                    .transpose()?
            };

            if let Some(row) = row {
                break Ok::<(i64, String, i64, Option<i64>, Option<String>), anyhow::Error>(row);
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    let recorded_id = recorded.0;
    assert_eq!(recorded.1, "get_symbols");
    assert_eq!(recorded.2, 0, "failure row should set success=0");
    assert_eq!(recorded.3, Some(0), "failure row should set output_bytes=0");
    let metadata = recorded.4.expect("failure row should include metadata");
    assert!(
        metadata.contains("error.message"),
        "failure metadata should include error.message"
    );

    let missing_file_request =
        CallToolRequestParams::new("get_symbols").with_arguments(json_object(serde_json::json!({
            "file_path": "src/missing_file.rs",
            "max_depth": 1
        })));
    let missing_file_result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        missing_file_request,
        RequestContext::new(NumberOrString::Number(2002), service.peer().clone()),
    )
    .await;
    assert!(
        missing_file_result.is_err(),
        "get_symbols should fail for missing file"
    );

    let missing_recorded = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let row = {
                let db = db_arc.lock().expect("workspace db should lock");
                let mut stmt = db.conn.prepare(
                    "SELECT id, success, output_bytes, metadata
                     FROM tool_calls
                     WHERE tool_name = 'get_symbols'
                     AND id > ?
                     ORDER BY id DESC LIMIT 1",
                )?;
                let mut rows = stmt.query([recorded_id])?;
                rows.next()?
                    .map(|row| {
                        Ok::<(i64, i64, Option<i64>, Option<String>), rusqlite::Error>((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                        ))
                    })
                    .transpose()?
            };

            if let Some(row) = row {
                break Ok::<(i64, i64, Option<i64>, Option<String>), anyhow::Error>(row);
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    assert_eq!(
        missing_recorded.1, 0,
        "missing file failure should set success=0"
    );
    assert_eq!(
        missing_recorded.2,
        Some(0),
        "missing file failure should set output_bytes=0"
    );
    let missing_metadata = missing_recorded.3.expect("missing file failure metadata");
    let missing_metadata: serde_json::Value =
        serde_json::from_str(&missing_metadata).unwrap_or(serde_json::json!(null));
    assert!(
        missing_metadata.to_string().contains("src/missing_file.rs"),
        "missing file failure metadata should include file path"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_deep_dive_failure_metrics_records_failed_handler_call() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;
    use std::time::Duration;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    std::fs::create_dir_all(temp_dir.path().join("src"))?;
    std::fs::write(temp_dir.path().join("src/lib.rs"), "pub fn hi() {}\n")?;

    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let request =
        CallToolRequestParams::new("deep_dive").with_arguments(json_object(serde_json::json!({
            "symbol": "hi",
            "workspace": "missing-workspace-id"
        })));
    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        request,
        RequestContext::new(NumberOrString::Number(2003), service.peer().clone()),
    )
    .await;
    assert!(
        result.is_err(),
        "deep_dive should fail for unknown workspace id"
    );

    let db_arc = {
        let workspace = handler.workspace.read().await;
        workspace
            .as_ref()
            .and_then(|workspace| workspace.db.as_ref())
            .expect("indexed workspace should have a database")
            .clone()
    };

    let recorded = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let row = {
                let db = db_arc.lock().expect("workspace db should lock");
                let mut stmt = db.conn.prepare(
                    "SELECT success, metadata
                     FROM tool_calls
                     WHERE tool_name = 'deep_dive'
                     ORDER BY id DESC LIMIT 1",
                )?;
                let mut rows = stmt.query([])?;
                rows.next()?
                    .map(|row| {
                        Ok::<(i64, Option<String>), rusqlite::Error>((row.get(0)?, row.get(1)?))
                    })
                    .transpose()?
            };

            if let Some(row) = row {
                break Ok::<(i64, Option<String>), anyhow::Error>(row);
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    assert_eq!(recorded.0, 0, "failure row should set success=0");
    let metadata = recorded.1.expect("failure row should include metadata");
    assert!(
        metadata.contains("error.message"),
        "failure metadata should include error.message"
    );

    let _ = service.cancel().await;
    Ok(())
}
