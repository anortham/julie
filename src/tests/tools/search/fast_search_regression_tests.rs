use anyhow::Result;
use std::fs;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::search::index::{SearchFilter, SearchIndex};
use crate::tools::search::FastSearchTool;
use crate::tools::search::text_search::definition_search_with_index_for_test;
use crate::tools::workspace::ManageWorkspaceTool;

fn extract_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content| content.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn mark_search_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

async fn index_workspace(workspace_path: &std::path::Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    mark_search_ready(&handler).await;
    Ok(handler)
}

fn rescued_symbol() -> Symbol {
    Symbol {
        id: "qualified-router".to_string(),
        name: "Phoenix.Router".to_string(),
        kind: SymbolKind::Module,
        language: "elixir".to_string(),
        file_path: "lib/phoenix/router.ex".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 3,
        end_column: 0,
        start_byte: 0,
        end_byte: 64,
        signature: Some("defmodule Phoenix.Router do".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some("defmodule Phoenix.Router do\nend".to_string()),
        content_type: None,
        annotations: Vec::new(),
    }
}

#[test]
fn fast_search_deserializes_limit_with_public_bounds() {
    let low: FastSearchTool =
        serde_json::from_str(r#"{"query":"needle","limit":0}"#).expect("low limit should parse");
    assert_eq!(low.limit, 1);

    let high: FastSearchTool =
        serde_json::from_str(r#"{"query":"needle","limit":501}"#).expect("high limit should parse");
    assert_eq!(high.limit, 500);
}

#[test]
fn sqlite_rescue_counts_rescued_hits_in_pre_trunc_total() -> Result<()> {
    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("symbols.db");
    let mut db = SymbolDatabase::new(&db_path)?;
    db.store_file_info(&FileInfo {
        path: "lib/phoenix/router.ex".to_string(),
        language: "elixir".to_string(),
        hash: "hash".to_string(),
        size: 64,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 1,
        line_count: 2,
        content: Some("defmodule Phoenix.Router do\nend".to_string()),
    })?;
    db.store_symbols(&[rescued_symbol()])?;

    let index_dir = TempDir::new()?;
    let index = SearchIndex::create(index_dir.path())?;
    index.commit()?;

    let filter = SearchFilter {
        language: Some("elixir".to_string()),
        kind: None,
        file_pattern: None,
        exclude_tests: false,
    };
    let (symbols, _relaxed, total) =
        definition_search_with_index_for_test("Router", &filter, 5, &index, Some(&db))?;

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Phoenix.Router");
    assert_eq!(total, 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn content_locations_format_omits_matching_line_text() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("app.rs"),
        "fn main() {\n    let compact_location_marker = 1;\n}\n",
    )?;

    let handler = index_workspace(workspace_path).await?;
    let result = FastSearchTool {
        query: "compact_location_marker".to_string(),
        search_target: "content".to_string(),
        return_format: "locations".to_string(),
        limit: 10,
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(
        text.contains("src/app.rs:2"),
        "locations output should include file and line, got:\n{text}"
    );
    assert!(
        !text.contains("let compact_location_marker"),
        "locations output must omit line snippets, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn definition_search_with_zero_limit_still_returns_one_result() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("lib.rs"),
        "pub fn zero_limit_should_still_find_one() {}\n",
    )?;

    let handler = index_workspace(workspace_path).await?;
    let result = FastSearchTool {
        query: "zero_limit_should_still_find_one".to_string(),
        search_target: "definitions".to_string(),
        return_format: "locations".to_string(),
        limit: 0,
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(
        text.contains("src/lib.rs:1"),
        "limit=0 should clamp to one result, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn file_search_missing_index_names_file_mode() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("main.rs"), "fn main() {}\n")?;

    let handler = index_workspace(workspace_path).await?;
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())?;
    let tantivy_dir = handler.workspace_tantivy_dir_for(&workspace_id).await?;
    let meta_path = tantivy_dir.join("meta.json");
    if meta_path.exists() {
        fs::remove_file(meta_path)?;
    }

    let result = FastSearchTool {
        query: "main.rs".to_string(),
        search_target: "files".to_string(),
        context_lines: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(
        text.contains("File search requires a Tantivy index"),
        "file mode should name file search in the missing-index message, got:\n{text}"
    );
    assert!(
        !text.contains("Definition search requires"),
        "file mode should not report a definition-search error, got:\n{text}"
    );

    Ok(())
}
