use anyhow::Result;
use std::fs;

use crate::database::FileInfo;
use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
use crate::handler::JulieServerHandler;
use crate::tools::navigation::call_path::{
    CallPathHop, CallPathResponse, CallPathTool, edge_label,
};
use crate::tools::workspace::ManageWorkspaceTool;
use tempfile::TempDir;

async fn setup_indexed_workspace(content: &str) -> Result<(TempDir, JulieServerHandler)> {
    setup_indexed_workspace_files(&[("src/lib.rs", content)]).await
}

async fn setup_indexed_workspace_files(
    files: &[(&str, &str)],
) -> Result<(TempDir, JulieServerHandler)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    for (path, content) in files {
        let full_path = workspace_path.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(full_path, content)?;
    }

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

fn make_call_path_symbol(id: &str, name: &str, file_path: &str, start_line: u32) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line,
        start_column: 0,
        end_line: start_line,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        parent_id: None,
        signature: Some(format!("fn {name}()")),
        doc_comment: None,
        visibility: Some(Visibility::Public),
        metadata: None,
        semantic_group: None,
        confidence: Some(0.9),
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    }
}

fn make_call_path_relationship(id: &str, from: &str, to: &str) -> Relationship {
    Relationship {
        id: id.to_string(),
        from_symbol_id: from.to_string(),
        to_symbol_id: to.to_string(),
        kind: RelationshipKind::Calls,
        file_path: "src/start.rs".to_string(),
        line_number: 10,
        confidence: 1.0,
        metadata: None,
    }
}

