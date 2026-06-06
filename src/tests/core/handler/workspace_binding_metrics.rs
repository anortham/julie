use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_record_tool_call_uses_binding_snapshot_for_metrics_attribution() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
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

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;
    let original_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize_with_index_root(
            original_path.clone(),
            indexes_dir.join(&original_id),
        )
        .await?,
    );
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
    let mut handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
        Some(dashboard_tx),
    )
    .await?;
    // Pin the shared index root so workspace_index_dir_for resolves under indexes_dir.
    handler.in_process_index_root = Some(indexes_dir.join(&original_id));

    let binding_snapshot = handler.require_primary_workspace_binding()?;
    handler.set_current_primary_binding(rebound_id.clone(), rebound_path);

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

    let recorded_daemon_source_bytes: Option<i64> = {
        let conn = daemon_db.conn_for_test();
        conn.query_row(
            "SELECT source_bytes FROM tool_calls ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
    };
    assert_eq!(
        recorded_daemon_source_bytes,
        Some(source_bytes as i64),
        "daemon metrics row should preserve source_bytes from the snapshotted workspace db"
    );

    let recorded_local_source_bytes: Option<i64> = {
        let conn = Connection::open(indexes_dir.join(&original_id).join("db").join("symbols.db"))?;
        conn.query_row(
            "SELECT source_bytes FROM tool_calls ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
    };
    assert_eq!(
        recorded_local_source_bytes,
        Some(source_bytes as i64),
        "local workspace metrics row should write source_bytes from the snapshotted workspace db"
    );
    assert_eq!(
        handler.session_metrics.total_source_bytes(),
        source_bytes,
        "session metrics should include source_bytes resolved from the snapshotted workspace db"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_metrics_workspace_binding_uses_target_workspace_param() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::tools::navigation::resolution::WorkspaceTarget;
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    std::fs::create_dir_all(&primary_root)?;
    std::fs::create_dir_all(&target_root)?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);

    let primary_path = primary_root.canonicalize()?;
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str)?;
    daemon_db.upsert_workspace(&primary_id, &primary_path_str, "ready")?;
    let primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(primary_path.clone())
            .await?);

    let target_path = target_root.canonicalize()?;
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str)?;
    daemon_db.upsert_workspace(&target_id, &target_path_str, "ready")?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
    )
    .await?;

    let binding = handler
        .metrics_workspace_binding_for_target(&WorkspaceTarget::Target(target_id.clone()))
        .await
        .expect("target workspace binding should resolve");

    assert_eq!(binding.workspace_id, target_id);
    assert_eq!(binding.workspace_root, target_path);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_refs_target_workspace_uses_requested_binding_for_metrics_attribution()
-> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::extractors::{Symbol, SymbolKind};
    use crate::workspace::registry::generate_workspace_id;
    use std::time::Duration;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    std::fs::create_dir_all(primary_root.join("src"))?;
    std::fs::create_dir_all(target_root.join("src"))?;

    let file_path = "src/target.rs";
    let primary_content = "pub fn primary_only() {}\n";
    let target_content = "pub fn target_symbol() {}\n\npub fn target_helper() {}\n";
    std::fs::write(primary_root.join(file_path), primary_content)?;
    std::fs::write(target_root.join(file_path), target_content)?;

    let primary_bytes = primary_content.len() as i64;
    let target_bytes = target_content.len() as i64;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);

    let primary_path = primary_root.canonicalize()?;
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str)?;
    daemon_db.upsert_workspace(&primary_id, &primary_path_str, "ready")?;
    let primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize_with_index_root(
            primary_path.clone(),
            indexes_dir.join(&primary_id),
        )
        .await?);
    {
        let primary_db = primary_ws
            .db
            .as_ref()
            .expect("primary workspace should have a database")
            .clone();
        let primary_db = primary_db.lock().unwrap();
        primary_db.store_file_info(&FileInfo {
            path: file_path.to_string(),
            language: "rust".to_string(),
            hash: "primary-hash".to_string(),
            size: primary_bytes,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 0,
            line_count: 1,
            content: Some(primary_content.to_string()),
        })?;
    }

    let target_path = target_root.canonicalize()?;
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str)?;
    daemon_db.upsert_workspace(&target_id, &target_path_str, "ready")?;
    let target_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize_with_index_root(
            target_path.clone(),
            indexes_dir.join(&target_id),
        )
        .await?);
    {
        let target_db = target_ws
            .db
            .as_ref()
            .expect("target workspace should have a database")
            .clone();
        let mut target_db = target_db.lock().unwrap();
        let symbol = Symbol {
            id: "target-symbol-id".to_string(),
            name: "target_symbol".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 24,
            start_byte: 0,
            end_byte: 24,
            signature: Some("pub fn target_symbol()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        };
        target_db.bulk_store_fresh_atomic(
            &[FileInfo {
                path: file_path.to_string(),
                language: "rust".to_string(),
                hash: "target-hash".to_string(),
                size: target_bytes,
                last_modified: 1,
                last_indexed: 1,
                symbol_count: 1,
                line_count: 3,
                content: Some(target_content.to_string()),
            }],
            &[symbol],
            &[],
            &[],
            &[],
            &target_id,
        )?;
    }

    let mut handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
    )
    .await?;
    // Pin the shared index root so workspace_index_dir_for(target_id) resolves
    // to indexes_dir/{target_id} rather than primary_path/.julie/indexes/{target_id}.
    handler.in_process_index_root = Some(indexes_dir.join(&primary_id));

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let request =
        CallToolRequestParams::new("fast_refs").with_arguments(json_object(serde_json::json!({
            "symbol": "target_symbol",
            "include_definition": true,
            "limit": 10,
            "workspace": target_id.clone(),
        })));

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        request,
        RequestContext::new(NumberOrString::Number(3), service.peer().clone()),
    )
    .await?;

    assert!(
        !result.content.is_empty(),
        "fast_refs should return a tool response"
    );

    let recorded = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let row = {
                let conn = daemon_db.conn_for_test();
                conn.query_row(
                    "SELECT workspace_id, source_bytes FROM tool_calls ORDER BY id DESC LIMIT 1",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
            };

            match row {
                Ok(values) => break Ok::<_, rusqlite::Error>(values),
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    tokio::task::yield_now().await;
                }
                Err(err) => break Err(err),
            }
        }
    })
    .await??;

    assert_eq!(
        recorded.0, target_id,
        "fast_refs telemetry should record the requested workspace id"
    );
    // source_bytes are looked up in the loaded primary workspace DB (task.workspace),
    // not the target workspace DB. This is the in-process behavior post WorkspacePool
    // removal: the metrics writer only has access to the primary workspace's DB.
    assert_eq!(
        recorded.1,
        Some(primary_bytes),
        "fast_refs source_bytes are resolved from the loaded primary workspace db"
    );
    assert_eq!(
        handler.session_metrics.total_source_bytes(),
        primary_bytes as u64,
        "fast_refs session source_bytes are resolved from the loaded primary workspace db"
    );

    let _ = service.cancel().await;
    Ok(())
}
