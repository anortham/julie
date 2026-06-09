use super::{mark_index_ready, setup_loaded_primary_without_tantivy};
use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::tests::helpers::mcp::call_tool_result_text as extract_text_from_result;
use crate::tools::search::FastSearchTool;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_reports_index_requirement_for_reference_without_tantivy()
-> Result<()> {
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
    let reference_root = temp_dir.path().join("reference-workspace");
    fs::create_dir_all(&original_root)?;
    fs::create_dir_all(&reference_root)?;
    fs::write(
        original_root.join("main.rs"),
        "fn original_workspace_only() {}\n",
    )?;
    fs::write(
        reference_root.join("lib.rs"),
        "fn reference_missing_tantivy() { println!(\"reference_missing_tantivy\"); }\n",
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

    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

    let reference_path = reference_root.canonicalize()?;
    let reference_path_str = reference_path.to_string_lossy().to_string();
    let reference_id = generate_workspace_id(&reference_path_str)?;
    daemon_db.upsert_workspace(&reference_id, &reference_path_str, "ready")?;

    let reference_db_path = handler
        .get_workspace()
        .await?
        .expect("primary workspace should exist")
        .workspace_db_path(&reference_id);
    fs::create_dir_all(
        reference_db_path
            .parent()
            .expect("reference db parent should exist"),
    )?;
    let _reference_db = crate::database::SymbolDatabase::new(&reference_db_path)?;

    handler.mark_workspace_active(&reference_id).await;
    mark_index_ready(&handler).await;

    let search_tool = FastSearchTool {
        query: "reference_missing_tantivy".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some(reference_id.clone()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search_tool.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    // Post-T8: the unified search no longer differentiates target-specific
    // missing-index messages.  All callers see the neutral wording.
    assert!(
        response_text.contains(&format!(
            "Search requires a Tantivy index for workspace '{}'",
            reference_id
        )),
        "reference search should return a clear readiness message when Tantivy is missing: {response_text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_definitions_reports_index_requirement_for_reference_without_tantivy()
-> Result<()> {
    use crate::registry::database::DaemonDatabase;
    use crate::workspace::registry::generate_workspace_id;
    use std::sync::Arc;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let reference_root = temp_dir.path().join("reference-workspace");
    fs::create_dir_all(&original_root)?;
    fs::create_dir_all(&reference_root)?;
    fs::write(
        original_root.join("main.rs"),
        "fn original_workspace_only() {}\n",
    )?;
    fs::write(
        reference_root.join("lib.rs"),
        "fn definition_missing_tantivy() {}\n",
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

    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

    let reference_path = reference_root.canonicalize()?;
    let reference_path_str = reference_path.to_string_lossy().to_string();
    let reference_id = generate_workspace_id(&reference_path_str)?;
    daemon_db.upsert_workspace(&reference_id, &reference_path_str, "ready")?;

    let reference_db_path = handler.workspace_db_file_path_for(&reference_id).await?;
    fs::create_dir_all(
        reference_db_path
            .parent()
            .expect("reference db parent should exist"),
    )?;
    let mut reference_db = crate::database::SymbolDatabase::new(&reference_db_path)?;
    let reference_file = crate::database::types::FileInfo {
        path: "lib.rs".to_string(),
        language: "rust".to_string(),
        hash: "ref-hash".to_string(),
        size: 1,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 1,
        line_count: 1,
        content: Some("fn definition_missing_tantivy() {}\n".to_string()),
    };
    let reference_symbol = Symbol {
        id: "definition-missing-id".to_string(),
        name: "definition_missing_tantivy".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "lib.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 31,
        start_byte: 0,
        end_byte: 31,
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
    reference_db.bulk_store_fresh_atomic(
        &[reference_file],
        &[reference_symbol],
        &[],
        &[],
        &[],
        &reference_id,
    )?;

    handler.mark_workspace_active(&reference_id).await;
    mark_index_ready(&handler).await;

    let search_tool = FastSearchTool {
        query: "definition_missing_tantivy".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some(reference_id.clone()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search_tool.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    // Post-T8: the unified search no longer differentiates target-specific
    // missing-index messages.  All callers see the neutral wording.
    assert!(
        response_text.contains(&format!(
            "Search requires a Tantivy index for workspace '{}'",
            reference_id
        )),
        "search should return a clear readiness message when Tantivy is missing: {response_text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_reports_index_requirement_for_loaded_primary_without_tantivy()
-> Result<()> {
    let (_temp_dir, handler) = setup_loaded_primary_without_tantivy().await?;

    let search_tool = FastSearchTool {
        query: "loaded_primary_missing_tantivy".to_string(),
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

    // Post-T8: the unified search no longer differentiates target-specific
    // missing-index messages.  All callers see the neutral wording.
    assert!(
        response_text.contains("Search requires a Tantivy index for the current primary workspace"),
        "loaded primary search should return an explicit Tantivy-required message: {response_text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_definitions_reports_index_requirement_for_loaded_primary_without_tantivy()
-> Result<()> {
    let (_temp_dir, handler) = setup_loaded_primary_without_tantivy().await?;

    let search_tool = FastSearchTool {
        query: "loaded_primary_missing_tantivy".to_string(),
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

    // Post-T8: the unified search no longer differentiates target-specific
    // missing-index messages.  All callers see the neutral wording.
    assert!(
        response_text.contains("Search requires a Tantivy index for the current primary workspace"),
        "loaded primary search should return an explicit Tantivy-required message: {response_text}"
    );

    Ok(())
}
