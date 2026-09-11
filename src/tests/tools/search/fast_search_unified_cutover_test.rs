//! T8 cutover acceptance tests.
//!
//! Verifies that after removing `search_target` from the public surface,
//! `FastSearchTool` routes every call through `execute_search_unified` and
//! returns mixed-kind hits (symbol rows + file rows) from a single query.

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::search::FastSearchTool;
use julie_test_support::FakeToolContext;

async fn index_workspace(workspace_path: &std::path::Path) -> Result<FakeToolContext> {
    snapshot_context(workspace_path)
}

/// A single `FastSearchTool` call for "BrowserClient" should return both
/// symbol-kind hits and file-kind hits when both exist in the indexed workspace.
/// This confirms the unified path returns mixed-kind results without a
/// `search_target` discriminant.
#[tokio::test(flavor = "multi_thread")]
async fn mixed_kinds() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create a source file whose name matches the query (file-kind hit) AND
    // defines a struct with the same name (symbol-kind hit).
    fs::create_dir_all(workspace_path.join("src"))?;
    fs::write(
        workspace_path.join("src/browser_client.rs"),
        "pub struct BrowserClient {\n    pub url: String,\n}\n\nimpl BrowserClient {\n    pub fn new(url: String) -> Self {\n        Self { url }\n    }\n}\n",
    )?;
    // Add a second file so the index has multiple files.
    fs::write(
        workspace_path.join("src/lib.rs"),
        "pub mod browser_client;\n",
    )?;

    let handler = index_workspace(workspace_path).await?;

    let execution = FastSearchTool {
        query: "BrowserClient".to_string(),
        limit: 20,
        offset: 0,
        workspace: Some("primary".to_string()),
        context_lines: None,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("unified search should populate execution trace");

    // Must have at least one hit.
    assert!(
        !execution.hits.is_empty(),
        "unified search for 'BrowserClient' should return hits, got none"
    );

    // Collect all kinds.
    let kinds: Vec<&str> = execution.hits.iter().map(|h| h.kind.as_str()).collect();

    // At least one symbol-kind hit (struct, function, etc.) must be present.
    let has_symbol_hit = execution
        .hits
        .iter()
        .any(|h| !matches!(h.kind.as_str(), "file" | ""));
    assert!(
        has_symbol_hit,
        "expected at least one symbol-kind hit, got kinds: {:?}",
        kinds
    );

    // At least one file-kind hit must be present (browser_client.rs basename match).
    let has_file_hit = execution.hits.iter().any(|h| h.kind == "file");
    assert!(
        has_file_hit,
        "expected at least one file-kind hit for browser_client.rs, got kinds: {:?}",
        kinds
    );

    Ok(())
}
