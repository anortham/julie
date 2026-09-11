use super::*;
use crate::tests::helpers::workspace::mark_workspace_root;
use rusqlite::OptionalExtension;

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_metrics_attribute_root_file_source_bytes() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;
    use std::time::Duration;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    let cargo_toml = temp_dir.path().join("Cargo.toml");
    let original = "[package]\nname = \"before\"\nversion = \"0.1.0\"\n";
    std::fs::write(&cargo_toml, original)?;

    let mut handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    attach_daemon_db(&mut handler, temp_dir.path())?;
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
        CallToolRequestParams::new("edit_file").with_arguments(json_object(serde_json::json!({
            "file_path": "Cargo.toml",
            "old_text": "name = \"before\"",
            "new_text": "name = \"after\"",
            "dry_run": false
        })));
    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        request,
        RequestContext::new(NumberOrString::Number(2), service.peer().clone()),
    )
    .await?;
    assert!(
        !result.as_result().content.is_empty(),
        "edit_file should return a tool response"
    );

    let source_bytes = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let bytes: Option<i64> = {
                let conn = handler
                    .daemon_db
                    .as_ref()
                    .expect("registry.db")
                    .conn_for_test();
                conn.query_row(
                    "SELECT source_bytes FROM tool_calls
                     WHERE tool_name = 'edit_file'
                     ORDER BY id DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .optional()?
            };
            if let Some(n) = bytes {
                break Ok::<u64, anyhow::Error>(n as u64);
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    assert!(
        source_bytes > 0,
        "edit_file metrics should attribute source bytes for root-level files"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_validation_errors_are_recorded_as_failures() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    std::fs::write(temp_dir.path().join("README.md"), "hello\n")?;

    let mut handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    attach_daemon_db(&mut handler, temp_dir.path())?;
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
        CallToolRequestParams::new("edit_file").with_arguments(json_object(serde_json::json!({
            "file_path": "README.md",
            "old_text": "",
            "new_text": "bye",
            "dry_run": true
        })));
    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        request,
        RequestContext::new(NumberOrString::Number(2002), service.peer().clone()),
    )
    .await;
    assert!(
        result.is_err(),
        "edit_file validation failure should be returned as an MCP error"
    );

    let (success_flag, metadata) = latest_tool_metric(&handler, "edit_file").await?;
    assert_eq!(
        success_flag, 0,
        "validation error should record a failed metrics row"
    );
    assert_eq!(metadata["kind"], "edit_file");
    assert_eq!(metadata["failure_kind"], "validation");
    assert_eq!(metadata["file"], "README.md");
    assert_eq!(metadata["target"]["target_file_path"], "README.md");
    assert!(metadata["input_bytes"].as_u64().unwrap() > 0);

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_empty_old_text_validation_precedes_file_io() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    std::fs::write(temp_dir.path().join("README.md"), "hello\n")?;

    let mut handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    attach_daemon_db(&mut handler, temp_dir.path())?;
    ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    }
    .call_tool(&handler)
    .await?;

    let succeeded = call_public_tool(
        &handler,
        "edit_file",
        serde_json::json!({
            "file_path": "missing.md",
            "old_text": "",
            "new_text": "bye",
            "dry_run": true
        }),
        2003,
    )
    .await?;
    assert!(!succeeded, "empty old_text should fail validation");

    let (success_flag, metadata) = latest_tool_metric(&handler, "edit_file").await?;
    assert_eq!(success_flag, 0);
    assert_eq!(metadata["failure_kind"], "validation");
    assert_eq!(metadata["applied"], false);
    assert_eq!(metadata["file"], "missing.md");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_metrics_include_input_and_edit_outcome() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    std::fs::write(temp_dir.path().join("README.md"), "hello\n")?;
    let mut handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    attach_daemon_db(&mut handler, temp_dir.path())?;
    ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    }
    .call_tool(&handler)
    .await?;

    let succeeded = call_public_tool(
        &handler,
        "edit_file",
        serde_json::json!({
            "file_path": "README.md",
            "old_text": "hello",
            "new_text": "goodbye",
            "dry_run": true
        }),
        2101,
    )
    .await?;
    assert!(succeeded, "edit_file dry run should succeed");

    let (success, metadata) = latest_tool_metric(&handler, "edit_file").await?;
    assert_eq!(success, 1);
    assert_eq!(metadata["kind"], "edit_file");
    assert_eq!(metadata["dry_run"], true);
    assert_eq!(metadata["applied"], false);
    assert!(metadata["input_bytes"].as_u64().unwrap() > 0);
    assert_eq!(metadata["file_size_bytes"], 6);
    assert_eq!(metadata["old_text_bytes"], 5);
    assert_eq!(metadata["new_text_bytes"], 7);
    assert!(metadata["diff_bytes"].as_u64().unwrap() > 0);
    assert!(metadata["changed_bytes"].as_u64().unwrap() > 0);
    assert_eq!(metadata["occurrence"], "first");
    assert_eq!(metadata["match_mode"], "exact");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_apply_metrics_record_conversion_outcome() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    let file_path = temp_dir.path().join("README.md");
    std::fs::write(&file_path, "hello\n")?;
    let mut handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    attach_daemon_db(&mut handler, temp_dir.path())?;
    ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    }
    .call_tool(&handler)
    .await?;

    let succeeded = call_public_tool(
        &handler,
        "edit_file",
        serde_json::json!({
            "file_path": "README.md",
            "old_text": "hello",
            "new_text": "goodbye",
            "dry_run": false
        }),
        2102,
    )
    .await?;
    assert!(succeeded, "edit_file apply should succeed");
    assert_eq!(std::fs::read_to_string(file_path)?, "goodbye\n");

    let (success, metadata) = latest_tool_metric(&handler, "edit_file").await?;
    assert_eq!(success, 1);
    assert_eq!(metadata["dry_run"], false);
    assert_eq!(metadata["applied"], true);
    assert!(metadata["changed_bytes"].as_u64().unwrap() > 0);
    assert!(metadata["diff_bytes"].as_u64().unwrap() > 0);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_failed_apply_metrics_record_applied_false() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    let file_path = temp_dir.path().join("README.md");
    std::fs::write(&file_path, "hello\n")?;
    let mut handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    attach_daemon_db(&mut handler, temp_dir.path())?;
    ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    }
    .call_tool(&handler)
    .await?;

    set_readonly(&file_path, true)?;
    let succeeded = call_public_tool(
        &handler,
        "edit_file",
        serde_json::json!({
            "file_path": "README.md",
            "old_text": "hello",
            "new_text": "goodbye",
            "dry_run": false
        }),
        2106,
    )
    .await?;
    set_readonly(&file_path, false)?;

    assert!(!succeeded, "readonly edit_file apply should fail");
    assert_eq!(std::fs::read_to_string(&file_path)?, "hello\n");

    let (success, metadata) = latest_tool_metric(&handler, "edit_file").await?;
    assert_eq!(success, 0);
    assert_eq!(metadata["dry_run"], false);
    assert_eq!(metadata["applied"], false);
    assert_eq!(metadata["failure_kind"], "execution_error");

    Ok(())
}
