//! Tests for file-name search behavior via the unified search path.
//!
//! After T8, `search_target` is removed from the public surface. All queries
//! (including file-path fragments like "mod.rs") route through the unified
//! `execute_search_unified` path.  The tests below verify that file-name
//! queries still work correctly via the new path.

use crate::tools::ManageWorkspaceTool;
use crate::tools::search::FastSearchTool;
use crate::tools::search::trace::{FilePatternDiagnostic, HintKind};
use crate::{handler::JulieServerHandler, mcp_compat::CallToolResult};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content| match content.raw {
            rmcp::model::RawContent::Text(ref text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn initialize_indexed_handler(workspace_path: &Path) -> JulieServerHandler {
    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("handler for test");
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .expect("initialize workspace");

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("index workspace");

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    handler
}

fn seed_scoped_mod_rs_workspace(workspace_path: &Path) {
    for i in 0..10 {
        let dir = workspace_path.join(format!("aaa{i:02}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("mod.rs"),
            format!("pub fn outside_scope_{i}() {{}}\n"),
        )
        .unwrap();
    }

    for path in ["scope/inside/mod.rs", "scope/extra/mod.rs"] {
        let full_path = workspace_path.join(path);
        fs::create_dir_all(full_path.parent().unwrap()).unwrap();
        fs::write(&full_path, "pub fn in_scope() {}\n").unwrap();
    }
}

/// After T8, search_target is gone from the public surface. Serialising a
/// `FastSearchTool` without `search_target` must succeed.
#[test]
fn test_fast_search_deserializes_without_search_target() {
    let tool: FastSearchTool = serde_json::from_str(r#"{"query":"line_mode.rs"}"#).unwrap();

    assert_eq!(tool.query, "line_mode.rs");
    // After T8, context_lines defaults to Some(1) (the unified default) since
    // there are no per-target context_lines rules.  The struct can still be
    // deserialised without the field.
    assert_eq!(tool.context_lines, Some(1));
}

/// Unknown fields (including the now-removed `search_target`) should be
/// silently ignored on deserialisation (serde `deny_unknown_fields` is NOT set
/// on FastSearchTool).
#[test]
fn test_fast_search_ignores_legacy_search_target_field_on_deserialization() {
    // Clients that still send search_target in JSON should not break.
    let tool = serde_json::from_str::<FastSearchTool>(
        r#"{"query":"line_mode.rs","search_target":"files","limit":7}"#,
    )
    .expect("legacy clients may still send search_target; it must be ignored");

    assert_eq!(tool.query, "line_mode.rs");
    assert_eq!(tool.limit, 7);
    assert_eq!(tool.context_lines, Some(1));
}

#[tokio::test(flavor = "multi_thread")]
async fn fast_search_unified_returns_file_hits_for_filename_query() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();

    // Use a distinctive basename so the unified path's file-basename ranking
    // can drive ordering without colliding with anything else in the index.
    fs::write(
        workspace_path.join("src/browser_client.rs"),
        "pub fn make_browser_client() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("handler for test");
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .expect("initialize workspace");

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("index workspace");

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // After T8 all traffic goes through the unified path — no search_target.
    let execution = FastSearchTool {
        query: "browser_client".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        context_lines: None,
        exclude_tests: None,
        backend: None,
        workspace: Some("primary".to_string()),
        return_format: "full".to_string(),
    }
    .execute_with_trace(&handler)
    .await
    .expect("file search should not error")
    .execution
    .expect("execute_with_trace populates execution");

    // Unified search returns hits — should include both symbol and file rows
    // for `browser_client`.
    assert!(
        !execution.hits.is_empty(),
        "unified search for 'browser_client' should return results"
    );
    // At least one hit should reference the browser_client.rs file.
    assert!(
        execution
            .hits
            .iter()
            .any(|h| h.file.ends_with("browser_client.rs")),
        "at least one hit should reference browser_client.rs, got: {:?}",
        execution
            .hits
            .iter()
            .map(|h| h.file.as_str())
            .collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fast_search_file_pattern_scopes_results() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();
    seed_scoped_mod_rs_workspace(&workspace_path);

    let handler = initialize_indexed_handler(&workspace_path).await;

    let response = FastSearchTool {
        query: "mod.rs".to_string(),
        language: None,
        file_pattern: Some("scope/**".to_string()),
        limit: 5,
        context_lines: None,
        exclude_tests: None,
        backend: None,
        workspace: Some("primary".to_string()),
        return_format: "locations".to_string(),
    }
    .execute_with_trace(&handler)
    .await
    .expect("file search should not error")
    .result;

    let output = extract_text_from_result(&response);
    // Scoped to scope/** — out-of-scope files should not appear.
    assert!(
        output.contains("scope/inside/mod.rs") || output.contains("scope/extra/mod.rs"),
        "scoped search should return in-scope mod.rs hits, got: {output}"
    );
    assert!(
        !output.contains("aaa"),
        "file_pattern=scope/** must not return out-of-scope mod.rs hits, got: {output}"
    );
    assert!(
        !output.contains("No files found"),
        "scoped search should not claim there were no results when in-scope files exist, got: {output}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn filename_locations_preserve_exact_file_rank_instead_of_line_mode_mentions() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("lib")).unwrap();

    fs::write(
        workspace_path.join("lib/application.js"),
        "function createApplication() {\n  return {};\n}\n",
    )
    .unwrap();
    fs::write(
        workspace_path.join("lib/response.js"),
        "// application.js application.js application.js\nfunction sendResponse() {}\n",
    )
    .unwrap();

    let handler = initialize_indexed_handler(&workspace_path).await;

    let response = FastSearchTool {
        query: "application.js".to_string(),
        language: None,
        file_pattern: None,
        limit: 5,
        context_lines: None,
        exclude_tests: None,
        backend: None,
        workspace: Some("primary".to_string()),
        return_format: "locations".to_string(),
    }
    .execute_with_trace(&handler)
    .await
    .expect("file search should not error")
    .result;

    let output = extract_text_from_result(&response);
    let application_pos = output
        .find("lib/application.js")
        .expect("exact filename hit should be rendered");
    if let Some(response_pos) = output.find("lib/response.js") {
        assert!(
            application_pos < response_pos,
            "locations mode should preserve unified exact-file ranking; got:\n{output}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn locations_scope_rescue_labels_out_of_scope_results() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src/ui")).unwrap();
    fs::create_dir_all(workspace_path.join("docs")).unwrap();

    fs::write(workspace_path.join("src/ui/view.rs"), "pub fn view() {}\n").unwrap();
    fs::write(
        workspace_path.join("docs/search.md"),
        "scope rescue marker lives only outside the requested source tree\n",
    )
    .unwrap();

    let handler = initialize_indexed_handler(&workspace_path).await;

    let run = FastSearchTool {
        query: "scope rescue marker".to_string(),
        language: None,
        file_pattern: Some("src/ui/**".to_string()),
        limit: 5,
        context_lines: None,
        exclude_tests: None,
        backend: None,
        workspace: Some("primary".to_string()),
        return_format: "locations".to_string(),
    }
    .execute_with_trace(&handler)
    .await
    .expect("search should not error");

    let execution = run
        .execution
        .expect("execute_with_trace should populate execution");
    let output = extract_text_from_result(&run.result);

    assert!(
        execution.trace.scope_relaxed,
        "out-of-scope miss should trigger scope rescue"
    );
    assert!(
        output.starts_with(
            "NOTE: 0 matches within file_pattern=src/ui/**. Showing 1 results from the full codebase (outside requested scope).",
        ),
        "locations scope rescue should lead with scope label, got:\n{output}"
    );
    assert!(
        output.contains("docs/search.md"),
        "rescued output should still show out-of-scope result, got:\n{output}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn request_level_whitespace_separated_globs_return_syntax_hint() {
    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("handler for test");

    // A whitespace-separated multi-glob should be caught by the input
    // diagnostic layer regardless of search mode.
    let run = FastSearchTool {
        query: "mod.rs".to_string(),
        language: None,
        file_pattern: Some("src/** docs/**".to_string()),
        limit: 10,
        context_lines: None,
        exclude_tests: None,
        backend: None,
        workspace: Some("primary".to_string()),
        return_format: "full".to_string(),
    }
    .execute_with_trace(&handler)
    .await
    .expect("search should not error");

    let execution = run
        .execution
        .expect("execute_with_trace must populate execution for request-level diagnostics");
    let text = extract_text_from_result(&run.result);

    assert!(execution.hits.is_empty());
    assert_eq!(execution.total_results, 0);
    assert!(!execution.relaxed);
    assert_eq!(execution.trace.strategy_id, "fast_search_input_diagnostic");
    assert_eq!(
        execution.trace.file_pattern_diagnostic,
        Some(FilePatternDiagnostic::WhitespaceSeparatedMultiGlob)
    );
    assert_eq!(
        execution.trace.hint_kind,
        Some(HintKind::FilePatternSyntaxHint)
    );
    assert!(text.contains("multiple globs separated by whitespace"));
    assert!(text.contains("Use ',' or '|'"));
}
