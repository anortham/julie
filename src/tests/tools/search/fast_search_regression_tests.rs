use anyhow::Result;
use std::fs;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};
use crate::tools::search::FastSearchTool;
use crate::tools::search::text_search::definition_search_with_index_for_test;
use crate::tools::search::trace::{LineEnrichmentStatus, ZeroHitReason};
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

async fn ensure_primary_projection_current(handler: &JulieServerHandler) {
    mark_search_ready(handler).await;

    let snapshot = handler
        .primary_workspace_snapshot()
        .await
        .expect("primary snapshot");
    let search_index = snapshot.search_index.expect("primary search index");
    let mut db = snapshot
        .database
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let idx = search_index
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::search::SearchProjection::tantivy(snapshot.binding.workspace_id)
        .ensure_current_with_gate(&mut db, &idx, &handler.indexing_status.search_ready)
        .expect("projection current");
}

async fn index_workspace(workspace_path: &std::path::Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;
    handler
        .stop_loaded_workspace_file_watching_for_test()
        .await
        .expect("stop file watcher for search-only test");

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
    ensure_primary_projection_current(&handler).await;
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
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

fn helper_symbol(id: &str, name: &str, metadata: Option<serde_json::Value>) -> Symbol {
    let metadata = metadata.and_then(|value| {
        value.as_object().map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
    });

    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "src/lib.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 20,
        start_byte: 0,
        end_byte: 20,
        signature: Some(format!("fn {name}()")),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata,
        semantic_group: None,
        confidence: None,
        code_context: Some(format!("fn {name}() {{}}")),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

fn test_metadata() -> serde_json::Value {
    serde_json::json!({
        "is_test": true,
        "test_role": "test_case"
    })
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

/// Regression: a qualified-name Elixir symbol (`Phoenix.Router`) must be
/// returned when querying "Router" (dot-separated names are token-split so
/// "router" appears as a term).
///
/// Previously this tested a SQLite rescue path (symbols stored only in SQLite
/// but not in Tantivy).  The T9 unified-schema cutover removed that fallback;
/// symbols must be in both stores.  The invariant under test is now: symbols
/// indexed in Tantivy via the unified schema are findable by a partial token.
#[test]
fn tantivy_indexed_qualified_name_found_by_partial_token() -> Result<()> {
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
    // Index the symbol in Tantivy (the unified path requires it).
    let sym = rescued_symbol();
    index.add_search_doc(&SearchDocument::symbol_from_parts(
        &sym.id,
        &sym.name,
        sym.signature.as_deref().unwrap_or(""),
        sym.doc_comment.as_deref().unwrap_or(""),
        sym.code_context.as_deref().unwrap_or(""),
        &sym.file_path,
        "module",
        &sym.language,
        sym.start_line,
    ))?;
    index.commit()?;

    let filter = SearchFilter {
        language: Some("elixir".to_string()),
        kind: None,
        file_pattern: None,
        exclude_tests: false,
    };
    let (symbols, _relaxed, total) =
        definition_search_with_index_for_test("Router", &filter, 5, &index, Some(&db))?;

    assert_eq!(
        symbols.len(),
        1,
        "Expected one result for 'Router'. Got: {:?}",
        symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert_eq!(symbols[0].name, "Phoenix.Router");
    assert_eq!(total, 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn content_nl_default_excludes_tests_but_explicit_false_includes_them() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    fs::create_dir_all(workspace_path.join("src/tests"))?;
    fs::write(
        workspace_path.join("src/auth.rs"),
        "pub fn refresh_token() {\n    let refresh = \"token\";\n}\n",
    )?;
    fs::write(
        workspace_path.join("src/tests/auth_test.rs"),
        "#[test]\nfn refresh_token_test() {\n    let refresh = \"token\";\n}\n",
    )?;

    let handler = index_workspace(workspace_path).await?;

    let default_run = FastSearchTool {
        query: "refresh token".to_string(),
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: Some(0),
        exclude_tests: None,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("content search should populate execution trace");

    assert!(
        default_run.hits.iter().any(|hit| hit.file == "src/auth.rs"),
        "NL content default should keep source hits, got: {:?}",
        default_run
            .hits
            .iter()
            .map(|hit| hit.file.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        default_run
            .hits
            .iter()
            .all(|hit| hit.file != "src/tests/auth_test.rs"),
        "NL content default should exclude tests unless caller opts in, got: {:?}",
        default_run
            .hits
            .iter()
            .map(|hit| hit.file.as_str())
            .collect::<Vec<_>>()
    );

    let explicit_include = FastSearchTool {
        query: "refresh token".to_string(),
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: Some(0),
        exclude_tests: Some(false),
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("content search should populate execution trace");

    assert!(
        explicit_include
            .hits
            .iter()
            .any(|hit| hit.file == "src/tests/auth_test.rs"),
        "explicit exclude_tests=false should include test hits, got: {:?}",
        explicit_include
            .hits
            .iter()
            .map(|hit| hit.file.as_str())
            .collect::<Vec<_>>()
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn content_auto_exclude_tests_respects_explicit_test_file_pattern() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    fs::create_dir_all(workspace_path.join("src/tests"))?;
    fs::write(
        workspace_path.join("src/tests/query_density_test.rs"),
        "#[test]\nfn query_density_terms() {\n    let note = \"Density repeated query terms case insensitive\";\n}\n",
    )?;

    let handler = index_workspace(workspace_path).await?;
    let query = "density repeated query terms case insensitive";

    let scoped_to_tests = FastSearchTool {
        query: query.to_string(),
        language: Some("rust".to_string()),
        file_pattern: Some("src/tests/**".to_string()),
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: Some(0),
        exclude_tests: None,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("content search should populate execution trace");

    assert!(
        scoped_to_tests
            .hits
            .iter()
            .any(|hit| hit.file == "src/tests/query_density_test.rs"),
        "auto exclude_tests should not filter an explicit test file_pattern, got hits: {:?}, zero_hit_reason: {:?}",
        scoped_to_tests
            .hits
            .iter()
            .map(|hit| hit.file.as_str())
            .collect::<Vec<_>>(),
        scoped_to_tests.trace.zero_hit_reason
    );
    assert_eq!(scoped_to_tests.trace.zero_hit_reason, None);

    let unscoped = FastSearchTool {
        query: query.to_string(),
        language: Some("rust".to_string()),
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: Some(0),
        exclude_tests: None,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("content search should populate execution trace");

    assert!(
        unscoped.hits.is_empty(),
        "unscoped NL auto exclude_tests should still filter tests, got hits: {:?}",
        unscoped
            .hits
            .iter()
            .map(|hit| hit.file.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        unscoped.trace.zero_hit_reason,
        Some(ZeroHitReason::TestFiltered)
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn content_test_intent_keeps_and_ranks_test_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    fs::create_dir_all(workspace_path.join("src/tests"))?;
    fs::write(
        workspace_path.join("src/aaa_refresh_token.rs"),
        "pub fn refresh_token_probe() {\n    // test refresh token behavior is described here\n}\n",
    )?;
    fs::write(
        workspace_path.join("src/tests/zzz_refresh_token_test.rs"),
        "#[test]\nfn refresh_token_probe_test() {\n    // test refresh token behavior is verified here\n}\n",
    )?;

    let handler = index_workspace(workspace_path).await?;
    let execution = FastSearchTool {
        query: "test refresh token".to_string(),
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: Some(0),
        exclude_tests: None,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("content search should populate execution trace");

    assert!(
        execution
            .hits
            .iter()
            .any(|hit| hit.file == "src/tests/zzz_refresh_token_test.rs"),
        "test intent should keep matching tests, got: {:?}",
        execution
            .hits
            .iter()
            .map(|hit| hit.file.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        execution.hits.first().map(|hit| hit.file.as_str()),
        Some("src/tests/zzz_refresh_token_test.rs"),
        "test intent should rank test files before source files, got: {:?}",
        execution
            .hits
            .iter()
            .map(|hit| hit.file.as_str())
            .collect::<Vec<_>>()
    );

    Ok(())
}

#[test]
fn definition_test_intent_uses_metadata_for_inline_test_helpers_before_centrality() -> Result<()> {
    let db_dir = TempDir::new()?;
    let db_path = db_dir.path().join("symbols.db");
    let mut db = SymbolDatabase::new(&db_path)?;
    db.store_file_info(&FileInfo {
        path: "src/lib.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash".to_string(),
        size: 100,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 2,
        line_count: 6,
        content: Some(
            "fn helper_refresh() {}\n#[cfg(test)] mod tests { #[test] fn helper_refresh_case() {} }\n"
                .to_string(),
        ),
    })?;

    let source = helper_symbol("source-helper", "helper_refresh", None);
    let test_helper = helper_symbol("test-helper", "helper_refresh_case", Some(test_metadata()));
    db.store_symbols(&[source.clone(), test_helper.clone()])?;
    db.conn.execute(
        "UPDATE symbols SET reference_score = ?1 WHERE id = ?2",
        rusqlite::params![10.0_f64, "source-helper"],
    )?;

    let index_dir = TempDir::new()?;
    let index = SearchIndex::create(index_dir.path())?;
    for symbol in [&source, &test_helper] {
        index.add_search_doc(&crate::search::index::SearchDocument::for_symbol(
            symbol,
            vec![],
            String::new(),
            String::new(),
        ))?;
    }
    index.commit()?;

    let filter = SearchFilter {
        language: Some("rust".to_string()),
        kind: None,
        file_pattern: None,
        exclude_tests: false,
    };
    let (symbols, _relaxed, _total) = definition_search_with_index_for_test(
        "test helper refresh",
        &filter,
        2,
        &index,
        Some(&db),
    )?;

    assert_eq!(
        symbols.first().map(|symbol| symbol.id.as_str()),
        Some("test-helper"),
        "test intent should use metadata for inline test helpers before centrality can dominate; got: {:?}",
        symbols
            .iter()
            .map(|symbol| (symbol.id.as_str(), symbol.name.as_str(), symbol.confidence))
            .collect::<Vec<_>>()
    );

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
async fn content_locations_trace_uses_line_hits_without_matching_line_text() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("app.py"),
        "def main():\n    print(\"trace marker phrase\")\n",
    )?;

    let handler = index_workspace(workspace_path).await?;
    let run = FastSearchTool {
        query: "trace marker phrase".to_string(),
        return_format: "locations".to_string(),
        limit: 10,
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let text = extract_text(&run.result);
    assert!(
        !text.contains("print(\"trace marker phrase\")"),
        "locations output should omit matching line text, got:\n{text}"
    );

    let execution = run.execution.expect("search should return execution trace");
    let line_hit = execution
        .hits
        .iter()
        .find(|hit| hit.kind == "line" && hit.line == Some(2))
        .expect("execution hits should include line hit for src/app.py:2");
    assert_eq!(line_hit.file, "src/app.py");
    assert_eq!(
        line_hit.language, "python",
        "line hits should preserve the indexed file language instead of defaulting to Rust"
    );
    assert!(
        text.contains("src/app.py:2"),
        "locations output should include the Python file and line, got:\n{text}"
    );
    assert_eq!(execution.trace.result_count, execution.hits.len());
    assert_eq!(
        execution.trace.line_enrichment_status,
        Some(LineEnrichmentStatus::Applied)
    );
    assert_eq!(execution.trace.line_enrichment_match_count, Some(1));
    assert!(
        execution
            .trace
            .top_hits
            .iter()
            .any(|hit| hit.kind == "line" && hit.line == Some(2)),
        "trace top hits should include line hit for src/app.py:2, got: {:#?}",
        execution.trace.top_hits
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn content_locations_cpp_h_language_filter_keeps_line_hits() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let include_dir = workspace_path.join("include");
    fs::create_dir_all(&include_dir)?;
    fs::write(
        include_dir.join("Widget.h"),
        r#"#pragma once
namespace app {
class Widget {
public:
    int value() const {
        return 42;
    }
};
}
"#,
    )?;

    let handler = index_workspace(workspace_path).await?;
    let run = FastSearchTool {
        query: "return 42".to_string(),
        return_format: "locations".to_string(),
        language: Some("cpp".to_string()),
        limit: 10,
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let text = extract_text(&run.result);
    assert!(
        text.contains("include/Widget.h:6"),
        "C++ .h locations output should include the matching line, got:\n{text}"
    );

    let execution = run.execution.expect("search should return execution trace");
    let line_hit = execution
        .hits
        .iter()
        .find(|hit| hit.kind == "line" && hit.file == "include/Widget.h" && hit.line == Some(6))
        .expect("execution trace should contain a C++ .h line hit");
    assert_eq!(line_hit.language, "cpp");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn content_full_format_includes_matching_line_text() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("app.rs"),
        "fn main() {\n    let full_format_context_marker = 1;\n}\n",
    )?;

    let handler = index_workspace(workspace_path).await?;
    let result = FastSearchTool {
        query: "full_format_context_marker".to_string(),
        return_format: "full".to_string(),
        limit: 10,
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(
        text.contains("let full_format_context_marker = 1;"),
        "full output should include the actual matching line, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn content_full_output_trace_records_line_enrichment_success() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("app.rs"),
        "fn main() {\n    let trace_enrichment_marker = 1;\n}\n",
    )?;

    let handler = index_workspace(workspace_path).await?;
    let run = FastSearchTool {
        query: "trace_enrichment_marker".to_string(),
        return_format: "full".to_string(),
        limit: 10,
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run.execution.expect("search should return execution trace");
    assert_eq!(
        execution.trace.line_enrichment_status,
        Some(LineEnrichmentStatus::Applied)
    );
    assert_eq!(execution.trace.line_enrichment_match_count, Some(1));
    assert!(
        execution.trace.line_match_strategy.is_some(),
        "successful enrichment should record the line-match strategy"
    );
    assert!(execution.trace.line_enrichment_error.is_none());

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn content_full_output_trace_records_line_enrichment_no_matches() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("only_path_marker.rs"), "fn main() {}\n")?;

    let handler = index_workspace(workspace_path).await?;
    let run = FastSearchTool {
        query: "only_path_marker".to_string(),
        return_format: "full".to_string(),
        limit: 10,
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run.execution.expect("search should return execution trace");
    assert!(
        !execution.hits.is_empty(),
        "path-shaped query should still return unified hits"
    );
    assert_eq!(
        execution.trace.line_enrichment_status,
        Some(LineEnrichmentStatus::NoMatches)
    );
    assert_eq!(execution.trace.line_enrichment_match_count, Some(0));
    assert!(execution.trace.line_enrichment_error.is_none());

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
        context_lines: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    // After T8 cutover, the unified surface no longer emits per-mode missing-
    // index messages.  The neutral "Search requires a Tantivy index..." message
    // covers all search modes (definition / content / file) uniformly.
    assert!(
        text.contains("Search requires a Tantivy index"),
        "missing-index message should name the search index, got:\n{text}"
    );
    assert!(
        !text.contains("Definition search requires"),
        "unified path should not report a definition-specific error, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn file_search_preserves_hidden_directory_ranking_in_tool_output() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    fs::create_dir_all(workspace_path.join(".cargo"))?;
    fs::write(
        workspace_path.join(".cargo/config.toml"),
        "[build]\nrustflags = []\n",
    )?;
    fs::write(
        workspace_path.join("Cargo.toml"),
        "[package]\nname = \"dogfood\"\nversion = \"0.1.0\"\n",
    )?;
    fs::write(workspace_path.join("Cargo.lock"), "version = 4\n")?;

    let handler = index_workspace(workspace_path).await?;
    let execution = FastSearchTool {
        query: ".cargo".to_string(),
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("file search should populate execution trace");

    assert_eq!(
        execution.hits.first().map(|hit| hit.file.as_str()),
        Some(".cargo/config.toml"),
        "fast_search file mode should preserve hidden-directory ranking, got: {:?}",
        execution
            .hits
            .iter()
            .map(|hit| hit.file.as_str())
            .collect::<Vec<_>>()
    );

    Ok(())
}
