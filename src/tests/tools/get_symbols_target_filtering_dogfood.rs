//! Dogfood target filtering test for GetSymbolsTool: reads the tool's own
//! source tree through a snapshot fixture.

use anyhow::Result;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tests::tools::get_symbols_target_filtering::symbols_tool_source_tree;
use crate::tools::GetSymbolsTool;

#[tokio::test]
async fn test_target_minimal_mode_includes_body_for_child_symbols() -> Result<()> {
    let handler = snapshot_context(symbols_tool_source_tree())?;
    let tool = GetSymbolsTool {
        file_path: "mod.rs".to_string(),
        max_depth: 2,
        target: Some("call_tool".to_string()),
        limit: None,
        offset: 0,
        mode: Some("minimal".to_string()),
        workspace: None,
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };

    let text = call_tool_result_text(&tool.call_tool(&handler).await?);

    assert!(
        text.contains("call_tool"),
        "Should find the targeted method.\nGot: {}",
        text
    );
    assert!(
        text.contains("resolve_workspace_target"),
        "Mode 'minimal' with target set should include code body for child symbols.\nGot: {}",
        text
    );
    Ok(())
}
