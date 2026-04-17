//! Dogfood target filtering test for GetSymbolsTool
//!
//! This module contains the single non-ignored test from get_symbols_target_filtering
//! that indexes the full julie repo (~164s). Isolated here so tools-misc bucket
//! does not carry a 164s outlier.

#[cfg(test)]
mod tests {
    use crate::tests::helpers::workspace::create_isolated_storage_handler;
    use crate::tools::{GetSymbolsTool, ManageWorkspaceTool};
    use anyhow::Result;

    async fn repo_handler() -> Result<crate::tests::helpers::workspace::IsolatedStorageHandler> {
        create_isolated_storage_handler(std::env::current_dir()?).await
    }

    #[tokio::test]
    async fn test_target_minimal_mode_includes_body_for_child_symbols() -> Result<()> {
        // BUG: When target is set and mode is "minimal", child symbols (methods)
        // get their body stripped because parent_id.is_none() == false.
        // The fix: when target is set, all matched symbols should get bodies.

        let handler = repo_handler().await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(std::env::current_dir()?.to_string_lossy().to_string()),
            workspace_id: None,
            name: None,
            force: Some(true),
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        // Target a child method with mode="minimal"
        let tool = GetSymbolsTool {
            file_path: "src/tools/symbols/mod.rs".to_string(),
            max_depth: 2,
            target: Some("call_tool".to_string()),
            limit: None,
            mode: Some("minimal".to_string()),
            workspace: None,
        };

        let result = tool.call_tool(&handler).await?;

        let text = result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
            })
            .collect::<Vec<String>>()
            .join("\n");

        // Should find the method
        assert!(
            text.contains("call_tool"),
            "Should find the targeted method.\nGot: {}",
            text
        );

        // The body should be present — this is the key assertion.
        // call_tool contains "resolve_workspace_filter" in its implementation,
        // which would only appear if the code body is extracted.
        assert!(
            text.contains("resolve_workspace_filter"),
            "Mode 'minimal' with target set should include code body for child symbols.\nGot: {}",
            text
        );

        Ok(())
    }
}
