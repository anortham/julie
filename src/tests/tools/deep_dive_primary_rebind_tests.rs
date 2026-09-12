use std::fs;

use anyhow::Result;
use tempfile::TempDir;

use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::deep_dive::{DeepDiveDepth, DeepDiveTool};

#[tokio::test]
async fn test_deep_dive_primary_uses_rebound_current_primary_store() -> Result<()> {
    let dir = TempDir::new()?;
    fs::create_dir_all(dir.path().join("src"))?;
    fs::write(
        dir.path().join("src/rebound.rs"),
        "pub fn rebound_primary_symbol() {}\n",
    )?;
    let context = snapshot_context(dir.path())?;

    let result = DeepDiveTool {
        symbol: "rebound_primary_symbol".to_string(),
        depth: DeepDiveDepth::Overview,
        context_file: None,
        workspace: Some("primary".to_string()),
        semantics: None,
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    }
    .call_tool(&context)
    .await?;

    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("src/rebound.rs:1") && result_text.contains("rebound_primary_symbol"),
        "deep_dive should use the rebound current-primary store instead of the stale loaded workspace: {result_text}"
    );

    Ok(())
}
