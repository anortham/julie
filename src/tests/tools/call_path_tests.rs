use anyhow::Result;
use std::fs;

use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::navigation::call_path::{
    CallPathHop, CallPathResponse, CallPathTool, edge_label,
};
use julie_extractors::RelationshipKind;
use julie_test_support::FakeToolContext;
use tempfile::TempDir;

async fn setup_indexed_workspace(content: &str) -> Result<(TempDir, FakeToolContext)> {
    setup_indexed_workspace_files(&[("src/lib.rs", content)]).await
}

async fn setup_indexed_workspace_files(
    files: &[(&str, &str)],
) -> Result<(TempDir, FakeToolContext)> {
    let temp_dir = TempDir::new()?;
    for (path, content) in files {
        let full_path = temp_dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(full_path, content)?;
    }
    let context = snapshot_context(temp_dir.path())?;
    Ok((temp_dir, context))
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
    try_parse_response(text)
        .unwrap_or_else(|| panic!("call_path should return compact text: {text}"))
}

fn try_parse_response(text: &str) -> Option<CallPathResponse> {
    let mut lines = text.lines();
    let header = lines.next()?.trim();
    let mut found = None;
    let mut hops = None;
    for part in header.split_whitespace() {
        if let Some(value) = part.strip_prefix("found=") {
            found = value.parse::<bool>().ok();
        } else if let Some(value) = part.strip_prefix("hops=") {
            hops = value.parse::<u32>().ok();
        }
    }

    let mut path = Vec::new();
    let mut diagnostic = None;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("diagnostic: ") {
            diagnostic = Some(value.to_string());
            continue;
        }

        let (_, hop) = line.split_once(". ")?;
        let (from, rest) = hop.split_once(" --")?;
        let (edge, rest) = rest.split_once("--> ")?;
        let (to, rest) = rest.split_once(" at ")?;
        let (file, target) = rest
            .split_once(" -> ")
            .map_or((rest, None), |(file, target)| (file, Some(target)));
        let (target_file, target_start_line) = target
            .and_then(|location| {
                let (file, line) = location.rsplit_once(':')?;
                Some((file.to_string(), line.parse::<u32>().ok()?))
            })
            .unwrap_or_default();

        path.push(CallPathHop {
            from: from.to_string(),
            to: to.to_string(),
            edge: edge.to_string(),
            file: file.to_string(),
            target_file,
            target_start_line,
        });
    }

    Some(CallPathResponse {
        found: found?,
        hops: hops?,
        path,
        diagnostic,
        ..Default::default()
    })
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
        ..Default::default()
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
        "validation failure should be compact text, not plain error text: {missing_input_text}"
    );

    let lookup_tool = CallPathTool {
        from: "missing_symbol".to_string(),
        to: "start".to_string(),
        max_hops: 4,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
        ..Default::default()
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
        "lookup failure should be compact text, not plain error text: {lookup_text}"
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
        ..Default::default()
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
async fn test_call_path_drops_ambiguous_call_instead_of_picking_by_storage_order() -> Result<()> {
    let (_temp_dir, handler) = setup_indexed_workspace_files(&[
        (
            "src/start.rs",
            "pub fn manual_start() {\n    manual_goal();\n}\n",
        ),
        ("src/a.rs", "pub fn manual_goal() {}\n"),
        ("src/z.rs", "pub fn manual_goal() {}\n"),
    ])
    .await?;

    let tool = CallPathTool {
        from: "manual_start".to_string(),
        to: "manual_goal".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("src/start.rs".to_string()),
        to_file_path: None,
        ..Default::default()
    };

    let result = tool.call_tool(&handler).await?;
    let response = parse_response(&extract_text(&result));
    assert!(
        !response.found,
        "a call that two same-priority definitions could satisfy is dropped, never picked by storage order: {response:?}"
    );
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
        ..Default::default()
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
async fn test_call_path_renders_compact_text_instead_of_json() -> Result<()> {
    let source = "pub fn start() {\n    leaf();\n}\n\npub fn leaf() {}\n";
    let (_temp_dir, handler) = setup_indexed_workspace(source).await?;

    let tool = CallPathTool {
        from: "start".to_string(),
        to: "leaf".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
        ..Default::default()
    };

    let text = extract_text(&tool.call_tool(&handler).await?);

    assert!(
        !text.trim_start().starts_with('{'),
        "call_path should return compact text, not JSON: {text}"
    );
    assert!(
        text.contains("found=true hops=1"),
        "missing compact header: {text}"
    );
    assert!(
        text.contains("1. start --call--> leaf"),
        "missing hop: {text}"
    );
    assert!(
        !text.contains("\"from\"") && !text.contains("\"target_file\""),
        "JSON keys should not appear in compact output: {text}"
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
    };

    let result = tool.call_tool(&handler).await?;
    let response = parse_response(&extract_text(&result));
    assert!(
        !response.found,
        "Implements edge must not produce a call-graph path: {response:?}"
    );
    assert_eq!(response.hops, 0);
    assert!(response.path.is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "the graph resolves call identifiers by leaf name: a crate-scoped call to one of two same-named functions is ambiguous, and `HashMap::new()` binds to a same-file `new` (Task 2 gap, see task-6 report)"]
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
        ..Default::default()
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
        ..Default::default()
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
async fn test_call_path_resolves_reexported_crate_call_to_definition_target() -> Result<()> {
    let (_temp_dir, handler) = setup_indexed_workspace_files(&[
        (
            "src/lib.rs",
            "pub mod extractors;\npub mod indexing;\npub mod pipeline;\n",
        ),
        (
            "src/extractors.rs",
            "pub use crate::pipeline::extract_canonical;\n",
        ),
        (
            "src/indexing.rs",
            "pub fn extract_symbols_static() {\n    crate::extractors::extract_canonical();\n}\n",
        ),
        ("src/pipeline.rs", "pub fn extract_canonical() {}\n"),
    ])
    .await?;

    let tool = CallPathTool {
        from: "extract_symbols_static".to_string(),
        to: "extract_canonical".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("src/indexing.rs".to_string()),
        to_file_path: Some("src/pipeline.rs".to_string()),
        ..Default::default()
    };

    let result = tool.call_tool(&handler).await?;
    let response = parse_response(&extract_text(&result));

    assert!(
        response.found,
        "re-exported crate call should resolve to the definition target: {response:?}"
    );
    assert_eq!(response.hops, 1);
    assert_eq!(
        response.path,
        vec![CallPathHop {
            from: "extract_symbols_static".to_string(),
            to: "extract_canonical".to_string(),
            edge: "call".to_string(),
            file: "src/indexing.rs:2".to_string(),
            target_file: "src/pipeline.rs".to_string(),
            target_start_line: 1,
        }]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_resolves_workspace_crate_glob_reexport_to_definition_target() -> Result<()>
{
    let (_temp_dir, handler) = setup_indexed_workspace_files(&[
        ("src/lib.rs", "pub mod extractors;\npub mod indexing;\n"),
        ("src/extractors/mod.rs", "pub use sample_lib::*;\n"),
        (
            "src/indexing.rs",
            "pub fn extract_symbols_static() {\n    crate::extractors::extract_canonical();\n}\n",
        ),
        (
            "crates/sample-lib/src/lib.rs",
            "pub use pipeline::extract_canonical;\npub mod pipeline;\n",
        ),
        (
            "crates/sample-lib/src/pipeline.rs",
            "pub fn extract_canonical() {}\n",
        ),
    ])
    .await?;

    let tool = CallPathTool {
        from: "extract_symbols_static".to_string(),
        to: "extract_canonical".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("src/indexing.rs".to_string()),
        to_file_path: Some("crates/sample-lib/src/pipeline.rs".to_string()),
        ..Default::default()
    };

    let result = tool.call_tool(&handler).await?;
    let response = parse_response(&extract_text(&result));

    assert!(
        response.found,
        "workspace crate glob re-export should resolve to the definition target: {response:?}"
    );
    assert_eq!(response.hops, 1);
    assert_eq!(
        response.path,
        vec![CallPathHop {
            from: "extract_symbols_static".to_string(),
            to: "extract_canonical".to_string(),
            edge: "call".to_string(),
            file: "src/indexing.rs:2".to_string(),
            target_file: "crates/sample-lib/src/pipeline.rs".to_string(),
            target_start_line: 1,
        }]
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
        ..Default::default()
    };

    let result = tool.call_tool(&handler).await?;
    let response = parse_response(&extract_text(&result));
    let hop = &response.path[0];

    assert_eq!(hop.to, "leaf");
    assert_eq!(hop.target_file, "src/lib.rs");
    assert_eq!(hop.target_start_line, 5);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_call_path_without_a_snapshot_reports_a_diagnostic() -> Result<()> {
    let handler = FakeToolContext::new();

    let tool = CallPathTool {
        from: "alpha".to_string(),
        to: "beta".to_string(),
        max_hops: 4,
        workspace: Some("nonexistent-workspace-id".to_string()),
        from_file_path: None,
        to_file_path: None,
        ..Default::default()
    };

    let text = extract_text(&tool.call_tool(&handler).await?);
    let response = parse_response(&text);
    assert!(
        !response.found,
        "call_path must not report a path when the workspace has no snapshot: {text}"
    );
    assert!(
        response
            .diagnostic
            .as_deref()
            .unwrap_or_default()
            .contains("Workspace resolution failed"),
        "expected a workspace diagnostic: {text}"
    );

    Ok(())
}

#[test]
fn test_edge_label_exhaustive_over_traversed_kinds() {
    assert_eq!(edge_label(&RelationshipKind::Calls), "call");
    assert_eq!(edge_label(&RelationshipKind::Instantiates), "construct");
    assert_eq!(edge_label(&RelationshipKind::Overrides), "dispatch");
}
