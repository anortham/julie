use anyhow::Result;
use std::fs;

use crate::handler::JulieServerHandler;
use crate::tools::navigation::call_path::{CallPathHop, CallPathResponse, CallPathTool};
use crate::tools::workspace::ManageWorkspaceTool;
use tempfile::TempDir;

async fn setup_indexed_workspace(content: &str) -> Result<(TempDir, JulieServerHandler)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src"))?;
    fs::write(workspace_path.join("src").join("lib.rs"), content)?;

    let handler = JulieServerHandler::new(workspace_path.clone()).await?;
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(workspace_path.to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    Ok((temp_dir, handler))
}

fn extract_text(result: &crate::mcp_compat::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| {
            serde_json::to_value(block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(|text| text.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_response(text: &str) -> CallPathResponse {
    serde_json::from_str(text).expect("call_path should return valid JSON")
}

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_finds_shortest_call_chain() -> Result<()> {
    let source = "pub fn start() {\n    middle();\n}\n\npub fn middle() {\n    leaf();\n}\n\npub fn leaf() {}\n";
    let (_temp_dir, handler) = setup_indexed_workspace(source).await?;

    let tool = CallPathTool {
        from: "start".to_string(),
        to: "leaf".to_string(),
        max_hops: 4,
        workspace: Some("primary".to_string()),
    };

    let result = tool.call_tool(&handler).await?;
    let response = parse_response(&extract_text(&result));

    assert!(response.found, "expected path to be found: {response:?}");
    assert_eq!(response.hops, 2);
    assert_eq!(
        response.path,
        vec![
            CallPathHop {
                from: "start".to_string(),
                to: "middle".to_string(),
                edge: "call".to_string(),
                file: "src/lib.rs:2".to_string(),
            },
            CallPathHop {
                from: "middle".to_string(),
                to: "leaf".to_string(),
                edge: "call".to_string(),
                file: "src/lib.rs:6".to_string(),
            },
        ]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_returns_found_false_when_no_path_exists() -> Result<()> {
    let source = "pub fn start() {\n    middle();\n}\n\npub fn middle() {\n    leaf();\n}\n\npub fn leaf() {}\n\npub fn lonely() {}\n";
    let (_temp_dir, handler) = setup_indexed_workspace(source).await?;

    let tool = CallPathTool {
        from: "lonely".to_string(),
        to: "leaf".to_string(),
        max_hops: 4,
        workspace: Some("primary".to_string()),
    };

    let result = tool.call_tool(&handler).await?;
    let response = parse_response(&extract_text(&result));

    assert!(!response.found, "expected no path: {response:?}");
    assert_eq!(response.hops, 0);
    assert!(response.path.is_empty());
    assert!(
        response
            .diagnostic
            .as_deref()
            .unwrap_or_default()
            .contains("No path found"),
        "expected no-path diagnostic: {response:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_respects_max_hops() -> Result<()> {
    let source = "pub fn start() {\n    middle();\n}\n\npub fn middle() {\n    leaf();\n}\n\npub fn leaf() {}\n";
    let (_temp_dir, handler) = setup_indexed_workspace(source).await?;

    let tool = CallPathTool {
        from: "start".to_string(),
        to: "leaf".to_string(),
        max_hops: 1,
        workspace: Some("primary".to_string()),
    };

    let result = tool.call_tool(&handler).await?;
    let response = parse_response(&extract_text(&result));

    assert!(!response.found, "path should be capped out: {response:?}");
    assert!(
        response
            .diagnostic
            .as_deref()
            .unwrap_or_default()
            .contains("within 1 hops"),
        "diagnostic should mention hop cap: {response:?}"
    );

    Ok(())
}
