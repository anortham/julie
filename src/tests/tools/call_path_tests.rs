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

// Regression test for the BFS edge-filter fix. The Rust extractor emits an
// `Implements` relationship for `impl Trait for Type`. Before the fix, BFS
// traversed every RelationshipKind, so an Implements edge would produce a
// 1-hop "path" from the type to the trait, answering "does Worker call Doer?"
// with a false yes. After the fix, BFS only walks Calls / Instantiates /
// Overrides, so Implements must not produce a reachable path.
//
// This test will FAIL if the `.retain()` filter inside bfs_shortest_path is
// removed: the Implements edge would re-appear at depth 1 and found=true.
#[tokio::test(flavor = "multi_thread")]
async fn test_non_call_edge_not_traversed() -> Result<()> {
    // Worker implements Doer via `impl Doer for Worker`. The extractor emits
    // an Implements relationship (Worker -> Doer). Worker does NOT call Doer.
    let source = "pub trait Doer {\n    fn act(&self);\n}\n\npub struct Worker;\n\nimpl Doer for Worker {\n    fn act(&self) {}\n}\n";
    let (_temp_dir, handler) = setup_indexed_workspace(source).await?;

    let tool = CallPathTool {
        from: "Worker".to_string(),
        to: "Doer".to_string(),
        max_hops: 4,
        workspace: Some("primary".to_string()),
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    let response: CallPathResponse = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("call_path must return valid JSON (err={e}, text={text})"));
    assert!(
        !response.found,
        "Implements edge must not produce a call-graph path: {response:?}"
    );
    assert_eq!(response.hops, 0);
    assert!(response.path.is_empty());

    Ok(())
}

// Workspace isolation: call_path scopes its search to the specified workspace DB.
// Specifying a non-existent workspace ID must not fall through to primary symbols.
#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_workspace_isolation() -> Result<()> {
    let source = "pub fn alpha() {\n    beta();\n}\npub fn beta() {}\n";
    let (_temp_dir, handler) = setup_indexed_workspace(source).await?;

    let tool = CallPathTool {
        from: "alpha".to_string(),
        to: "beta".to_string(),
        max_hops: 4,
        workspace: Some("nonexistent-workspace-id".to_string()),
    };

    // call_tool may propagate Err when workspace resolution fails outright,
    // or return Ok with an error-message string. Either is correct behavior.
    // The key guarantee: it must NOT return found=true via primary-workspace symbols.
    match tool.call_tool(&handler).await {
        Err(_) => {
            // Workspace not found — isolation enforced at the routing layer.
        }
        Ok(result) => {
            let text = extract_text(&result);
            let found_via_wrong_workspace = serde_json::from_str::<CallPathResponse>(&text)
                .map(|r| r.found)
                .unwrap_or(false);
            assert!(
                !found_via_wrong_workspace,
                "call_path must not traverse primary symbols when a different workspace is specified: {text}"
            );
        }
    }

    Ok(())
}
