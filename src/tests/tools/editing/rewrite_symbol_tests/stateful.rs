use super::*;
use futures::poll;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_uses_current_primary_db_after_rebind() -> Result<()> {
    use crate::registry::database::DaemonDatabase;
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        original_root.join("src").join("test.rs"),
        "pub fn original_only() { println!(\"original\"); }\n",
    )?;
    fs::write(
        rebound_root.join("src").join("test.rs"),
        "pub fn rebound_target() { println!(\"before\"); }\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    let original_ws =
        Arc::new(crate::workspace::JulieWorkspace::initialize(original_path.clone()).await?);

    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
    )
    .await?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    let rebound_ws =
        Arc::new(crate::workspace::JulieWorkspace::initialize(rebound_path.clone()).await?);
    {
        let rebound_file_path = rebound_root.join("src").join("test.rs");
        let rebound_hash = crate::database::calculate_file_hash(&rebound_file_path)?;
        let rebound_db = rebound_ws
            .db
            .as_ref()
            .expect("rebound workspace db should exist")
            .clone();
        let mut rebound_db = rebound_db.lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "src/test.rs".to_string(),
            language: "rust".to_string(),
            hash: rebound_hash,
            size: 64,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: Some("pub fn rebound_target() { println!(\"before\"); }\n".to_string()),
        };
        let symbol = crate::extractors::Symbol {
            id: "rebound_symbol".to_string(),
            name: "rebound_target".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/test.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 24,
            start_byte: 0,
            end_byte: 24,
            signature: Some("pub fn rebound_target()".to_string()),
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
        rebound_db.bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &rebound_id)?;
    }

    handler.set_current_primary_binding(rebound_id, rebound_path.clone());

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "rebound_target".to_string(),
        operation: "replace_full".to_string(),
        content: "pub fn rebound_target() {\n    println!(\"after\");\n}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("Applied replace_full"),
        "rewrite_symbol after current-primary rebind should use rebound DB/index: {text}"
    );

    let rebound_on_disk = fs::read_to_string(rebound_root.join("src").join("test.rs"))?;
    assert!(rebound_on_disk.contains("after"));
    let original_on_disk = fs::read_to_string(original_root.join("src").join("test.rs"))?;
    assert!(original_on_disk.contains("original"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "daemon multi-workspace session/roots lifecycle (pool-backed); reworked in Phase 3d.3 registry rework"]
async fn test_rewrite_symbol_keeps_primary_binding_snapshot_across_swap_window() -> Result<()> {
    use crate::registry::database::DaemonDatabase;
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        original_root.join("src").join("test.rs"),
        "pub fn original_target() { println!(\"before\"); }\n",
    )?;
    fs::write(
        rebound_root.join("src").join("test.rs"),
        "pub fn rebound_target() { println!(\"rebound\"); }\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;
    let original_ws =
        Arc::new(crate::workspace::JulieWorkspace::initialize(original_path.clone()).await?);
    {
        let original_file_path = original_root.join("src").join("test.rs");
        let original_hash = crate::database::calculate_file_hash(&original_file_path)?;
        let original_db = original_ws
            .db
            .as_ref()
            .expect("original workspace db should exist")
            .clone();
        let mut original_db = original_db.lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "src/test.rs".to_string(),
            language: "rust".to_string(),
            hash: original_hash,
            size: 64,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: Some("pub fn original_target() { println!(\"before\"); }\n".to_string()),
        };
        let symbol = crate::extractors::Symbol {
            id: "original_symbol".to_string(),
            name: "original_target".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/test.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 25,
            start_byte: 0,
            end_byte: 25,
            signature: Some("pub fn original_target()".to_string()),
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
        original_db.bulk_store_fresh_atomic(
            &[file_info],
            &[symbol],
            &[],
            &[],
            &[],
            &original_id,
        )?;
    }

    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
    )
    .await?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;
    Arc::new(crate::workspace::JulieWorkspace::initialize(rebound_path.clone()).await?);

    let workspace_write_guard = handler.workspace.write().await;
    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "original_target".to_string(),
        operation: "replace_full".to_string(),
        content: "pub fn original_target() {\n    println!(\"after\");\n}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };
    let mut edit_future = Box::pin(tool.call_tool(&handler));
    assert!(
        poll!(edit_future.as_mut()).is_pending(),
        "rewrite_symbol should block on the first await while the workspace lock is held"
    );

    handler.set_current_primary_binding(rebound_id, rebound_path.clone());
    drop(workspace_write_guard);

    let result = edit_future.await?;
    let text = extract_text(&result);
    assert!(
        text.contains("Applied replace_full"),
        "snapshot-bound edit should still succeed across the swap window: {text}"
    );

    let original_on_disk = fs::read_to_string(original_root.join("src").join("test.rs"))?;
    assert!(
        original_on_disk.contains("after"),
        "edit should apply to the original root selected at call start"
    );

    let rebound_on_disk = fs::read_to_string(rebound_root.join("src").join("test.rs"))?;
    assert!(
        rebound_on_disk.contains("rebound"),
        "swap-window edit must not leak into the rebound root"
    );

    Ok(())
}

#[tokio::test]
async fn test_prepared_rewrite_drives_metrics_and_dry_run_output() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n";
    let (_temp_dir, handler, _) = setup_indexed_workspace(source).await?;
    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "{\n    println!(\"hi there\");\n}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: true,
    };

    let prepared = tool.prepare_rewrite(&handler).await?;
    let metadata = tool.success_metrics_metadata_from_prepared(&prepared);
    assert_eq!(metadata["file_path"], "src/test.rs");
    assert_eq!(metadata["file_size_bytes"], source.len());
    assert_eq!(metadata["match_count"], 1);
    assert_eq!(metadata["applied"], false);
    assert!(metadata["symbol_span_bytes"].as_u64().unwrap() > 0);
    assert!(metadata["diff_bytes"].as_u64().unwrap() > 0);
    assert!(metadata["changed_bytes"].as_u64().unwrap() > 0);

    let prepared_result = tool.call_prepared(prepared)?;
    let direct_result = tool.call_tool(&handler).await?;

    assert_eq!(extract_text(&prepared_result), extract_text(&direct_result));

    Ok(())
}

#[tokio::test]
async fn test_prepared_rewrite_rejects_changed_target_before_commit() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n";
    let intervening =
        "pub fn greet() {\n    println!(\"hello\");\n    println!(\"external\");\n}\n";
    let (temp_dir, handler, _) = setup_indexed_workspace(source).await?;
    let file_path = temp_dir.path().join("src").join("test.rs");
    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "{\n    println!(\"hi there\");\n}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let prepared = tool.prepare_rewrite(&handler).await?;
    fs::write(&file_path, intervening)?;
    let err = tool
        .call_prepared(prepared)
        .expect_err("prepared apply must reject a file changed after preparation")
        .to_string();

    assert!(
        err.contains("File changed during edit"),
        "error should explain the stale target, got: {err}"
    );
    assert_eq!(
        fs::read_to_string(&file_path)?,
        intervening,
        "intervening content must be preserved"
    );

    Ok(())
}
