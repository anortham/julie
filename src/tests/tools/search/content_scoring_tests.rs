//! Tests for content-search hit scoring.
//!
//! Covers Task 9: `SearchHit.score` for content (line-mode) hits must be a
//! neutral `0.0_f32` rather than the old synthetic `workspace_total - idx`
//! count-derived pseudo-score. Real per-line BM25 is deferred; the fake score
//! was misleading downstream consumers that assumed scores were comparable
//! across searches.

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

    // Five files each containing one match for "marker_token". Line-mode
    // search should return 5 hits, one per file.
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
        limit: 10,
        search_target: "content".to_string(),
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
        .expect("execute_with_trace populates execution for content search");

    assert_eq!(
        execution.hits.len(),
        5,
        "expected 5 content hits, one per seeded file; got {}: {:?}",
        execution.hits.len(),
        execution
            .hits
            .iter()
            .map(|h| &h.file)
            .collect::<Vec<_>>(),
    );

    for hit in &execution.hits {
        assert_eq!(
            hit.score, 0.0_f32,
            "content hit score must be 0.0 (Task 9); got {} for {}:{:?}",
            hit.score, hit.file, hit.line,
        );
    }

    // All scores equal ⇒ set of unique scores has exactly one element.
    let unique_scores: std::collections::BTreeSet<u32> = execution
        .hits
        .iter()
        .map(|h| h.score.to_bits())
        .collect();
    assert_eq!(
        unique_scores.len(),
        1,
        "content hit scores must all be equal (no synthetic ranking); \
         unique bit patterns: {:?}",
        unique_scores,
    );
}
