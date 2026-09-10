//! `FastRefsTool` over the primary snapshot: definition and caller listing,
//! source-name resolution, qualified lookups, and per-line deduplication.

use std::fs;

use anyhow::Result;
use julie_test_support::FakeToolContext;
use tempfile::TempDir;

use crate::mcp_compat::CallToolResult;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::navigation::FastRefsTool;

const REBOUND: &str = "pub fn rebound_primary_symbol() {}\n\npub fn rebound_primary_caller() {\n    rebound_primary_symbol();\n}\n";
const FAST_REFS_TOOL: &str =
    "pub struct FastRefsTool {}\nimpl FastRefsTool {\n    pub fn call_tool() {}\n}\n";
const OTHER_TOOL: &str =
    "pub struct OtherTool {}\nimpl OtherTool {\n    pub fn call_tool() {}\n}\n";

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn context(files: &[(&str, &str)]) -> Result<(TempDir, FakeToolContext)> {
    let dir = TempDir::new()?;
    for (path, content) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap())?;
        fs::write(full, content)?;
    }
    let context = snapshot_context(dir.path())?;
    Ok((dir, context))
}

fn fast_refs(symbol: &str, limit: u32, reference_kind: Option<&str>) -> FastRefsTool {
    FastRefsTool {
        symbol: symbol.to_string(),
        include_definition: true,
        limit,
        workspace: Some("primary".to_string()),
        reference_kind: reference_kind.map(str::to_string),
        semantics: None,
    }
}

#[tokio::test]
async fn test_fast_refs_primary_uses_rebound_current_primary_store() -> Result<()> {
    let (_dir, context) = context(&[("src/rebound.rs", REBOUND)])?;

    let result = fast_refs("rebound_primary_symbol", 10, None)
        .call_tool(&context)
        .await?;

    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("src/rebound.rs:1") && !result_text.contains("No references found"),
        "fast_refs should use the rebound current-primary store instead of the stale loaded workspace: {result_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_fast_refs_primary_keeps_rebound_source_name_resolution_after_rebind() -> Result<()> {
    let (_dir, context) = context(&[("src/rebound.rs", REBOUND)])?;

    let result = fast_refs("rebound_primary_symbol", 10, Some("call"))
        .call_tool(&context)
        .await?;

    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("rebound_primary_caller (Calls)"),
        "fast_refs should resolve source names from the same rebound primary snapshot used for the main lookup: {result_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_fast_refs_primary_qualified_identifier_fallback_respects_parent_filter() -> Result<()>
{
    let (_dir, context) = context(&[
        ("src/rebound.rs", REBOUND),
        ("src/fast_refs.rs", FAST_REFS_TOOL),
        ("src/other.rs", OTHER_TOOL),
        (
            "src/caller.rs",
            "pub fn caller_fast_refs() {\n    FastRefsTool::call_tool();\n}\n",
        ),
        (
            "src/other_caller.rs",
            "pub fn caller_other() {\n    OtherTool::call_tool();\n}\n",
        ),
    ])?;

    let result = fast_refs("FastRefsTool::call_tool", 10, None)
        .call_tool(&context)
        .await?;

    let result_text = extract_text_from_result(&result);
    assert!(
        result_text.contains("src/caller.rs:")
            && result_text.contains(":2  caller_fast_refs")
            && !result_text.contains("src/other_caller.rs:")
            && !result_text.contains(":2  caller_other"),
        "qualified fast_refs should stay on the matching definition set: {result_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_fast_refs_primary_identifier_fallback_dedupes_within_batch() -> Result<()> {
    let (_dir, context) = context(&[
        ("src/rebound.rs", REBOUND),
        ("src/fast_refs.rs", FAST_REFS_TOOL),
        (
            "src/caller.rs",
            "pub fn caller_fast_refs() {\n    FastRefsTool::call_tool(); FastRefsTool::call_tool();\n    FastRefsTool::call_tool();\n}\n",
        ),
    ])?;

    let result = fast_refs("FastRefsTool::call_tool", 2, None)
        .call_tool(&context)
        .await?;

    let result_text = extract_text_from_result(&result);
    let first_line_count = result_text.matches(":2  caller_fast_refs").count();
    let second_line_count = result_text.matches(":3  caller_fast_refs").count();

    assert_eq!(
        first_line_count, 1,
        "duplicate file:line refs should collapse before limit handling: {result_text}"
    );
    assert_eq!(
        second_line_count, 1,
        "limit should still leave room for the unique line after dedupe: {result_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_fast_refs_lists_a_reexport_line_once() -> Result<()> {
    let (_dir, context) = context(&[
        ("src/lib.rs", "pub mod inner;\npub use inner::Thing;\n"),
        ("src/inner.rs", "pub struct Thing;\n"),
        ("src/user.rs", "use crate::Thing;\nfn build(t: Thing) {}\n"),
    ])?;

    let result = fast_refs("Thing", 10, None).call_tool(&context).await?;

    let result_text = extract_text_from_result(&result);
    let reexport_lines: Vec<&str> = result_text
        .lines()
        .filter(|line| line.contains("src/lib.rs"))
        .collect();
    assert_eq!(
        reexport_lines.len(),
        1,
        "the `pub use` line must be listed once: {result_text}"
    );
    assert!(
        reexport_lines[0].contains("src/lib.rs:2") && reexport_lines[0].contains("Imports"),
        "the re-export is an import reference: {result_text}"
    );
    assert!(
        result_text.contains("src/user.rs:") && result_text.contains("build (Uses)"),
        "the type usage in the importing file is listed: {result_text}"
    );

    Ok(())
}
