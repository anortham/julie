use super::mark_index_ready;
use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::tests::helpers::mcp::{
    answer_next_list_roots_request, call_tool_result_text as extract_text_from_result,
};
use crate::tools::search::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, NumberOrString},
    service::{RequestContext, serve_directly},
};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_primary_uses_rebound_session_primary() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::workspace::registry::generate_workspace_id;
    use std::sync::Arc;

    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(&original_root)?;
    fs::create_dir_all(&rebound_root)?;
    fs::write(
        original_root.join("main.rs"),
        "fn original_workspace_only() { println!(\"original_only_marker\"); }\n",
    )?;
    fs::write(
        rebound_root.join("lib.rs"),
        "fn rebound_workspace_only() { println!(\"rebound_only_marker\"); }\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
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
    let seed_handler = JulieServerHandler::new_with_shared_workspace(
        rebound_ws,
        rebound_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(rebound_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(rebound_path_str.clone()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&seed_handler)
    .await?;

    handler.set_current_primary_binding(rebound_id.clone(), rebound_path);
    mark_index_ready(&handler).await;

    let search_tool = FastSearchTool {
        query: "rebound_only_marker".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search_tool.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    assert!(
        response_text.contains("rebound_only_marker"),
        "primary line-mode search should use rebound session primary: {}",
        response_text
    );
    assert!(
        !response_text.contains("original_only_marker"),
        "primary line-mode search should not read stale loaded primary content: {}",
        response_text
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_reference_db_cache_tracks_primary_root_changes() -> Result<()> {
    use crate::database::SymbolDatabase;
    use crate::database::types::FileInfo;

    let temp_dir = TempDir::new()?;
    let first_root = temp_dir.path().join("first-root");
    let second_root = temp_dir.path().join("second-root");
    fs::create_dir_all(first_root.join(".git"))?;
    fs::create_dir_all(second_root.join(".git"))?;

    let handler = JulieServerHandler::new(first_root.clone()).await?;
    handler
        .initialize_workspace_with_force(Some(first_root.to_string_lossy().to_string()), true)
        .await?;

    let ref_id = "shared-ref";
    let first_db_path = first_root
        .join(".julie")
        .join("indexes")
        .join(ref_id)
        .join("db")
        .join("symbols.db");
    fs::create_dir_all(first_db_path.parent().expect("first db parent"))?;
    let mut first_db = SymbolDatabase::new(&first_db_path)?;
    let first_file = FileInfo {
        path: "a.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash-a".to_string(),
        size: 1,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 1,
        line_count: 1,
        content: Some("fn alpha() {}".to_string()),
    };
    let first_symbol = Symbol {
        id: "alpha-id".to_string(),
        name: "alpha".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "a.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 12,
        start_byte: 0,
        end_byte: 12,
        signature: None,
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
    first_db.bulk_store_fresh_atomic(&[first_file], &[first_symbol], &[], &[], &[], ref_id)?;

    let first_db_handle = handler.get_database_for_workspace(ref_id).await?;
    let first_count = first_db_handle
        .lock()
        .unwrap()
        .count_symbols_for_workspace()?;
    assert_eq!(first_count, 1);

    handler
        .initialize_workspace_with_force(Some(second_root.to_string_lossy().to_string()), true)
        .await?;

    let second_db_path = second_root
        .join(".julie")
        .join("indexes")
        .join(ref_id)
        .join("db")
        .join("symbols.db");
    fs::create_dir_all(second_db_path.parent().expect("second db parent"))?;
    let mut second_db = SymbolDatabase::new(&second_db_path)?;
    let second_file = FileInfo {
        path: "b.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash-b".to_string(),
        size: 1,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 2,
        line_count: 2,
        content: Some("fn beta() {}\nfn gamma() {}".to_string()),
    };
    let second_symbols = vec![
        Symbol {
            id: "beta-id".to_string(),
            name: "beta".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "b.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 11,
            start_byte: 0,
            end_byte: 11,
            signature: None,
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
        },
        Symbol {
            id: "gamma-id".to_string(),
            name: "gamma".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "b.rs".to_string(),
            start_line: 2,
            start_column: 0,
            end_line: 2,
            end_column: 12,
            start_byte: 13,
            end_byte: 25,
            signature: None,
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
        },
    ];
    second_db.bulk_store_fresh_atomic(&[second_file], &second_symbols, &[], &[], &[], ref_id)?;

    let second_db_handle = handler.get_database_for_workspace(ref_id).await?;
    let second_count = second_db_handle
        .lock()
        .unwrap()
        .count_symbols_for_workspace()?;
    assert_eq!(
        second_count, 2,
        "reference db cache should follow the new primary root anchor instead of reusing the old cached handle"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_reference_indexing_uses_rebound_primary_storage_root() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let first_root = temp_dir.path().join("first-root");
    let second_root = temp_dir.path().join("second-root");
    let reference_root = temp_dir.path().join("reference-root");
    fs::create_dir_all(first_root.join(".git"))?;
    fs::create_dir_all(second_root.join(".git"))?;
    fs::create_dir_all(&reference_root)?;
    fs::write(first_root.join("main.rs"), "fn first_root() {}\n")?;
    fs::write(second_root.join("main.rs"), "fn second_root() {}\n")?;
    fs::write(reference_root.join("ref.rs"), "fn reference_symbol() {}\n")?;

    let handler = JulieServerHandler::new(first_root.clone()).await?;
    handler
        .initialize_workspace_with_force(Some(first_root.to_string_lossy().to_string()), true)
        .await?;
    handler
        .initialize_workspace_with_force(Some(second_root.to_string_lossy().to_string()), true)
        .await?;

    let reference_path = reference_root.canonicalize()?;
    let reference_id =
        crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(reference_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let second_db_path = second_root
        .join(".julie")
        .join("indexes")
        .join(&reference_id)
        .join("db")
        .join("symbols.db");
    let first_db_path = first_root
        .join(".julie")
        .join("indexes")
        .join(&reference_id)
        .join("db")
        .join("symbols.db");

    assert!(
        second_db_path.exists(),
        "reference indexing should land under the rebound primary storage root"
    );
    assert!(
        !first_db_path.exists(),
        "reference indexing should not write under the stale loaded primary storage root"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_primary_rejects_neutral_gap_without_primary_identity() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("example.rs"),
        "pub fn neutral_gap_search_target() {}\n",
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    handler.publish_loaded_workspace_swap_intent_for_test();

    let search_tool = FastSearchTool {
        query: "neutral_gap_search_target".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let err = search_tool
        .call_tool(&handler)
        .await
        .expect_err("neutral gap should reject primary fast_search requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_primary_cold_start_reports_index_first_instead_of_swap_gap() -> Result<()>
{
    let handler = JulieServerHandler::new_for_test().await?;

    let search_tool = FastSearchTool {
        query: "cold_start_primary_search_target".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search_tool.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    assert!(
        response_text.contains(
            "Workspace not indexed yet. Run manage_workspace(operation=\"index\") first."
        ),
        "cold-start primary search should preserve the normal index-first guidance: {response_text}"
    );
    assert!(
        !response_text.contains("Primary workspace identity unavailable during swap"),
        "cold-start primary search should not be classified as a swap gap: {response_text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_primary_rejects_swap_in_progress_after_partial_publish() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let rebound_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let rebound_path = rebound_dir.path().canonicalize()?;
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("example.rs"),
        "pub fn partial_publish_search_target() {}\n",
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let rebound_id =
        crate::workspace::registry::generate_workspace_id(&rebound_path.to_string_lossy())?;

    handler.publish_loaded_workspace_swap_intent_for_test();
    handler.set_current_primary_binding(rebound_id, rebound_path);

    let search_tool = FastSearchTool {
        query: "partial_publish_search_target".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let err = search_tool
        .call_tool(&handler)
        .await
        .expect_err("swap-in-progress should reject primary fast_search after partial publish");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );

    Ok(())
}

// Post-T8: this test exercised the legacy line_mode `-term` exclusion
// syntax against a file with no symbols (comment-only content).  The
// unified path indexes only symbol-bearing files and only searches
// symbol fields + file path-text, so a comment-only fixture with a
// `-term` query has no path through FastSearchTool's default flow
// anymore.  The exclusion syntax remains a property of the line_mode
// utility (still reachable via `return_format=locations` once content
// matches exist) and is covered by `line_match_strategy_tests`.

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_primary_wrapper_resolves_roots_before_searching() -> Result<()> {
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
        "pub fn rebound_search_symbol() {}\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
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
            hash: "roots-search-hash".to_string(),
            size: 1,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: Some("pub fn rebound_search_symbol() {}\n".to_string()),
        };
        let symbol = Symbol {
            id: "roots-search-symbol-id".to_string(),
            name: "rebound_search_symbol".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/rebound.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 31,
            start_byte: 0,
            end_byte: 31,
            signature: Some("fn rebound_search_symbol()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("pub fn rebound_search_symbol() {}".to_string()),
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
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

    let roots = [roots_path.as_path()];
    let roots_reply = answer_next_list_roots_request(&mut lines, &mut write_half, &roots);

    let search = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("fast_search").with_arguments(
            serde_json::json!({
                "query": "rebound_search_symbol",
                "workspace": "primary",
                "limit": 10
            })
            .as_object()
            .expect("fast_search args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(31), service.peer().clone()),
    );
    let (_, result) = tokio::join!(roots_reply, search);
    let response_text = extract_text_from_result(&result?);

    // Post-T8: the unified search no longer emits target-specific
    // missing-index or content-zero-hit messages.  Accept any of the
    // neutral variants (no-results, neutral missing-index, or the
    // explicit "No results found" copy from execute_with_trace).
    assert!(
        response_text.contains("No lines found matching")
            || response_text.contains("0 content matches for")
            || response_text.contains("No results found for:")
            || response_text
                .contains("Search requires a Tantivy index for the current primary workspace"),
        "fast_search should resolve roots first and produce a normal roots-bound search response: {response_text}"
    );
    assert_eq!(handler.current_workspace_id(), Some(roots_id));

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}
