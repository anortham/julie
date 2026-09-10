//! `GetContextTool` over the primary snapshot served for the current primary root.

use std::fs;

use anyhow::Result;
use tempfile::TempDir;

use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::get_context::GetContextTool;

#[tokio::test]
async fn test_get_context_primary_uses_rebound_current_primary_store() -> Result<()> {
    let dir = TempDir::new()?;
    fs::create_dir_all(dir.path().join("src"))?;
    fs::write(
        dir.path().join("src").join("rebound.rs"),
        "/// rebound context phrase\npub fn rebound_primary_symbol() {}\n",
    )?;
    let context = snapshot_context(dir.path())?;

    let result = GetContextTool {
        query: "rebound context phrase".to_string(),
        max_tokens: Some(1200),
        workspace: Some("primary".to_string()),
        language: Some("rust".to_string()),
        file_pattern: None,
        format: Some("readable".to_string()),
        edited_files: None,
        entry_symbols: None,
        stack_trace: None,
        failing_test: None,
        max_hops: None,
        prefer_tests: None,
        semantics: None,
    }
    .call_tool(&context)
    .await?;

    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("rebound_primary_symbol") && result_text.contains("src/rebound.rs"),
        "get_context should use the rebound current-primary store instead of the stale loaded workspace: {result_text}"
    );

    Ok(())
}
