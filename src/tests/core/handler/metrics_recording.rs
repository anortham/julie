use super::*;
use crate::tests::helpers::workspace::mark_workspace_root;

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
