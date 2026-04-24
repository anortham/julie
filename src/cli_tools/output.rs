//! Output formatting for CLI tool results.
//!
//! Three modes:
//! - **Text** (default): prints the tool's text payload as-is. Tools already
//!   produce formatted text for MCP clients (terminals), so no transformation
//!   is needed.
//! - **JSON**: pretty-prints the full `CallToolResult` value for piping into
//!   `jq` or other automation.
//! - **Markdown**: wraps the output in report-style headers and fenced code
//!   blocks for documentation or review workflows.

use super::{CliToolOutput, OutputFormat};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Format CLI tool output according to the requested format.
///
/// `tool_name` is used by the markdown formatter as a section header.
/// Pass `command.tool_name()` from the calling site.
pub fn format_output(output: &CliToolOutput, format: OutputFormat, tool_name: &str) -> String {
    match format {
        OutputFormat::Text => format_text(&output.result),
        OutputFormat::Json => format_json(&output.result),
        OutputFormat::Markdown => format_markdown(&output.result, tool_name),
    }
}

// ---------------------------------------------------------------------------
// Text formatter
// ---------------------------------------------------------------------------

/// Extract text content from a serialized `CallToolResult` and return it as-is.
///
/// The result JSON has shape `{ "content": [{ "type": "text", "text": "..." }], ... }`.
/// We concatenate all text items with newlines. If the structure is unexpected
/// (e.g. a raw daemon response), fall back to pretty-printed JSON.
fn format_text(result: &serde_json::Value) -> String {
    extract_text_items(result)
        .unwrap_or_else(|| serde_json::to_string_pretty(result).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// JSON formatter
// ---------------------------------------------------------------------------

/// Pretty-print the full result value for machine consumption.
fn format_json(result: &serde_json::Value) -> String {
    serde_json::to_string_pretty(result).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Markdown formatter
// ---------------------------------------------------------------------------

/// Render the tool result as a markdown report with a header and fenced blocks.
///
/// Structure:
/// ```text
/// # fast_search
///
/// ```
/// <tool output>
/// ```
/// ```
fn format_markdown(result: &serde_json::Value, tool_name: &str) -> String {
    let body = extract_text_items(result)
        .unwrap_or_else(|| serde_json::to_string_pretty(result).unwrap_or_default());

    let mut out = String::with_capacity(tool_name.len() + body.len() + 32);
    out.push_str("# ");
    out.push_str(tool_name);
    out.push_str("\n\n```\n");
    out.push_str(&body);
    // Ensure the fenced block closing is on its own line
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```\n");
    out
}

// ---------------------------------------------------------------------------
// Signals report formatter
// ---------------------------------------------------------------------------

/// Format an early warning signals report for CLI output.
pub fn format_signals_report(
    report: &crate::analysis::EarlyWarningReport,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(report).unwrap_or_default(),
        OutputFormat::Text => format_signals_text(report),
        OutputFormat::Markdown => format_signals_markdown(report),
    }
}

fn format_signals_text(report: &crate::analysis::EarlyWarningReport) -> String {
    let mut out = String::new();
    let s = &report.summary;
    out.push_str(&format!(
        "Early Warning Signals  (entry_points: {}, auth_coverage_candidates: {}, review_markers: {})\n",
        s.entry_points, s.auth_coverage_candidates, s.review_markers
    ));
    if report.from_cache {
        out.push_str("  (from cache)\n");
    }
    out.push('\n');

    if !report.entry_points.is_empty() {
        out.push_str("Entry Points:\n");
        for ep in &report.entry_points {
            out.push_str(&format!(
                "  {} ({}:{}) [{}]\n",
                ep.symbol_name, ep.file_path, ep.start_line, ep.annotation
            ));
        }
        out.push('\n');
    }

    if !report.auth_coverage_candidates.is_empty() {
        out.push_str("Auth Coverage Candidates:\n");
        for ac in &report.auth_coverage_candidates {
            out.push_str(&format!(
                "  {} ({}:{}) [{}]\n",
                ac.symbol_name, ac.file_path, ac.start_line, ac.annotation
            ));
        }
        out.push('\n');
    }

    if !report.review_markers.is_empty() {
        out.push_str("Review Markers:\n");
        for rm in &report.review_markers {
            out.push_str(&format!(
                "  {} ({}:{}) [{}]\n",
                rm.symbol_name, rm.file_path, rm.start_line, rm.annotation
            ));
        }
    }

    out
}

fn format_signals_markdown(report: &crate::analysis::EarlyWarningReport) -> String {
    let mut out = String::new();
    let s = &report.summary;
    out.push_str("# Early Warning Signals\n\n");
    out.push_str(&format!(
        "| Metric | Count |\n|--------|-------|\n| Entry Points | {} |\n| Auth Coverage Candidates | {} |\n| Review Markers | {} |\n\n",
        s.entry_points, s.auth_coverage_candidates, s.review_markers
    ));

    if !report.entry_points.is_empty() {
        out.push_str("## Entry Points\n\n| Symbol | File | Line | Annotation |\n|--------|------|------|------------|\n");
        for ep in &report.entry_points {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                ep.symbol_name, ep.file_path, ep.start_line, ep.annotation
            ));
        }
        out.push('\n');
    }

    if !report.auth_coverage_candidates.is_empty() {
        out.push_str("## Auth Coverage Candidates\n\n| Symbol | File | Line | Annotation |\n|--------|------|------|------------|\n");
        for ac in &report.auth_coverage_candidates {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                ac.symbol_name, ac.file_path, ac.start_line, ac.annotation
            ));
        }
        out.push('\n');
    }

    if !report.review_markers.is_empty() {
        out.push_str("## Review Markers\n\n| Symbol | File | Line | Annotation |\n|--------|------|------|------------|\n");
        for rm in &report.review_markers {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                rm.symbol_name, rm.file_path, rm.start_line, rm.annotation
            ));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Extract text items from a serialized `CallToolResult`.
///
/// Returns `None` if the JSON doesn't have the expected `content` array
/// structure, signaling the caller to fall back to raw JSON output.
fn extract_text_items(result: &serde_json::Value) -> Option<String> {
    let content = result.get("content")?.as_array()?;
    let texts: Vec<&str> = content
        .iter()
        .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
        .collect();

    if texts.is_empty() {
        return None;
    }

    Some(texts.join("\n"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_tools::{CliExecutionMode, CliToolOutput};
    use std::path::PathBuf;

    /// Build a `CliToolOutput` with the given result JSON for testing.
    fn make_output(result: serde_json::Value) -> CliToolOutput {
        CliToolOutput {
            mode: CliExecutionMode::Standalone,
            workspace_root: PathBuf::from("/tmp/test"),
            result,
            is_error: false,
        }
    }

    /// Build a result JSON matching `CallToolResult::success(vec![Content::text(text)])`.
    fn success_result(text: &str) -> serde_json::Value {
        serde_json::json!({
            "content": [
                { "type": "text", "text": text }
            ]
        })
    }

    /// Build an error result JSON.
    fn error_result(text: &str) -> serde_json::Value {
        serde_json::json!({
            "content": [
                { "type": "text", "text": text }
            ],
            "isError": true
        })
    }

    // -- Text formatter tests -------------------------------------------------

    #[test]
    fn test_text_format_extracts_text_content() {
        let output = make_output(success_result("hello world"));
        let formatted = format_output(&output, OutputFormat::Text, "fast_search");
        assert_eq!(formatted, "hello world");
    }

    #[test]
    fn test_text_format_concatenates_multiple_content_items() {
        let result = serde_json::json!({
            "content": [
                { "type": "text", "text": "line one" },
                { "type": "text", "text": "line two" }
            ]
        });
        let output = make_output(result);
        let formatted = format_output(&output, OutputFormat::Text, "test_tool");
        assert_eq!(formatted, "line one\nline two");
    }

    #[test]
    fn test_text_format_falls_back_to_json_on_unexpected_structure() {
        let result = serde_json::json!({ "unexpected": "structure" });
        let output = make_output(result.clone());
        let formatted = format_output(&output, OutputFormat::Text, "test_tool");
        let expected = serde_json::to_string_pretty(&result).unwrap();
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_text_format_falls_back_when_content_has_no_text() {
        let result = serde_json::json!({
            "content": [
                { "type": "image", "data": "base64..." }
            ]
        });
        let output = make_output(result.clone());
        let formatted = format_output(&output, OutputFormat::Text, "test_tool");
        // No text items found, should fall back to pretty JSON
        let expected = serde_json::to_string_pretty(&result).unwrap();
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_text_format_empty_content_array_falls_back() {
        let result = serde_json::json!({ "content": [] });
        let output = make_output(result.clone());
        let formatted = format_output(&output, OutputFormat::Text, "test_tool");
        let expected = serde_json::to_string_pretty(&result).unwrap();
        assert_eq!(formatted, expected);
    }

    // -- JSON formatter tests -------------------------------------------------

    #[test]
    fn test_json_format_produces_valid_json() {
        let result = success_result("search results here");
        let output = make_output(result.clone());
        let formatted = format_output(&output, OutputFormat::Json, "fast_search");

        // Must parse back to the same value
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed, result);
    }

    #[test]
    fn test_json_format_preserves_is_error_field() {
        let result = error_result("something went wrong");
        let output = make_output(result);
        let formatted = format_output(&output, OutputFormat::Json, "fast_search");

        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed["isError"], serde_json::json!(true));
        assert_eq!(parsed["content"][0]["text"], "something went wrong");
    }

    #[test]
    fn test_json_format_is_pretty_printed() {
        let result = success_result("test");
        let output = make_output(result);
        let formatted = format_output(&output, OutputFormat::Json, "test_tool");

        // Pretty-printed JSON has newlines and indentation
        assert!(formatted.contains('\n'));
        assert!(formatted.contains("  "));
    }

    // -- Markdown formatter tests ---------------------------------------------

    #[test]
    fn test_markdown_format_has_header_and_fenced_block() {
        let output = make_output(success_result("search output here"));
        let formatted = format_output(&output, OutputFormat::Markdown, "fast_search");

        assert!(formatted.starts_with("# fast_search\n"));
        assert!(formatted.contains("```\n"));
        assert!(formatted.contains("search output here"));
        // Should end with closing fence
        assert!(formatted.ends_with("```\n"));
    }

    #[test]
    fn test_markdown_format_uses_tool_name_as_header() {
        let output = make_output(success_result("data"));
        let formatted = format_output(&output, OutputFormat::Markdown, "get_symbols");
        assert!(formatted.starts_with("# get_symbols\n"));
    }

    #[test]
    fn test_markdown_format_body_inside_fence() {
        let output = make_output(success_result("line1\nline2\nline3"));
        let formatted = format_output(&output, OutputFormat::Markdown, "test_tool");

        // The body should be between the opening and closing fences
        let expected = "# test_tool\n\n```\nline1\nline2\nline3\n```\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_markdown_format_body_without_trailing_newline() {
        let output = make_output(success_result("no trailing newline"));
        let formatted = format_output(&output, OutputFormat::Markdown, "test_tool");

        // Should add a newline before closing fence
        assert!(formatted.contains("no trailing newline\n```\n"));
    }

    // -- extract_text_items tests ---------------------------------------------

    #[test]
    fn test_extract_text_items_from_valid_result() {
        let result = success_result("hello");
        assert_eq!(extract_text_items(&result), Some("hello".to_string()));
    }

    #[test]
    fn test_extract_text_items_returns_none_for_missing_content() {
        let result = serde_json::json!({ "other": "field" });
        assert_eq!(extract_text_items(&result), None);
    }

    #[test]
    fn test_extract_text_items_returns_none_for_non_array_content() {
        let result = serde_json::json!({ "content": "not an array" });
        assert_eq!(extract_text_items(&result), None);
    }

    #[test]
    fn test_extract_text_items_returns_none_for_empty_content() {
        let result = serde_json::json!({ "content": [] });
        assert_eq!(extract_text_items(&result), None);
    }

    // -- Error output tests ---------------------------------------------------

    #[test]
    fn test_error_result_text_format_extracts_error_message() {
        let output = CliToolOutput {
            mode: CliExecutionMode::Standalone,
            workspace_root: PathBuf::from("/tmp/test"),
            result: error_result("tool failed: invalid query"),
            is_error: true,
        };
        let formatted = format_output(&output, OutputFormat::Text, "fast_search");
        assert_eq!(formatted, "tool failed: invalid query");
    }

    #[test]
    fn test_error_result_json_format_includes_error_flag() {
        let output = CliToolOutput {
            mode: CliExecutionMode::Standalone,
            workspace_root: PathBuf::from("/tmp/test"),
            result: error_result("bad input"),
            is_error: true,
        };
        let formatted = format_output(&output, OutputFormat::Json, "fast_search");
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed["isError"], true);
    }
}
