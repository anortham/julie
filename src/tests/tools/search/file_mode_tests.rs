use crate::tools::ManageWorkspaceTool;
use crate::tools::search::FastSearchTool;
use crate::tools::search::target::SearchTarget;
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

#[test]
fn test_fast_search_deserializes_files_target_without_default_context_lines() {
    let tool: FastSearchTool =
        serde_json::from_str(r#"{"query":"line_mode.rs","search_target":"files"}"#).unwrap();

    assert_eq!(tool.search_target, "files");
    assert_eq!(tool.context_lines, None);
    assert_eq!(tool.validated_search_target().unwrap(), SearchTarget::Files);
}

#[test]
fn test_fast_search_deserializes_paths_alias_as_files() {
    let tool: FastSearchTool =
        serde_json::from_str(r#"{"query":"line_mode.rs","search_target":"paths"}"#).unwrap();

    assert_eq!(tool.search_target, "files");
    assert_eq!(tool.context_lines, None);
    assert_eq!(tool.validated_search_target().unwrap(), SearchTarget::Files);
}

#[test]
fn test_fast_search_rejects_unknown_target_during_deserialization() {
    let err = serde_json::from_str::<FastSearchTool>(
        r#"{"query":"line_mode.rs","search_target":"defintions"}"#,
    )
    .unwrap_err();

    assert!(
        err.to_string().contains("Invalid search_target"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_fast_search_rejects_context_lines_for_files_target() {
    let tool = FastSearchTool {
        query: "line_mode.rs".to_string(),
        search_target: "files".to_string(),
        context_lines: Some(0),
        ..Default::default()
    };

    let err = tool.validated_search_target().unwrap_err();
    assert!(
        err.to_string().contains("does not support context_lines"),
        "unexpected error: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fast_search_files_execution_returns_file_hits_and_demotes_tests() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src/tools/search")).unwrap();
    fs::create_dir_all(workspace_path.join("tests/tools/search")).unwrap();

    fs::write(
        workspace_path.join("src/tools/search/mod.rs"),
        "pub fn prod_search() {}\n",
    )
    .unwrap();
    fs::write(
        workspace_path.join("tests/tools/search/mod.rs"),
        "#[test]\nfn file_mode_test() {}\n",
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

    let execution = FastSearchTool {
        query: "mod.rs".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        search_target: "files".to_string(),
        context_lines: None,
        exclude_tests: None,
        workspace: Some("primary".to_string()),
        return_format: "full".to_string(),
    }
    .execute_with_trace(&handler)
    .await
    .expect("file search should not error")
    .execution
    .expect("execute_with_trace populates execution for file search");

    assert!(
        matches!(
            execution.kind,
            crate::tools::search::trace::SearchExecutionKind::Files
        ),
        "file search should report file execution kind"
    );
    assert_eq!(execution.hits.len(), 2);
    assert_eq!(execution.hits[0].file, "src/tools/search/mod.rs");
    assert_eq!(execution.hits[1].file, "tests/tools/search/mod.rs");

    for hit in &execution.hits {
        assert_eq!(hit.kind, "file");
        assert_eq!(hit.line, None);
        assert_eq!(hit.symbol_id, None);
    }
    assert!(
        execution.definition_symbols().is_empty(),
        "file hits must not masquerade as definition symbols"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fast_search_files_locations_output_is_path_only() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src/tools/search")).unwrap();

    fs::write(
        workspace_path.join("src/tools/search/mod.rs"),
        "pub fn prod_search() {}\n",
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

    let response = FastSearchTool {
        query: "mod.rs".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        search_target: "files".to_string(),
        context_lines: None,
        exclude_tests: None,
        workspace: Some("primary".to_string()),
        return_format: "locations".to_string(),
    }
    .execute_with_trace(&handler)
    .await
    .expect("file search should not error")
    .result;

    let output = extract_text_from_result(&response);
    let lines: Vec<&str> = output.lines().collect();

    assert!(
        lines[0].contains("file matches for \"mod.rs\""),
        "unexpected header: {output}"
    );
    assert_eq!(lines[2], "src/tools/search/mod.rs");
    assert!(!lines[2].contains(':'));
    assert!(!output.contains("(file)"));
    assert!(!output.contains("prod_search"));
}

#[tokio::test(flavor = "multi_thread")]
async fn fast_search_files_file_pattern_does_not_starve_scoped_hits() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();
    seed_scoped_mod_rs_workspace(&workspace_path);

    let handler = initialize_indexed_handler(&workspace_path).await;

    let response = FastSearchTool {
        query: "mod.rs".to_string(),
        language: None,
        file_pattern: Some("scope/**".to_string()),
        limit: 5,
        search_target: "files".to_string(),
        context_lines: None,
        exclude_tests: None,
        workspace: Some("primary".to_string()),
        return_format: "locations".to_string(),
    }
    .execute_with_trace(&handler)
    .await
    .expect("file search should not error")
    .result;

    let output = extract_text_from_result(&response);
    assert!(
        output.contains("scope/inside/mod.rs"),
        "scoped file search should include target hit, got: {output}"
    );
    assert!(
        output.contains("scope/extra/mod.rs"),
        "scoped file search should include every in-scope hit, got: {output}"
    );
    assert!(
        !output.contains("No files found"),
        "scoped file search should not claim there were no files, got: {output}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fast_search_files_filtered_header_uses_filtered_total() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();
    seed_scoped_mod_rs_workspace(&workspace_path);

    let handler = initialize_indexed_handler(&workspace_path).await;

    let response = FastSearchTool {
        query: "mod.rs".to_string(),
        language: None,
        file_pattern: Some("scope/**".to_string()),
        limit: 20,
        search_target: "files".to_string(),
        context_lines: None,
        exclude_tests: None,
        workspace: Some("primary".to_string()),
        return_format: "full".to_string(),
    }
    .execute_with_trace(&handler)
    .await
    .expect("file search should not error")
    .result;

    let output = extract_text_from_result(&response);
    assert!(
        output.starts_with("2 file matches for \"mod.rs\":"),
        "filtered file search header should use the in-scope total, got: {output}"
    );
    assert!(
        !output.contains("showing 2 of 12"),
        "filtered file search should not leak raw pre-filter totals, got: {output}"
    );
}
