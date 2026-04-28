use anyhow::Result;
use std::fs;

use crate::handler::JulieServerHandler;
use crate::tools::navigation::call_path::{CallPathResponse, CallPathTool};
use crate::tools::navigation::resolution::file_path_matches_suffix;
use crate::tools::workspace::ManageWorkspaceTool;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn setup_multi_file_workspace(
    files: &[(&str, &str)],
) -> Result<(TempDir, JulieServerHandler)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    for (relative_path, content) in files {
        let full_path = workspace_path.join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_path, content)?;
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
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn try_parse_response(text: &str) -> Option<CallPathResponse> {
    serde_json::from_str(text).ok()
}

#[test]
fn test_file_path_suffix_filter_normalizes_user_input() {
    let path = "src/lib.rs";

    for filter in ["src/lib.rs", "./src/lib.rs", "src\\lib.rs", "src//lib.rs"] {
        assert!(
            file_path_matches_suffix(path, filter),
            "expected normalized filter {filter:?} to match {path:?}"
        );
    }

    assert!(file_path_matches_suffix("lib.rs", "lib.rs"));
    assert!(file_path_matches_suffix(
        "src/tools/handler.rs",
        "handler.rs"
    ));
    assert!(!file_path_matches_suffix(
        "src/tools/foohandler.rs",
        "handler.rs"
    ));
}

// ---------------------------------------------------------------------------
// Case 1: from_file_path disambiguates an ambiguous `from` symbol
// ---------------------------------------------------------------------------
//
// Two files each define a `process` function. Without a hint both are
// returned and call_path errors with "ambiguous". With from_file_path
// pointing at src/a.rs the correct symbol is selected and the path is found.

#[tokio::test(flavor = "multi_thread")]
async fn test_disambiguation_from_file_path() -> Result<()> {
    let files = &[
        (
            "src/a.rs",
            "pub fn process() { helper(); }\npub fn helper() {}\n",
        ),
        ("src/b.rs", "pub fn process() {}\n"),
    ];
    let (_temp_dir, handler) = setup_multi_file_workspace(files).await?;

    // Without hint: should error with "ambiguous"
    let ambiguous_tool = CallPathTool {
        from: "process".to_string(),
        to: "helper".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
    };
    let text = extract_text(&ambiguous_tool.call_tool(&handler).await?);
    assert!(
        text.contains("ambiguous"),
        "expected ambiguity error without hint, got: {text}"
    );

    // With from_file_path: should resolve and find the 1-hop path
    let tool = CallPathTool {
        from: "process".to_string(),
        to: "helper".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("src/a.rs".to_string()),
        to_file_path: None,
    };
    let text = extract_text(&tool.call_tool(&handler).await?);
    let response =
        try_parse_response(&text).unwrap_or_else(|| panic!("expected JSON response, got: {text}"));

    assert!(
        response.found,
        "path should be found via from_file_path disambiguation: {response:?}"
    );
    assert_eq!(response.hops, 1);

    Ok(())
}

// ---------------------------------------------------------------------------
// Case 2: both from_file_path and to_file_path disambiguate simultaneously
// ---------------------------------------------------------------------------
//
// Two files each define `entry` and `target`. Without hints both endpoints
// are ambiguous. With both file-path params the path is found.

