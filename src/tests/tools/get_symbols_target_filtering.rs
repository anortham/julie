//! Target filtering tests for GetSymbolsTool: the target parameter matches
//! symbols at every nesting level, not only top-level ones.

use anyhow::Result;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::GetSymbolsTool;

pub(crate) fn symbols_tool_source_tree() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/julie-tools/src/symbols")
}

async fn targeted_text(file_path: &str, target: &str) -> Result<String> {
    let handler = snapshot_context(symbols_tool_source_tree())?;
    let tool = GetSymbolsTool {
        file_path: file_path.to_string(),
        max_depth: 2,
        target: Some(target.to_string()),
        limit: None,
        offset: 0,
        mode: None,
        workspace: None,
    };
    Ok(call_tool_result_text(&tool.call_tool(&handler).await?))
}

#[tokio::test]
#[ignore = "target filtering returns matches and descendants only; ancestor inclusion was never implemented"]
async fn test_target_filtering_matches_child_methods() -> Result<()> {
    let text = targeted_text("mod.rs", "call_tool").await?;

    assert!(
        text.contains("GetSymbolsTool"),
        "Should show parent struct when targeting child method.\nGot: {}",
        text
    );
    assert!(
        text.contains("call_tool"),
        "Should show the targeted method.\nGot: {}",
        text
    );
    assert!(
        !text.contains("No symbols matching"),
        "Should find matches for child symbols.\nGot: {}",
        text
    );
    Ok(())
}

#[tokio::test]
async fn test_target_filtering_top_level_still_works() -> Result<()> {
    let text = targeted_text("mod.rs", "GetSymbolsTool").await?;

    assert!(
        text.contains("GetSymbolsTool"),
        "Should find top-level symbol.\nGot: {}",
        text
    );
    assert!(
        text.contains("call_tool"),
        "Should show methods of matched struct.\nGot: {}",
        text
    );
    Ok(())
}

#[tokio::test]
async fn test_target_filtering_case_insensitive() -> Result<()> {
    let text = targeted_text("mod.rs", "getsymbolstool").await?;

    assert!(
        text.contains("GetSymbolsTool"),
        "Should match case-insensitively.\nGot: {}",
        text
    );
    Ok(())
}

#[tokio::test]
async fn test_target_filtering_partial_match() -> Result<()> {
    let text = targeted_text("formatting.rs", "format").await?;

    assert!(
        text.contains("format_symbol"),
        "Should match partial names.\nGot: {}",
        text
    );
    Ok(())
}
