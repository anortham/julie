use super::{ensure_primary_projection_current, mark_index_ready};
use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::tests::helpers::mcp::call_tool_result_text as extract_text_from_result;
use crate::tools::search::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_primary_uses_rebound_session_primary() -> Result<()> {
    use crate::registry::database::DaemonDatabase;
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
    )
    .await?;

    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    let rebound_ws =
        Arc::new(crate::workspace::JulieWorkspace::initialize(rebound_path.clone()).await?);
    let seed_handler = JulieServerHandler::new_with_shared_workspace(
        rebound_ws,
        rebound_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(rebound_id.clone()),
        None,
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
    ensure_primary_projection_current(&handler).await;

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
    use crate::database::FactsStore;
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
    let mut first_db = FactsStore::new(&first_db_path)?;
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
        extracted: julie_extractors::Symbol {
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
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        code_context: None,
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
    let mut second_db = FactsStore::new(&second_db_path)?;
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
            extracted: julie_extractors::Symbol {
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
                content_type: None,
                body_span: None,
                body_hash: None,
                annotations: Vec::new(),
            },
            code_context: None,
        },
        Symbol {
            extracted: julie_extractors::Symbol {
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
                content_type: None,
                body_span: None,
                body_hash: None,
                annotations: Vec::new(),
            },
            code_context: None,
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
async fn test_fast_search_primary_cold_start_reports_index_first_instead_of_swap_gap() -> Result<()>
{
    let handler = julie_test_support::FakeToolContext::new()
        .with_system_status(julie_core::health_types::SystemStatus::NotReady);

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

// Post-T8: this test exercised the legacy line_mode `-term` exclusion
// syntax against a file with no symbols (comment-only content).  The
// unified path indexes only symbol-bearing files and only searches
// symbol fields + file path-text, so a comment-only fixture with a
// `-term` query has no path through FastSearchTool's default flow
// anymore.  The exclusion syntax remains a property of the line_mode
// utility (still reachable via `return_format=locations` once content
// matches exist) and is covered by `line_match_strategy_tests`.