#[tokio::test(flavor = "multi_thread")]
async fn test_disambiguation_both_file_paths() -> Result<()> {
    let files = &[
        (
            "src/a.rs",
            "pub fn entry() { target(); }\npub fn target() {}\n",
        ),
        (
            "src/b.rs",
            "pub fn entry() { other(); }\npub fn other() {}\n",
        ),
    ];
    let (_temp_dir, handler) = setup_multi_file_workspace(files).await?;

    // Without hints: from is ambiguous
    let ambiguous_tool = CallPathTool {
        from: "entry".to_string(),
        to: "target".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
    };
    let text = extract_text(&ambiguous_tool.call_tool(&handler).await?);
    assert!(
        text.contains("ambiguous"),
        "expected ambiguity error without hints, got: {text}"
    );

    // With both file paths: should resolve and find path
    let tool = CallPathTool {
        from: "entry".to_string(),
        to: "target".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("src/a.rs".to_string()),
        to_file_path: Some("src/a.rs".to_string()),
    };
    let text = extract_text(&tool.call_tool(&handler).await?);
    let response =
        try_parse_response(&text).unwrap_or_else(|| panic!("expected JSON response, got: {text}"));

    assert!(
        response.found,
        "expected path with both file hints: {response:?}"
    );
    assert_eq!(response.hops, 1);
    assert_eq!(response.path[0].file, "src/a.rs:1");

    Ok(())
}

// ---------------------------------------------------------------------------
// Case 2b: multiple `to` candidates do not require disambiguation when only one
// is reachable from `from`
// ---------------------------------------------------------------------------
//
// Two files each define a `result` function. `from` is unambiguous (only in
// src/a.rs), but `to` has two candidates. The reachable one in src/a.rs should
// be enough for path search to succeed. to_file_path can still pin the same
// destination explicitly.

#[tokio::test(flavor = "multi_thread")]
async fn test_disambiguation_to_file_path() -> Result<()> {
    let files = &[
        (
            "src/a.rs",
            "pub fn runner() { result(); }\npub fn result() {}\n",
        ),
        ("src/b.rs", "pub fn result() {}\n"),
    ];
    let (_temp_dir, handler) = setup_multi_file_workspace(files).await?;

    // Without hint: call_path should accept multiple `to` candidates and find
    // the reachable one.
    let tool_without_hint = CallPathTool {
        from: "runner".to_string(),
        to: "result".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
    };
    let text = extract_text(&tool_without_hint.call_tool(&handler).await?);
    let response =
        try_parse_response(&text).unwrap_or_else(|| panic!("expected JSON response, got: {text}"));
    assert!(
        response.found,
        "reachable destination should not require to_file_path: {response:?}"
    );
    assert_eq!(response.hops, 1);
    assert_eq!(response.path[0].file, "src/a.rs:1");

    // With to_file_path: should resolve and find the 1-hop path
    let tool = CallPathTool {
        from: "runner".to_string(),
        to: "result".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: Some("src/a.rs".to_string()),
    };
    let text = extract_text(&tool.call_tool(&handler).await?);
    let response =
        try_parse_response(&text).unwrap_or_else(|| panic!("expected JSON response, got: {text}"));

    assert!(
        response.found,
        "path should be found via to_file_path disambiguation: {response:?}"
    );
    assert_eq!(response.hops, 1);

    Ok(())
}

// ---------------------------------------------------------------------------
// Case 2e: still-ambiguous after filter (filter matches multiple files)
// ---------------------------------------------------------------------------
//
// Two files both named handler.rs in different directories each define
// `process`. The filter "handler.rs" matches both (each path ends with
// /handler.rs), so the error should still report "ambiguous" rather than
// silently picking one.

