//! Tests for content-search hit scoring.
//!
//! Pre-T8: Task 9 required line-mode content hits to carry a neutral
//! `0.0_f32` score (no synthetic count-derived ranking).
//!
//! After T8 atomic cutover: content queries route through the unified search
//! path, which returns mixed-kind hits (symbol rows + file rows) with real
//! BM25 + rerank scores.  The test below now verifies the unified path's
//! invariants for content-style queries: at least one hit per seeded file,
//! and every seeded file appears in the results.

use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::{FastSearchTool, ManageWorkspaceTool};

#[tokio::test(flavor = "multi_thread")]
async fn content_hits_have_neutral_zero_score() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    // Five files each containing one match for "marker_token".  Each file
    // has both a symbol whose body mentions the token and a file row whose
    // content carries it.  After T8 the unified path can return both.
    for i in 0..5 {
        let body = format!(
            "pub fn sample_{i}() -> &'static str {{\n    // marker_token stays here\n    \"ok\"\n}}\n"
        );
        fs::write(src_dir.join(format!("file_{i}.rs")), body).unwrap();
    }

    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("handler for test");
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .expect("initialize workspace");

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool
        .call_tool(&handler)
        .await
        .expect("index workspace");

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let tool = FastSearchTool {
        query: "marker_token".to_string(),
        language: None,
        file_pattern: None,
        limit: 50,
        context_lines: Some(0),
        exclude_tests: None,
        workspace: Some("primary".to_string()),
        return_format: "full".to_string(),
    };

    let execution = tool
        .execute_with_trace(&handler)
        .await
        .expect("search should not error")
        .execution
        .expect("execute_with_trace populates execution for unified search");

    assert!(
        !execution.hits.is_empty(),
        "unified search for 'marker_token' should return hits"
    );

    // Every seeded file should appear among the hits (either as a symbol row
    // or a file row).  Unified search is allowed to return either or both.
    let seen_files: HashSet<&str> =
        execution.hits.iter().map(|hit| hit.file.as_str()).collect();
    for i in 0..5 {
        let expected = format!("src/file_{i}.rs");
        assert!(
            seen_files.contains(expected.as_str()),
            "expected hit for {expected}, got files: {:?}",
            seen_files
        );
    }
}
