//! Golden master tests for the edit_file tool.

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::editing::edit_file::{EditFileTool, apply_edit};
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture_source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/editing/sources")
        .join(name)
}

fn fixture_control(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/editing/controls/edit-file")
        .join(name)
}

fn load(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e))
}

fn extract_text(result: &CallToolResult) -> String {
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

#[test]
fn test_exact_replace() {
    let source = load(&fixture_source("dmp_rust_module.rs"));
    let expected = load(&fixture_control("rust_exact_replace.rs"));

    let result = apply_edit(
        &source,
        "pub fn get_user(&self, id: u64) -> Option<&String> {",
        "pub fn get_user(&self, id: u64) -> Result<&String, NotFoundError> {",
        "first",
    )
    .expect("Edit should succeed");

    assert_eq!(
        result, expected,
        "Output should match golden master (exact replace)"
    );
}

#[test]
fn test_replace_all_occurrences() {
    let source = load(&fixture_source("dmp_rust_module.rs"));
    let expected = load(&fixture_control("rust_replace_all.rs"));

    let result = apply_edit(&source, "(&self", "(&mut self", "all").expect("Edit should succeed");

    assert_eq!(
        result, expected,
        "Output should match golden master (replace all)"
    );
}

#[test]
fn test_markdown_edit() {
    let source = load(&fixture_source("dmp_markdown_doc.md"));
    let expected = load(&fixture_control("markdown_edit.md"));

    let old_text = "Add advanced features and testing.\n\n- Task C: Integration tests\n- Task D: Performance tuning";
    let new_text = "Redesigned to focus on security hardening.\n\n- Task C: Security audit\n- Task D: Penetration testing\n- Task E: Fix vulnerabilities";

    let result = apply_edit(&source, old_text, new_text, "first").expect("Edit should succeed");

    assert_eq!(
        result, expected,
        "Output should match golden master (markdown edit)"
    );
}

#[test]
fn test_no_match_returns_error() {
    let source = "fn main() {}\n";
    let result = apply_edit(source, "fn nonexistent()", "fn replacement()", "first");
    assert!(result.is_err(), "Should return error when no match found");
}

#[test]
fn test_empty_old_text_returns_error() {
    let result = apply_edit("some content", "", "replacement", "first");
    assert!(result.is_err(), "Should return error for empty old_text");
}

#[test]
fn test_replace_last_occurrence() {
    let source = "aaa bbb aaa bbb aaa";
    let result = apply_edit(source, "aaa", "ccc", "last").unwrap();
    assert_eq!(result, "aaa bbb aaa bbb ccc");
}

#[test]
fn test_invalid_occurrence_returns_error() {
    let result = apply_edit("content", "con", "new", "invalid");
    assert!(result.is_err());
}

// --- Trimmed-line fuzzy matching tests ---

#[test]
fn test_fuzzy_indentation_difference() {
    // File uses 4-space indent, old_text uses 2-space. Should match via trimmed lines.
    let content = "fn main() {\n    let x = 1;\n    let y = 2;\n}\n";
    let old_text = "  let x = 1;\n  let y = 2;";
    let new_text = "    let x = 10;\n    let y = 20;";
    let result = apply_edit(content, old_text, new_text, "first").unwrap();
    assert_eq!(result, "fn main() {\n    let x = 10;\n    let y = 20;\n}\n");
}

#[test]
fn test_fuzzy_long_single_line_wider_indent() {
    // Single line >32 chars, 8-space indent in old_text vs 4-space in file.
    // 8-space is NOT a substring of 4-space content, so exact fails and trimmed matches.
    let content = "    some_function_with_a_very_long_name(param1, param2, param3);\n";
    let old_text = "        some_function_with_a_very_long_name(param1, param2, param3);";
    let new_text = "    some_function_with_a_very_long_name(param1, param2, param3, param4);";
    let result = apply_edit(content, old_text, new_text, "first").unwrap();
    assert_eq!(
        result,
        "    some_function_with_a_very_long_name(param1, param2, param3, param4);\n"
    );
}

#[test]
fn test_fuzzy_trailing_whitespace_difference() {
    // File has trailing spaces on line 1, old_text doesn't.
    let content = "let x = 1;  \nlet y = 2;\n";
    let old_text = "let x = 1;\nlet y = 2;";
    let new_text = "let x = 10;\nlet y = 20;";
    let result = apply_edit(content, old_text, new_text, "first").unwrap();
    assert_eq!(result, "let x = 10;\nlet y = 20;\n");
}

#[test]
fn test_fuzzy_tabs_vs_spaces() {
    // File uses tabs, old_text uses spaces.
    let content = "\tfn process() {\n\t\tdo_work();\n\t}\n";
    let old_text = "    fn process() {\n        do_work();\n    }";
    let new_text = "\tfn process_v2() {\n\t\tdo_work();\n\t}";
    let result = apply_edit(content, old_text, new_text, "first").unwrap();
    assert_eq!(result, "\tfn process_v2() {\n\t\tdo_work();\n\t}\n");
}

#[test]
fn test_fuzzy_no_match_still_errors() {
    // Completely different content should still fail.
    let content = "fn main() {\n    let x = 1;\n}\n";
    let old_text = "fn nonexistent() {\n    something_else();\n}";
    let result = apply_edit(content, old_text, "fn replaced() {}", "first");
    assert!(result.is_err(), "Should error when no lines match");
}

#[test]
fn test_dmp_fuzzy_handles_extra_char_in_content() {
    // Content has an extra space ("let x  = 1;" is 11 chars vs old_text's 10).
    // DMP bitap finds the match, but splice must replace 11 chars, not 10.
    let content = "let x  = 1;\nmore stuff\n";
    let old_text = "let x = 1;";
    let new_text = "let x = 2;";
    let result = apply_edit(content, old_text, new_text, "first").unwrap();
    assert_eq!(result, "let x = 2;\nmore stuff\n");
}

#[test]
fn test_dmp_fuzzy_handles_missing_char_in_content() {
    // Content has a missing space ("letx = 1;" is 9 chars vs old_text's 10).
    let content = "letx = 1;\nmore stuff\n";
    let old_text = "let x = 1;";
    let new_text = "let y = 2;";
    let result = apply_edit(content, old_text, new_text, "first").unwrap();
    assert_eq!(result, "let y = 2;\nmore stuff\n");
}

#[test]
fn test_fuzzy_overlapping_spans_not_corrupted() {
    // Repeated trimmed-equal lines create overlapping window matches.
    // occurrence="all" must not produce overlapping spans.
    let content = "  x\n  x\n  x\n";
    let old_text = "x\nx";
    let new_text = "Z";
    let result = apply_edit(content, old_text, new_text, "all").unwrap();
    // Should replace first match (lines 0-1) only; second would overlap so it's skipped.
    assert_eq!(result, "Z\n  x\n");
}

#[test]
fn test_fuzzy_crlf_line_endings_preserved() {
    // Trimmed-line matching on CRLF files must not eat the \r.
    let content = "  let x = 1;\r\n  let y = 2;\r\n";
    let old_text = "let x = 1;\nlet y = 2;";
    let new_text = "let x = 10;\r\nlet y = 20;";
    let result = apply_edit(content, old_text, new_text, "first").unwrap();
    assert_eq!(result, "let x = 10;\r\nlet y = 20;\r\n");
}

#[test]
fn test_exact_match_still_preferred() {
    // When exact match works, it should be used (no behavior change).
    let content = "    let x = 1;\n    let y = 2;\n";
    let old_text = "    let x = 1;\n    let y = 2;";
    let new_text = "    let x = 10;\n    let y = 20;";
    let result = apply_edit(content, old_text, new_text, "first").unwrap();
    assert_eq!(result, "    let x = 10;\n    let y = 20;\n");
}

/// DMP bitap loop must make forward progress on every iteration.
/// Without the `end.max(pos + 1)` guard, if compute_fuzzy_end returns `pos`
/// (possible when pos is at the tail of content and the window is empty),
/// the loop would hang.  This test verifies "all" occurrence replacement
/// on a short pattern completes and produces the correct output.
#[test]
fn test_dmp_loop_forward_progress_multiple_short_matches() {
    // Pattern is <=32 chars (triggers DMP bitap path). Three occurrences.
    let content = "abc xyz abc xyz abc";
    let result = apply_edit(content, "abc", "ZZZ", "all").unwrap();
    assert_eq!(result, "ZZZ xyz ZZZ xyz ZZZ");
}

/// DMP bitap at the very end of content: position near tail where the window
/// would be empty. The guard must prevent the loop from re-visiting the same position.
#[test]
fn test_dmp_loop_forward_progress_match_near_tail() {
    let content = "long prefix text then xy";
    // "xy" is short (<=32 chars), match is at tail of content
    let result = apply_edit(content, "xy", "AB", "first").unwrap();
    assert_eq!(result, "long prefix text then AB");
}

// Finding #30: editing tools have no `workspace` field but serde silently
// accepts unknown fields by default, so `edit_file(..., workspace="x")`
// silently ignores `workspace` and edits against primary. Tighten with
// #[serde(deny_unknown_fields)] so the silent-ignore becomes a loud error.
#[test]
fn test_edit_file_rejects_unknown_workspace_field() {
    let json = serde_json::json!({
        "file_path": "src/foo.rs",
        "old_text": "old",
        "new_text": "new",
        "workspace": "secondary-id",
    });
    let result: Result<crate::tools::editing::edit_file::EditFileTool, _> =
        serde_json::from_value(json);
    let err = result.expect_err("unknown `workspace` field should be rejected");
    assert!(
        err.to_string().contains("workspace"),
        "error should mention the offending field name, got: {err}"
    );
}

#[test]
fn test_edit_file_accepts_known_fields() {
    // Sanity: deny_unknown_fields must not reject known fields.
    let json = serde_json::json!({
        "file_path": "src/foo.rs",
        "old_text": "old",
        "new_text": "new",
        "dry_run": true,
        "occurrence": "first",
    });
    serde_json::from_value::<crate::tools::editing::edit_file::EditFileTool>(json)
        .expect("known fields must still parse");
}

#[tokio::test]
async fn test_edit_file_dry_run_truncates_large_diff_preview() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir)?;
    let file_path = src_dir.join("large.rs");

    let original = (0..90u32)
        .map(|i| format!("let value_{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let replacement = (0..90u32)
        .map(|i| format!("let value_{i} = {};", i + 900))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&file_path, &original)?;

    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let tool = EditFileTool {
        file_path: "src/large.rs".to_string(),
        old_text: original.trim_end().to_string(),
        new_text: replacement,
        dry_run: true,
        occurrence: "first".to_string(),
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);

    assert!(
        text.contains("Dry run preview"),
        "dry-run response should include preview header, got: {text}"
    );
    assert!(
        text.contains("diff lines omitted") && text.contains("full diff has"),
        "large dry-run diff should include a line-count summary, got: {text}"
    );
    assert!(
        text.lines().count() < 90,
        "dry-run response should be capped, got {} lines",
        text.lines().count()
    );
    assert!(
        !text.contains("-let value_60 = 60;"),
        "middle removed lines should be omitted from capped preview, got: {text}"
    );
    assert!(
        !text.contains("+let value_30 = 930;"),
        "middle added lines should be omitted from capped preview, got: {text}"
    );
    assert!(
        text.contains("+let value_89 = 989;"),
        "tail of the diff should remain visible, got: {text}"
    );
    assert_eq!(
        fs::read_to_string(&file_path)?,
        original,
        "dry-run must not modify the file"
    );

    Ok(())
}
