//! Tests for get_symbols tool with relative Unix-style path storage
//!
//! These tests verify that get_symbols correctly handles relative paths
//! after Phase 2 implementation (relative Unix-style path storage).

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::symbols::GetSymbolsTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, NumberOrString, ServerJsonRpcMessage, ServerRequest},
    service::{RequestContext, serve_directly},
};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn send_json_line(writer: &mut (impl AsyncWriteExt + Unpin), value: &serde_json::Value) {
    writer
        .write_all(serde_json::to_string(value).unwrap().as_bytes())
        .await
        .unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
}

async fn read_server_message(
    lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
) -> ServerJsonRpcMessage {
    let line = lines
        .next_line()
        .await
        .unwrap()
        .expect("server should emit a JSON-RPC message line");
    serde_json::from_str(&line).unwrap()
}

/// Test that get_symbols can find symbols when given a relative path
///
/// After Phase 2, database stores relative Unix-style paths like "src/main.rs"
/// The tool should accept relative paths and find symbols correctly.
#[tokio::test]
async fn test_get_symbols_with_relative_path() -> Result<()> {
    // Setup: Create temp workspace with a Rust file
    let temp_dir = TempDir::new()?;
    let src_dir = temp_dir.path().join("src");
    fs::create_dir(&src_dir)?;

    let test_file = src_dir.join("test_unique_file.rs");
    fs::write(
        &test_file,
        r#"
        pub fn get_user_data(id: u32) -> String {
            format!("User {}", id)
        }

        pub struct UserService {
            pub name: String,
        }
    "#,
    )?;

    // Initialize workspace and index
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // TEST: Call get_symbols with RELATIVE path (Phase 2 format)
    let tool = GetSymbolsTool {
        file_path: "src/test_unique_file.rs".to_string(), // RELATIVE, not absolute!
        max_depth: 1,
        mode: None,
        limit: None,
        target: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    // ASSERTION: Should find symbols (currently fails)
    assert!(
        result_text.contains("get_user_data") || result_text.contains("UserService"),
        "Should find symbols with relative path input, got: {}",
        result_text
    );

    // Verify no "No symbols found" message
    assert!(
        !result_text.contains("No symbols found"),
        "Should not return 'No symbols found' for valid relative path"
    );

    Ok(())
}

/// Test that get_symbols can find symbols when given an absolute path
///
/// Even with relative storage, the tool should accept absolute paths
/// and convert them to relative before querying.
#[tokio::test]
async fn test_get_symbols_with_absolute_path() -> Result<()> {
    // Setup: Create temp workspace with a Rust file
    let temp_dir = TempDir::new()?;
    let src_dir = temp_dir.path().join("src");
    fs::create_dir(&src_dir)?;

    let test_file = src_dir.join("test_unique_lib.rs");
    fs::write(
        &test_file,
        r#"
        pub fn calculate_score(points: i32) -> i32 {
            points * 2
        }
    "#,
    )?;

    // Initialize workspace and index
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // TEST: Call get_symbols with ABSOLUTE path
    let absolute_path = test_file.to_string_lossy().to_string();
    let tool = GetSymbolsTool {
        file_path: absolute_path.clone(),
        max_depth: 1,
        mode: None,
        limit: None,
        target: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    // ASSERTION: Should find symbols (currently fails)
    assert!(
        result_text.contains("calculate_score"),
        "Should find symbols with absolute path input (converted to relative), got: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_get_symbols_relative_path_uses_rebound_current_primary_root() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::workspace::registry::generate_workspace_id;
    use std::sync::Arc;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        original_root.join("src").join("old.rs"),
        "fn old_root_only() {}\n",
    )?;
    fs::write(
        rebound_root.join("src").join("rebound.rs"),
        "pub fn rebound_symbol() {}\n",
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

    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    let rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;
    {
        let rebound_db = rebound_ws.db.as_ref().unwrap().clone();
        let mut rebound_db = rebound_db.lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "src/rebound.rs".to_string(),
            language: "rust".to_string(),
            hash: "rebound-hash".to_string(),
            size: 1,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: Some("pub fn rebound_symbol() {}\n".to_string()),
        };
        let symbol = crate::extractors::Symbol {
            id: "rebound-symbol-id".to_string(),
            name: "rebound_symbol".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/rebound.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 24,
            start_byte: 0,
            end_byte: 24,
            signature: Some("pub fn rebound_symbol()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        };
        rebound_db.bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &rebound_id)?;
    }

    handler.set_current_primary_binding(rebound_id.clone(), rebound_path.clone());

    let tool = GetSymbolsTool {
        file_path: "src/rebound.rs".to_string(),
        max_depth: 1,
        mode: None,
        limit: None,
        target: None,
        workspace: Some(rebound_id),
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("rebound_symbol"),
        "relative get_symbols should resolve against rebound current primary root, not stale loaded root: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_get_symbols_primary_uses_rebound_current_primary_root() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::workspace::registry::generate_workspace_id;
    use std::sync::Arc;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        original_root.join("src").join("old.rs"),
        "fn old_root_only() {}\n",
    )?;
    fs::write(
        rebound_root.join("src").join("rebound.rs"),
        "pub fn rebound_primary_symbol() {}\n",
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

    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    let rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;
    {
        let rebound_db = rebound_ws.db.as_ref().unwrap().clone();
        let mut rebound_db = rebound_db.lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "src/rebound.rs".to_string(),
            language: "rust".to_string(),
            hash: "rebound-primary-hash".to_string(),
            size: 1,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: Some("pub fn rebound_primary_symbol() {}\n".to_string()),
        };
        let symbol = crate::extractors::Symbol {
            id: "rebound-primary-symbol-id".to_string(),
            name: "rebound_primary_symbol".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/rebound.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 32,
            start_byte: 0,
            end_byte: 32,
            signature: Some("pub fn rebound_primary_symbol()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        };
        rebound_db.bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &rebound_id)?;
    }

    handler.set_current_primary_binding(rebound_id.clone(), rebound_path.clone());

    let tool = GetSymbolsTool {
        file_path: "src/rebound.rs".to_string(),
        max_depth: 1,
        mode: None,
        limit: None,
        target: None,
        workspace: Some("primary".to_string()),
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("rebound_primary_symbol"),
        "primary get_symbols should resolve against rebound current primary root, not stale loaded root: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_get_symbols_primary_wrapper_resolves_roots_before_reading() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let startup_root = temp_dir.path().join("startup-primary");
    let roots_root = temp_dir.path().join("roots-primary");
    fs::create_dir_all(startup_root.join("src"))?;
    fs::create_dir_all(roots_root.join("src"))?;
    fs::write(startup_root.join("src/old.rs"), "fn old_root_only() {}\n")?;
    fs::write(
        roots_root.join("src/rebound.rs"),
        "pub fn rebound_primary_symbol() {}\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let startup_path = startup_root.canonicalize()?;
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_ws = pool.get_or_init(&startup_id, startup_path.clone()).await?;

    let roots_path = roots_root.canonicalize()?;
    let roots_id = generate_workspace_id(&roots_path.to_string_lossy())?;
    daemon_db.upsert_workspace(&startup_id, &startup_path.to_string_lossy(), "ready")?;
    daemon_db.upsert_workspace(&roots_id, &roots_path.to_string_lossy(), "ready")?;
    let roots_ws = pool.get_or_init(&roots_id, roots_path.clone()).await?;
    {
        let rebound_db = roots_ws.db.as_ref().unwrap().clone();
        let mut rebound_db = rebound_db.lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "src/rebound.rs".to_string(),
            language: "rust".to_string(),
            hash: "roots-wrapper-hash".to_string(),
            size: 1,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: Some("pub fn rebound_primary_symbol() {}\n".to_string()),
        };
        let symbol = crate::extractors::Symbol {
            id: "roots-wrapper-symbol-id".to_string(),
            name: "rebound_primary_symbol".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/rebound.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 32,
            start_byte: 0,
            end_byte: 32,
            signature: Some("fn rebound_primary_symbol()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("pub fn rebound_primary_symbol() {}".to_string()),
            content_type: None,
        };
        rebound_db.bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &roots_id)?;
    }

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;
    handler.set_client_supports_workspace_roots_for_test(true);

    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let roots_reply = async {
        match read_server_message(&mut lines).await {
            ServerJsonRpcMessage::Request(request) => match request.request {
                ServerRequest::ListRootsRequest(_) => {
                    send_json_line(
                        &mut write_half,
                        &serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request.id,
                            "result": {
                                "roots": [{ "uri": format!("file://{}", roots_path.to_string_lossy()) }]
                            }
                        }),
                    )
                    .await;
                }
                other => panic!("unexpected server request: {other:?}"),
            },
            other => panic!("unexpected server message: {other:?}"),
        }
    };

    let get_symbols = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("get_symbols").with_arguments(
            serde_json::json!({
                "file_path": "src/rebound.rs",
                "workspace": "primary",
                "max_depth": 1,
                "mode": "structure",
                "limit": 10
            })
            .as_object()
            .expect("get_symbols args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(21), service.peer().clone()),
    );
    let (_, result) = tokio::join!(roots_reply, get_symbols);
    let text = extract_text_from_result(&result?);

    assert!(
        text.contains("rebound_primary_symbol"),
        "get_symbols should read from roots-bound primary: {text}"
    );
    assert_eq!(handler.current_workspace_id(), Some(roots_id));

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

/// Test that database actually stores relative Unix-style paths
///
/// This is the foundation test - verify Phase 2 storage is working.
#[tokio::test]
async fn test_database_stores_relative_unix_paths() -> Result<()> {
    // Setup: Create temp workspace
    let temp_dir = TempDir::new()?;
    let src_dir = temp_dir.path().join("src");
    let tools_dir = src_dir.join("tools");
    fs::create_dir_all(&tools_dir)?;

    let test_file = tools_dir.join("search.rs");
    fs::write(
        &test_file,
        r#"
        pub fn search_code() {}
    "#,
    )?;

    // Initialize and index
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // Get database and query directly
    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace should exist");
    let db = workspace.db.as_ref().expect("Database should exist");
    let db_lock = db.lock().unwrap();

    // Query all symbols
    let all_symbols = db_lock.get_all_symbols()?;

    // ASSERTION: Paths should be relative Unix-style
    let paths: Vec<String> = all_symbols.iter().map(|s| s.file_path.clone()).collect();

    // Should have at least one symbol from our test file
    assert!(!paths.is_empty(), "Should have indexed symbols");

    // Check that paths are relative (don't start with /)
    for path in &paths {
        assert!(
            !path.starts_with('/'),
            "Path should be relative, not absolute: {}",
            path
        );

        assert!(
            !path.contains('\\'),
            "Path should use Unix-style separators, not backslashes: {}",
            path
        );
    }

    // Should find our specific file with relative path
    let search_file_symbols: Vec<_> = all_symbols
        .iter()
        .filter(|s| s.file_path == "src/tools/search.rs")
        .collect();

    assert!(
        !search_file_symbols.is_empty(),
        "Should find symbols with relative path 'src/tools/search.rs'"
    );

    Ok(())
}
