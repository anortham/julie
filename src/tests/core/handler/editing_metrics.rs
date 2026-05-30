use super::*;
use crate::tests::helpers::workspace::mark_workspace_root;

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_metrics_attribute_root_file_source_bytes() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;
    use std::time::Duration;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    let cargo_toml = temp_dir.path().join("Cargo.toml");
    let original = "[package]\nname = \"before\"\nversion = \"0.1.0\"\n";
    std::fs::write(&cargo_toml, original)?;

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
        !result.content.is_empty(),
        "edit_file should return a tool response"
    );

    let db_arc = {
        let workspace = handler.workspace.read().await;
        workspace
            .as_ref()
            .and_then(|workspace| workspace.db.as_ref())
            .expect("indexed workspace should have a database")
            .clone()
    };
    let source_bytes = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(summary) = {
                let db = db_arc.lock().expect("workspace db should lock");
                db.query_session_summary(&handler.session_metrics.session_id)?
                    .into_iter()
                    .find(|summary| summary.tool_name == "edit_file")
            } {
                break Ok::<u64, anyhow::Error>(summary.total_source_bytes);
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

    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
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
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
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
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
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
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
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

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_metrics_include_symbol_span_and_failure_kind() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    std::fs::create_dir_all(temp_dir.path().join("src"))?;
    std::fs::write(
        temp_dir.path().join("src/lib.rs"),
        "pub fn target() { println!(\"old\"); }\npub fn collide() {}\npub fn collide() {}\n",
    )?;
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
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
        "rewrite_symbol",
        serde_json::json!({
            "symbol": "target",
            "operation": "replace_full",
            "content": "pub fn target() { println!(\"new\"); }",
            "dry_run": true
        }),
        2103,
    )
    .await?;
    assert!(succeeded, "rewrite_symbol dry run should succeed");

    let (success, metadata) = latest_tool_metric(&handler, "rewrite_symbol").await?;
    assert_eq!(success, 1);
    assert_eq!(metadata["kind"], "rewrite_symbol");
    assert_eq!(metadata["dry_run"], true);
    assert_eq!(metadata["applied"], false);
    assert_eq!(metadata["operation"], "replace_full");
    assert_eq!(metadata["symbol"], "target");
    assert_eq!(metadata["match_count"], 1);
    assert!(metadata["input_bytes"].as_u64().unwrap() > 0);
    assert!(metadata["file_size_bytes"].as_u64().unwrap() > 0);
    assert!(metadata["symbol_span_bytes"].as_u64().unwrap() > 0);
    assert!(metadata["content_bytes"].as_u64().unwrap() > 0);
    assert!(metadata["diff_bytes"].as_u64().unwrap() > 0);
    assert!(metadata["changed_bytes"].as_u64().unwrap() > 0);

    let succeeded = call_public_tool(
        &handler,
        "rewrite_symbol",
        serde_json::json!({
            "symbol": "collide",
            "operation": "replace_full",
            "content": "pub fn collide() {}",
            "dry_run": true
        }),
        2105,
    )
    .await?;
    assert!(!succeeded, "ambiguous rewrite_symbol should fail");

    let (success, metadata) = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let row = latest_tool_metric(&handler, "rewrite_symbol").await?;
            if row.1["symbol"] == "collide" {
                break Ok::<(i64, serde_json::Value), anyhow::Error>(row);
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    assert_eq!(success, 0);
    assert_eq!(metadata["kind"], "rewrite_symbol");
    assert_eq!(metadata["operation"], "replace_full");
    assert_eq!(metadata["symbol"], "collide");
    assert_eq!(metadata["failure_kind"], "ambiguous_symbol");
    assert_eq!(metadata["applied"], false);
    assert!(metadata["content_bytes"].as_u64().unwrap() > 0);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_failed_apply_metrics_record_applied_false() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    std::fs::create_dir_all(temp_dir.path().join("src"))?;
    let file_path = temp_dir.path().join("src/lib.rs");
    std::fs::write(&file_path, "pub fn target() { println!(\"old\"); }\n")?;
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
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
        "rewrite_symbol",
        serde_json::json!({
            "symbol": "target",
            "operation": "replace_full",
            "content": "pub fn target() { println!(\"new\"); }",
            "dry_run": false
        }),
        2107,
    )
    .await?;
    set_readonly(&file_path, false)?;

    assert!(!succeeded, "readonly rewrite_symbol apply should fail");
    assert_eq!(
        std::fs::read_to_string(&file_path)?,
        "pub fn target() { println!(\"old\"); }\n"
    );

    let (success, metadata) = latest_tool_metric(&handler, "rewrite_symbol").await?;
    assert_eq!(success, 0);
    assert_eq!(metadata["dry_run"], false);
    assert_eq!(metadata["applied"], false);
    assert_eq!(metadata["failure_kind"], "execution_error");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rename_symbol_metrics_include_reference_and_change_counts() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;

    let temp_dir = TempDir::new()?;
    mark_workspace_root(temp_dir.path());
    std::fs::write(
        temp_dir.path().join("main.rs"),
        "fn getUserData() { getUserData(); }\n",
    )?;
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
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
        "rename_symbol",
        serde_json::json!({
            "old_name": "getUserData",
            "new_name": "fetchUserData",
            "dry_run": true
        }),
        2104,
    )
    .await?;
    assert!(succeeded, "rename_symbol dry run should succeed");

    let (success, metadata) = latest_tool_metric(&handler, "rename_symbol").await?;
    assert_eq!(success, 1);
    assert_eq!(metadata["kind"], "rename_symbol");
    assert_eq!(metadata["dry_run"], true);
    assert_eq!(metadata["scope"], "workspace");
    assert!(metadata["reference_count"].as_u64().unwrap() > 0);
    assert_eq!(metadata["changed_file_count"], 1);
    assert!(metadata["changed_line_count"].as_u64().unwrap() > 0);

    Ok(())
}
