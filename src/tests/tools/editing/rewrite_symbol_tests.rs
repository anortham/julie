use anyhow::Result;
use futures::poll;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::workspace::ManageWorkspaceTool;

fn extract_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| {
            serde_json::to_value(block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(|text| text.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn setup_indexed_workspace(content: &str) -> Result<(TempDir, JulieServerHandler, String)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    let file_path = src_dir.join("test.rs");
    fs::write(&file_path, content)?;

    let handler = JulieServerHandler::new(workspace_path.clone()).await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(workspace_path.to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    Ok((temp_dir, handler, "src/test.rs".to_string()))
}

async fn setup_indexed_workspace_with_files(
    files: &[(&str, &str)],
) -> Result<(TempDir, JulieServerHandler)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    for (relative_path, content) in files {
        let absolute_path = workspace_path.join(relative_path);
        if let Some(parent) = absolute_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&absolute_path, content)?;
    }

    let handler = JulieServerHandler::new(workspace_path.clone()).await?;
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(workspace_path.to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    Ok((temp_dir, handler))
}

fn read_workspace_file(temp_dir: &TempDir, relative_path: &str) -> Result<String> {
    Ok(fs::read_to_string(temp_dir.path().join(relative_path))?)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_replace_body_via_live_parse() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n\npub fn farewell() {\n    println!(\"goodbye\");\n}\n";
    let (temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "{\n    println!(\"hi there\");\n}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);

    assert!(
        !text.contains("Error:"),
        "Expected successful apply, got: {text}"
    );
    assert!(
        text.contains("Applied replace_body"),
        "Expected replace_body success message, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("src").join("test.rs"))?;
    assert!(
        on_disk.contains("println!(\"hi there\")"),
        "Function body should be updated, got: {on_disk}"
    );
    assert!(
        on_disk.contains("pub fn greet()"),
        "Signature should be preserved, got: {on_disk}"
    );
    assert!(
        on_disk.contains("fn farewell()"),
        "Other symbols should be preserved, got: {on_disk}"
    );
    assert!(
        !on_disk.contains("println!(\"hello\")"),
        "Old body should be gone, got: {on_disk}"
    );

    Ok(())
}

#[test]
fn test_rewrite_symbol_rejects_unknown_field() {
    let json = serde_json::json!({
        "symbol": "foo",
        "operation": "replace_full",
        "content": "fn foo() {}",
        "bogus": true,
    });
    let result: Result<crate::tools::editing::rewrite_symbol::RewriteSymbolTool, _> =
        serde_json::from_value(json);
    let err = result.expect_err("unknown field should be rejected");
    assert!(
        err.to_string().contains("bogus"),
        "error should mention offending field, got: {err}"
    );
}

#[test]
fn test_rewrite_symbol_defaults_to_dry_run_and_primary_workspace() {
    let json = serde_json::json!({
        "symbol": "foo",
        "operation": "replace_full",
        "content": "fn foo() {}",
    });
    let tool: crate::tools::editing::rewrite_symbol::RewriteSymbolTool =
        serde_json::from_value(json).expect("tool should deserialize");
    assert!(tool.dry_run, "dry_run should default to true");
    assert_eq!(tool.workspace.as_deref(), Some("primary"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_replace_full_applies_live_symbol_span() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n\npub fn farewell() {\n    println!(\"goodbye\");\n}\n";
    let (temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_full".to_string(),
        content: "pub fn greet(name: &str) {\n    println!(\"hello {name}\");\n}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("Applied replace_full"),
        "Expected success message, got: {text}"
    );

    let on_disk = read_workspace_file(&temp_dir, "src/test.rs")?;
    assert!(
        on_disk.contains("hello {name}"),
        "Full replacement should be applied, got: {on_disk}"
    );
    assert!(
        on_disk.contains("fn farewell()"),
        "Other symbol should remain, got: {on_disk}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_replace_signature_preserves_body() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n";
    let (temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_signature".to_string(),
        content: "pub fn greet(name: &str)".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    tool.call_tool(&handler).await?;

    let on_disk = read_workspace_file(&temp_dir, "src/test.rs")?;
    assert!(
        on_disk.contains("pub fn greet(name: &str) {"),
        "Signature should be rewritten, got: {on_disk}"
    );
    assert!(
        on_disk.contains("println!(\"hello\")"),
        "Body should remain, got: {on_disk}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_insert_before_dry_run_leaves_file_unchanged() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n";
    let (temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "insert_before".to_string(),
        content: "const MAGIC: i32 = 7;".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: true,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("Dry run preview"),
        "Expected preview output, got: {text}"
    );
    assert!(
        text.contains("const MAGIC"),
        "Preview should include inserted content, got: {text}"
    );

    let on_disk = read_workspace_file(&temp_dir, "src/test.rs")?;
    assert_eq!(on_disk, source, "dry run should not modify file");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_insert_after_applies_below_symbol() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n\npub fn farewell() {}\n";
    let (temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "insert_after".to_string(),
        content: "pub fn helper() {\n    println!(\"helper\");\n}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    tool.call_tool(&handler).await?;

    let on_disk = read_workspace_file(&temp_dir, "src/test.rs")?;
    let greet_pos = on_disk.find("pub fn greet").expect("greet should exist");
    let helper_pos = on_disk.find("pub fn helper").expect("helper should exist");
    let farewell_pos = on_disk
        .find("pub fn farewell")
        .expect("farewell should exist");
    assert!(
        greet_pos < helper_pos && helper_pos < farewell_pos,
        "helper should be inserted after greet and before farewell, got: {on_disk}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_add_doc_inserts_comment_above_symbol() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n";
    let (temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "add_doc".to_string(),
        content: "/// Greets the caller.".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    tool.call_tool(&handler).await?;

    let on_disk = read_workspace_file(&temp_dir, "src/test.rs")?;
    let doc_pos = on_disk
        .find("/// Greets the caller.")
        .expect("doc should exist");
    let fn_pos = on_disk.find("pub fn greet").expect("function should exist");
    assert!(
        doc_pos < fn_pos,
        "doc should appear before function, got: {on_disk}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_rejects_existing_doc_comment() -> Result<()> {
    let source = "/// Existing docs.\npub fn greet() {\n    println!(\"hello\");\n}\n";
    let (_temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "add_doc".to_string(),
        content: "/// New docs.".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("already has documentation"),
        "Expected doc rejection, got: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_rejects_stale_index() -> Result<()> {
    let source = "pub fn stable() {\n    println!(\"before\");\n}\n";
    let (temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

    fs::write(
        temp_dir.path().join("src").join("test.rs"),
        "pub fn stable() {\n    println!(\"mutated\");\n}\n",
    )?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "stable".to_string(),
        operation: "replace_body".to_string(),
        content: "{\n    println!(\"after\");\n}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("changed since last indexing"),
        "Expected stale-index rejection, got: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_not_found() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n";
    let (_temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "ghost".to_string(),
        operation: "replace_full".to_string(),
        content: "pub fn ghost() {}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("not found"),
        "Expected not-found error, got: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_requires_file_path_for_ambiguous_match() -> Result<()> {
    let files = [
        (
            "src/alpha.rs",
            "pub fn collide() {\n    println!(\"alpha\");\n}\n",
        ),
        (
            "src/beta.rs",
            "pub fn collide() {\n    println!(\"beta\");\n}\n",
        ),
    ];
    let (_temp_dir, handler) = setup_indexed_workspace_with_files(&files).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "collide".to_string(),
        operation: "replace_full".to_string(),
        content: "pub fn collide() {}".to_string(),
        file_path: None,
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("Provide file_path or symbol@line"),
        "Expected ambiguity error, got: {text}"
    );
    assert!(
        text.contains("src/alpha.rs") && text.contains("src/beta.rs"),
        "Ambiguity error should list both files, got: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rewrite_symbol_uses_current_primary_db_after_rebind() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    let original_ws = pool
        .get_or_init(&original_id, original_path.clone())
        .await?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    let rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;
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
async fn test_rewrite_symbol_keeps_primary_binding_snapshot_across_swap_window() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
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
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;
    pool.get_or_init(&rebound_id, rebound_path.clone()).await?;

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

// Task 1: explicit errors for unsupported ops

#[tokio::test(flavor = "multi_thread")]
async fn test_replace_signature_no_body_returns_explicit_error() -> Result<()> {
    // Rust trait method declarations parse as function_signature_item (no body field).
    // replace_signature must return an explicit error, not silently clobber the whole symbol.
    let source = "pub trait Greetable {\n    fn greet(&self) -> String;\n}\n";
    let (temp_dir, handler, _) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_signature".to_string(),
        content: "fn greet(&self, name: &str) -> String".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("replace_signature is not supported"),
        "Expected explicit error for trait method with no body, got: {text}"
    );
    assert!(
        text.contains("greet"),
        "Error should name the symbol, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("src").join("test.rs"))?;
    assert_eq!(
        on_disk, source,
        "File must be unchanged after replace_signature error"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_replace_body_error_lists_actual_field_names() -> Result<()> {
    // replace_body on a trait method declaration should error and list the actual
    // field names the node has (e.g., name, parameters, return_type) — not 'body'.
    let source = "pub trait Greetable {\n    fn greet(&self) -> String;\n}\n";
    let (temp_dir, handler, _) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "{ String::from(\"hello\") }".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("node has fields:"),
        "Error should list actual node field names, got: {text}"
    );
    assert!(
        text.contains("no 'body' field"),
        "Error should mention the missing 'body' field, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("src").join("test.rs"))?;
    assert_eq!(
        on_disk, source,
        "File must be unchanged after replace_body error"
    );

    Ok(())
}

// Task 2: no-op detection

#[tokio::test(flavor = "multi_thread")]
async fn test_replace_signature_noop_returns_info_message() -> Result<()> {
    let source = "pub fn greet(name: &str) -> String {\n    format!(\"hello {name}\")\n}\n";
    let (_temp_dir, handler, _) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_signature".to_string(),
        content: "pub fn greet(name: &str) -> String".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("No changes:"),
        "Expected no-op info message, got: {text}"
    );
    assert!(
        text.contains("greet"),
        "No-op message should name the symbol, got: {text}"
    );
    assert!(
        !text.contains("Applied"),
        "No-op should not claim to have applied anything, got: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_replace_body_noop_returns_info_message() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n";
    let (_temp_dir, handler, _) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "{\n    println!(\"hello\");\n}".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("No changes:"),
        "Expected no-op info message for replace_body with identical body, got: {text}"
    );

    Ok(())
}

// Task 3: dry-run span header

#[tokio::test(flavor = "multi_thread")]
async fn test_dry_run_replace_body_shows_old_content_with_braces() -> Result<()> {
    // The span header must show the old content so callers can see the braces
    // are part of the replaced span and must be included in their 'content'.
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

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("--- Old content ---"),
        "Dry-run preview should include old content header, got: {text}"
    );
    assert!(
        text.contains('{') && text.contains('}'),
        "Old content section should show enclosing braces, got: {text}"
    );
    assert!(
        text.contains("Replacing"),
        "Dry-run should show span replacement header, got: {text}"
    );
    assert!(
        text.contains("bytes"),
        "Replacement header should show byte range, got: {text}"
    );
    assert!(
        text.contains("lines"),
        "Replacement header should show line range, got: {text}"
    );
    assert!(
        text.contains("--- Diff ---"),
        "Dry-run should include diff section header, got: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_dry_run_add_doc_shows_anchor_no_old_content() -> Result<()> {
    let source = "pub fn greet() {\n    println!(\"hello\");\n}\n";
    let (_temp_dir, handler, _) = setup_indexed_workspace(source).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "add_doc".to_string(),
        content: "/// Greets the user.".to_string(),
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: true,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("Inserting at byte"),
        "Insert dry-run should report anchor byte position, got: {text}"
    );
    assert!(
        !text.contains("--- Old content ---"),
        "Insert dry-run must not include old content section, got: {text}"
    );
    assert!(
        text.contains("--- Diff ---"),
        "Insert dry-run should include diff section header, got: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_dry_run_long_body_elision() -> Result<()> {
    // A body longer than 30 lines should show first 15 + last 5, rest elided.
    let mut lines = vec!["pub fn long_fn() {".to_string()];
    for i in 0..35u32 {
        lines.push(format!("    let _x{i} = {i};"));
    }
    lines.push("}".to_string());
    let source = lines.join("\n") + "\n";

    let (_temp_dir, handler, _) = setup_indexed_workspace(&source).await?;

    let new_body_lines: Vec<String> = (0..35u32)
        .map(|i| format!("    let _x{i} = {};", i + 100))
        .collect();
    let new_body = format!("{{\n{}\n}}", new_body_lines.join("\n"));

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "long_fn".to_string(),
        operation: "replace_body".to_string(),
        content: new_body,
        file_path: Some("src/test.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: true,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("lines elided"),
        "Long body should be elided in dry-run preview, got: {text}"
    );

    Ok(())
}

/// Regression test: file_path filter must use suffix semantics, not substring.
/// `handler.rs` must match `src/tools/handler.rs` but NOT `src/tools/foohandler.rs`.
#[tokio::test]
async fn test_rewrite_symbol_file_path_filter_uses_suffix_not_substring() -> Result<()> {
    let files = [
        (
            "src/tools/handler.rs",
            "pub fn process() {\n    println!(\"handler\");\n}\n",
        ),
        (
            "src/tools/foohandler.rs",
            "pub fn process() {\n    println!(\"foohandler\");\n}\n",
        ),
    ];
    let (_temp_dir, handler) = setup_indexed_workspace_with_files(&files).await?;

    // With file_path="handler.rs", only src/tools/handler.rs should match.
    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "process".to_string(),
        operation: "replace_full".to_string(),
        content: "pub fn process() { println!(\"updated\"); }".to_string(),
        file_path: Some("handler.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: true,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);

    // Must resolve to the exact-suffix file, not produce an ambiguity error or wrong-file match.
    assert!(
        !text.starts_with("Error:"),
        "Expected successful resolution to handler.rs, got: {text}"
    );
    assert!(
        text.contains("src/tools/handler.rs"),
        "Diff must reference src/tools/handler.rs, got: {text}"
    );
    assert!(
        !text.contains("foohandler.rs"),
        "foohandler.rs must not appear in the result, got: {text}"
    );

    Ok(())
}

/// Regression test: a bogus file_path filter that is not a valid suffix of any indexed path
/// must return a not-found error, not a false positive from substring matching.
#[tokio::test]
async fn test_rewrite_symbol_file_path_bogus_filter_returns_not_found() -> Result<()> {
    let files = [(
        "src/tools/handler.rs",
        "pub fn process() {\n    println!(\"handler\");\n}\n",
    )];
    let (_temp_dir, handler) = setup_indexed_workspace_with_files(&files).await?;

    // "andler.rs" is a substring of "handler.rs" but NOT a valid path suffix (no leading `/`).
    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "process".to_string(),
        operation: "replace_full".to_string(),
        content: "pub fn process() {}".to_string(),
        file_path: Some("andler.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: true,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);

    assert!(
        text.contains("Error:") && text.contains("not found"),
        "Expected not-found error for bogus suffix filter, got: {text}"
    );

    Ok(())
}
