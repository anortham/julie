use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn test_record_tool_call_uses_binding_snapshot_for_metrics_attribution() -> Result<()> {
    use crate::registry::database::DaemonDatabase;
    use crate::workspace::registry::generate_workspace_id;
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
    let source_contents = "fn original() {}\n";
    let source_bytes = source_contents.len() as u64;
    std::fs::write(original_root.join(&source_file_rel), source_contents)?;
    {
        let guard = julie_core::workspace::mutation_gate::acquire_gate(&original_id).await;
        original_ws.store.apply(
            &[julie_index::checkout_store::PathChange::Upsert {
                path: source_file_rel.clone(),
                bytes: source_contents.as_bytes().to_vec(),
                language: "rust".to_string(),
            }],
            &guard,
        )?;
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
            if daemon_count > 0 {
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
        "daemon metrics row should preserve source_bytes from the snapshotted checkout"
    );

    assert_eq!(
        handler.session_metrics.total_source_bytes(),
        source_bytes,
        "session metrics should include source_bytes resolved from the snapshotted checkout"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_metrics_workspace_binding_uses_target_workspace_param() -> Result<()> {
    use crate::registry::database::DaemonDatabase;
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
    let primary_ws =
        Arc::new(crate::workspace::JulieWorkspace::initialize(primary_path.clone()).await?);

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
    use crate::extractors::SymbolKind;
    use crate::registry::database::DaemonDatabase;
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
        .await?,
    );
    {
        let guard = julie_core::workspace::mutation_gate::acquire_gate(&primary_id).await;
        primary_ws.store.apply(
            &[julie_index::checkout_store::PathChange::Upsert {
                path: file_path.to_string(),
                bytes: primary_content.as_bytes().to_vec(),
                language: "rust".to_string(),
            }],
            &guard,
        )?;
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
        .await?,
    );
    {
        let guard = julie_core::workspace::mutation_gate::acquire_gate(&target_id).await;
        target_ws.store.apply(
            &[julie_index::checkout_store::PathChange::Upsert {
                path: file_path.to_string(),
                bytes: target_content.as_bytes().to_vec(),
                language: "rust".to_string(),
            }],
            &guard,
        )?;
    }

    let mut handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
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
        !result.as_result().content.is_empty(),
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
    assert_eq!(
        recorded.1,
        Some(target_bytes),
        "fast_refs source_bytes should be resolved from the requested target workspace db"
    );
    assert_eq!(
        handler.session_metrics.total_source_bytes(),
        target_bytes as u64,
        "fast_refs session source_bytes should be resolved from the requested target workspace db"
    );

    let _ = service.cancel().await;
    Ok(())
}
