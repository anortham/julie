//! Tests for content-search hit scoring.
//!
//! Content queries route through the unified search path, which returns
//! mixed-kind hits with real BM25 + rerank scores. These tests verify that
//! content-style matches produce meaningful scores and ordering.

use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::{FastSearchTool, ManageWorkspaceTool};

#[tokio::test(flavor = "multi_thread")]
async fn content_hits_have_nonzero_scores_sorted_by_score() {
    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    fs::write(
        src_dir.join("dominant.rs"),
        "pub fn dominant() -> &'static str {\n    // rankmarker rankmarker rankmarker rankmarker\n    \"rankmarker rankmarker rankmarker rankmarker\"\n}\n",
    )
    .unwrap();
    fs::write(
        src_dir.join("sparse.rs"),
        "pub fn sparse() -> &'static str {\n    // rankmarker\n    \"ok\"\n}\n",
    )
    .unwrap();

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
        query: "rankmarker".to_string(),
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
        "unified search for 'rankmarker' should return hits"
    );

    let seen_files: HashSet<&str> = execution.hits.iter().map(|hit| hit.file.as_str()).collect();
    for expected in ["src/dominant.rs", "src/sparse.rs"] {
        assert!(
            seen_files.contains(expected),
            "expected hit for {expected}, got files: {:?}",
            seen_files
        );
    }

    assert!(
        execution.hits.iter().any(|hit| hit.score > 0.0),
        "content results should carry non-zero ranking scores"
    );
    assert!(
        execution.hits.first().unwrap().score > execution.hits.last().unwrap().score,
        "top content hit should have a higher score than the last hit; hits={:?}",
        execution
            .hits
            .iter()
            .map(|hit| (&hit.file, &hit.kind, hit.score))
            .collect::<Vec<_>>()
    );
    for window in execution.hits.windows(2) {
        assert!(
            window[0].score >= window[1].score,
            "content hits should be sorted by descending score; hits={:?}",
            execution
                .hits
                .iter()
                .map(|hit| (&hit.file, &hit.kind, hit.score))
                .collect::<Vec<_>>()
        );
    }
}
