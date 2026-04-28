use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use rmcp::ServerHandler;
use rmcp::model::{CallToolRequestParams, NumberOrString};
use rmcp::service::{RequestContext, serve_directly};
use serde_json::Value;
use tempfile::TempDir;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::database::types::FileInfo;
use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;

fn json_object(value: Value) -> rmcp::model::JsonObject {
    value
        .as_object()
        .expect("test arguments should be a JSON object")
        .clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn test_get_context_target_workspace_uses_requested_binding_for_metrics_attribution()
-> Result<()> {
    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    std::fs::create_dir_all(primary_root.join("src"))?;
    std::fs::create_dir_all(target_root.join("src"))?;

    let file_path = "src/context_target.rs";
    let primary_content = "pub fn primary_context_only() {}\n";
    let target_content = "/// target context phrase\npub fn target_context_symbol() {\n    target_context_helper();\n}\n\npub fn target_context_helper() {}\n";
    std::fs::write(primary_root.join(file_path), primary_content)?;
    std::fs::write(target_root.join(file_path), target_content)?;

    let primary_bytes = primary_content.len() as i64;
    let target_bytes = target_content.len() as i64;
    assert_ne!(
        primary_bytes, target_bytes,
        "byte counts must differ so wrong telemetry attribution is visible"
    );

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize()?;
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str)?;
    daemon_db.upsert_workspace(&primary_id, &primary_path_str, "ready")?;
    let primary_ws = pool.get_or_init(&primary_id, primary_path.clone()).await?;
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

    let seed_ws = pool.get_or_init(&target_id, target_path.clone()).await?;
    let seed_handler = JulieServerHandler::new_with_shared_workspace(
        seed_ws,
        target_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(target_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(target_path_str),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&seed_handler)
    .await?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let request =
        CallToolRequestParams::new("get_context").with_arguments(json_object(serde_json::json!({
            "query": "target context phrase",
            "workspace": target_id.clone(),
            "language": "rust",
            "format": "readable",
            "max_tokens": 1200
        })));

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        request,
        RequestContext::new(NumberOrString::Number(3), service.peer().clone()),
    )
    .await?;
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("target_context_symbol"),
        "get_context should return data from the requested workspace: {result_text}"
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
        "get_context telemetry should record the requested workspace id"
    );
    assert_eq!(
        recorded.1,
        Some(target_bytes),
        "get_context telemetry should attribute source bytes to the requested workspace"
    );
    assert_eq!(
        handler.session_metrics.total_source_bytes(),
        target_bytes as u64,
        "get_context should count source bytes from the requested workspace"
    );

    let _ = service.cancel().await;
    Ok(())
}