#[tokio::test(flavor = "multi_thread")]
async fn test_disambiguation_still_ambiguous_after_filter() -> Result<()> {
    let files = &[
        (
            "src/a/handler.rs",
            "pub fn process() { step(); }\npub fn step() {}\n",
        ),
        ("src/b/handler.rs", "pub fn process() {}\n"),
    ];
    let (_temp_dir, handler) = setup_multi_file_workspace(files).await?;

    // "handler.rs" matches both src/a/handler.rs and src/b/handler.rs — still ambiguous
    let tool = CallPathTool {
        from: "process".to_string(),
        to: "step".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("handler.rs".to_string()),
        to_file_path: None,
    };
    let text = extract_text(&tool.call_tool(&handler).await?);
    assert!(
        text.contains("ambiguous"),
        "filter matching two files should still report ambiguous, got: {text}"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Case 3: substring false-positive rejection
// ---------------------------------------------------------------------------
//
// Only src/tools/foohandler.rs exists. from_file_path="handler.rs" must NOT
// match "foohandler.rs" via substring (the slash-boundary rule rejects it).
// The resolver should return "not found" rather than a false match.

#[tokio::test(flavor = "multi_thread")]
async fn test_disambiguation_no_substring_false_positive() -> Result<()> {
    let files = &[(
        "src/tools/foohandler.rs",
        "pub fn process() { end_step(); }\npub fn end_step() {}\n",
    )];
    let (_temp_dir, handler) = setup_multi_file_workspace(files).await?;

    // "handler.rs" ends_with of "foohandler.rs" would match via bare ends_with,
    // but must NOT match with the slash-boundary rule.
    let tool = CallPathTool {
        from: "process".to_string(),
        to: "end_step".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: Some("handler.rs".to_string()),
        to_file_path: None,
    };
    let text = extract_text(&tool.call_tool(&handler).await?);

    // Should not produce a valid found=true response
    let found_false_positive = try_parse_response(&text).map(|r| r.found).unwrap_or(false);
    assert!(
        !found_false_positive,
        "handler.rs must not match foohandler.rs via substring: {text}"
    );
    // The error should mention "not found" (filter reduced matches to zero)
    assert!(
        text.contains("not found") || text.contains("Error"),
        "expected not-found error for mismatched filter, got: {text}"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Case 4: multi-segment qualified name (MyStruct::my_method)
// ---------------------------------------------------------------------------
//
// Qualified name resolution via "Parent::child" notation is supported by
// find_symbol's Step 2 path. Verify that a method on a struct can be used as
// the `from` endpoint of a call_path query.

#[tokio::test(flavor = "multi_thread")]
async fn test_qualified_name_struct_method() -> Result<()> {
    let source = "pub struct Processor;\n\
                  impl Processor {\n\
                      pub fn run(&self) { do_work(); }\n\
                  }\n\
                  pub fn do_work() {}\n";
    let files = &[("src/lib.rs", source)];
    let (_temp_dir, handler) = setup_multi_file_workspace(files).await?;

    let tool = CallPathTool {
        from: "Processor::run".to_string(),
        to: "do_work".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
    };
    let text = extract_text(&tool.call_tool(&handler).await?);
    let response = try_parse_response(&text)
        .unwrap_or_else(|| panic!("expected JSON response for qualified name, got: {text}"));

    assert!(
        response.found,
        "qualified name Processor::run should resolve and find path to do_work: {response:?}"
    );
    assert_eq!(response.hops, 1);

    Ok(())
}

// ---------------------------------------------------------------------------
// Case 5: trait-impl qualified name
// ---------------------------------------------------------------------------
//
#[tokio::test(flavor = "multi_thread")]
async fn test_trait_impl_qualified_name_limitation() -> Result<()> {
    let source = "pub trait Runnable { fn run(&self); }\n\
                  pub struct Worker;\n\
                  impl Runnable for Worker {\n\
                      fn run(&self) { inner(); }\n\
                  }\n\
                  pub fn inner() {}\n";
    let files = &[("src/lib.rs", source)];
    let (_temp_dir, handler) = setup_multi_file_workspace(files).await?;

    let tool = CallPathTool {
        from: "Worker::run".to_string(),
        to: "inner".to_string(),
        max_hops: 2,
        workspace: Some("primary".to_string()),
        from_file_path: None,
        to_file_path: None,
    };
    let text = extract_text(&tool.call_tool(&handler).await?);
    let response =
        try_parse_response(&text).unwrap_or_else(|| panic!("expected JSON response, got: {text}"));

    assert!(
        response.found,
        "Worker::run should resolve via trait-impl qualified lookup: {response:?}"
    );

    Ok(())
}