fn make_call_path_file(path: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash-{path}"),
        size: 100,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 1,
        line_count: 20,
        content: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_failures_return_structured_response() -> Result<()> {
    let source = "pub fn start() {}\n";
    let (_temp_dir, handler) = setup_indexed_workspace(source).await?;

    let missing_input_tool = CallPathTool {
        from: "start".to_string(),
        to: String::new(),
        max_hops: 4,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
    };
    let missing_input_text = extract_text(&missing_input_tool.call_tool(&handler).await?);
    let missing_input = parse_response(&missing_input_text);
    assert!(
        !missing_input.found,
        "validation failure should return found=false: {missing_input:?}"
    );
    assert_eq!(missing_input.hops, 0);
    assert!(missing_input.path.is_empty());
    assert!(
        missing_input
            .diagnostic
            .as_deref()
            .unwrap_or_default()
            .contains("from"),
        "validation diagnostic should name missing endpoint: {missing_input:?}"
    );
    assert!(
        !missing_input_text.starts_with("Error:"),
        "validation failure should be JSON, not plain text: {missing_input_text}"
    );

    let lookup_tool = CallPathTool {
        from: "missing_symbol".to_string(),
        to: "start".to_string(),
        max_hops: 4,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
    };
    let lookup_text = extract_text(&lookup_tool.call_tool(&handler).await?);
    let lookup = parse_response(&lookup_text);
    assert!(
        !lookup.found,
        "lookup failure should return found=false: {lookup:?}"
    );
    assert_eq!(lookup.hops, 0);
    assert!(lookup.path.is_empty());
    assert!(
        lookup
            .diagnostic
            .as_deref()
            .unwrap_or_default()
            .contains("not found"),
        "lookup diagnostic should explain resolution failure: {lookup:?}"
    );
    assert!(
        !lookup_text.starts_with("Error:"),
        "lookup failure should be JSON, not plain text: {lookup_text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_rejects_max_hops_above_cap() -> Result<()> {
    let source = "pub fn start() {}\n";
    let (_temp_dir, handler) = setup_indexed_workspace(source).await?;

    let tool = CallPathTool {
        from: "start".to_string(),
        to: "start".to_string(),
        max_hops: 10_000,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
    };

    let text = extract_text(&tool.call_tool(&handler).await?);
    let response = parse_response(&text);
    assert!(
        !response.found,
        "hop cap violation should not search: {response:?}"
    );
    assert_eq!(response.hops, 0);
    assert!(response.path.is_empty());
    let diagnostic = response.diagnostic.as_deref().unwrap_or_default();
    assert!(
        diagnostic.contains("max_hops") && diagnostic.contains("1") && diagnostic.contains("32"),
        "diagnostic should document accepted hop range: {diagnostic}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_tie_ordering_uses_target_identity() -> Result<()> {
    let (_temp_dir, handler) = setup_indexed_workspace("pub fn placeholder() {}\n").await?;

    {
        let snapshot = handler.primary_workspace_snapshot().await?;
        let mut db = snapshot.database.lock().expect("primary database lock");
        for path in ["src/start.rs", "src/a.rs", "src/z.rs"] {
            db.store_file_info(&make_call_path_file(path))?;
        }
        db.store_symbols(&[
            make_call_path_symbol("sym-start", "manual_start", "src/start.rs", 1),
            make_call_path_symbol("target-a", "manual_goal", "src/a.rs", 3),
            make_call_path_symbol("target-z", "manual_goal", "src/z.rs", 7),
        ])?;
        db.store_relationships(&[
            make_call_path_relationship("rel-z", "sym-start", "target-z"),
            make_call_path_relationship("rel-a", "sym-start", "target-a"),
        ])?;
    }

    let tool = CallPathTool {
        from: "manual_start".to_string(),
        to: "manual_goal".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("src/start.rs".to_string()),
        to_file_path: None,
    };

    let result = tool.call_tool(&handler).await?;
    let response_json: serde_json::Value = serde_json::from_str(&extract_text(&result))?;
    assert_eq!(response_json["found"], true);
    assert_eq!(
        response_json["path"][0]["target_file"].as_str(),
        Some("src/a.rs"),
        "same-score ties should be resolved by target identity, not storage order"
    );
    assert_eq!(
        response_json["path"][0]["target_start_line"].as_u64(),
        Some(3)
    );

    Ok(())
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
        from_file_path: None,
        to_file_path: None,
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
                target_file: "src/lib.rs".to_string(),
                target_start_line: 5,
            },
            CallPathHop {
                from: "middle".to_string(),
                to: "leaf".to_string(),
                edge: "call".to_string(),
                file: "src/lib.rs:6".to_string(),
                target_file: "src/lib.rs".to_string(),
                target_start_line: 9,
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
        from_file_path: None,
        to_file_path: None,
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
        from_file_path: None,
        to_file_path: None,
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
        from_file_path: None,
        to_file_path: None,
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

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_resolves_rust_crate_scoped_call_to_namespaced_target() -> Result<()> {
    let (_temp_dir, handler) = setup_indexed_workspace_files(&[
        (
            "src/main.rs",
            "mod search;\nmod other;\n\npub fn caller() {\n    crate::search::hybrid::should_use_semantic_fallback();\n}\n\npub fn std_caller() {\n    std::collections::HashMap::new();\n}\n\npub fn new() {}\n",
        ),
        ("src/search/mod.rs", "pub mod hybrid;\n"),
        (
            "src/search/hybrid.rs",
            "pub fn should_use_semantic_fallback() {}\n",
        ),
        (
            "src/other.rs",
            "pub fn should_use_semantic_fallback() {}\n",
        ),
    ])
    .await?;

    let tool = CallPathTool {
        from: "caller".to_string(),
        to: "should_use_semantic_fallback".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("src/main.rs".to_string()),
        to_file_path: Some("src/search/hybrid.rs".to_string()),
    };

    let result = tool.call_tool(&handler).await?;
    let response = parse_response(&extract_text(&result));

    assert!(
        response.found,
        "crate-scoped call should resolve to the namespaced target: {response:?}"
    );
    assert_eq!(response.hops, 1);
    assert_eq!(
        response.path,
        vec![CallPathHop {
            from: "caller".to_string(),
            to: "should_use_semantic_fallback".to_string(),
            edge: "call".to_string(),
            file: "src/main.rs:5".to_string(),
            target_file: "src/search/hybrid.rs".to_string(),
            target_start_line: 1,
        }]
    );

    let std_tool = CallPathTool {
        from: "std_caller".to_string(),
        to: "new".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("src/main.rs".to_string()),
        to_file_path: Some("src/main.rs".to_string()),
    };

    let std_result = std_tool.call_tool(&handler).await?;
    let std_response = parse_response(&extract_text(&std_result));

    assert!(
        !std_response.found,
        "std::collections::HashMap::new() must not create a call path to local new: {std_response:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_hops_include_target_definition_identity() -> Result<()> {
    let source = "pub fn start() {\n    leaf();\n}\n\npub fn leaf() {}\n";
    let (_temp_dir, handler) = setup_indexed_workspace(source).await?;

    let tool = CallPathTool {
        from: "start".to_string(),
        to: "leaf".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
    };

    let result = tool.call_tool(&handler).await?;
    let response_json: serde_json::Value = serde_json::from_str(&extract_text(&result))?;
    let hop = response_json["path"][0]
        .as_object()
        .expect("first hop should be an object");

    assert_eq!(
        hop.get("to").and_then(serde_json::Value::as_str),
        Some("leaf")
    );
    assert_eq!(
        hop.get("target_file").and_then(serde_json::Value::as_str),
        Some("src/lib.rs")
    );
    assert_eq!(
        hop.get("target_start_line")
            .and_then(serde_json::Value::as_u64),
        Some(5)
    );

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
        from_file_path: None,
        to_file_path: None,
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

#[test]
fn test_edge_label_exhaustive_over_traversed_kinds() {
    assert_eq!(edge_label(&RelationshipKind::Calls), "call");
    assert_eq!(edge_label(&RelationshipKind::Instantiates), "construct");
    assert_eq!(edge_label(&RelationshipKind::Overrides), "dispatch");
}
