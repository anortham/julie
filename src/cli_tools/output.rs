//! Formatting CLI tool outputs for stdout.
//!
//! Formats tool outputs as JSON envelopes, text, or markdown based on user preferences.
//! All logs, diagnostics, and errors should be written to stderr; stdout is reserved
//! for machine-readable envelopes or user-facing formatted results.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;

use crate::cli_tools::CliToolOutput;
use crate::cli_tools::subcommands::OutputFormat;
use crate::request_engine::{RequestFailure, ToolReply};

// ---------------------------------------------------------------------------
// Standardized JSON Envelopes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliSuccessEnvelope {
    pub schema_version: u32,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub reply: ToolReply,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFailureEnvelope {
    pub schema_version: u32,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub error: CliErrorDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliErrorDetails {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Value,
}

impl CliSuccessEnvelope {
    pub fn new(request_id: Option<String>, reply: ToolReply) -> Self {
        Self {
            schema_version: 1,
            ok: true,
            request_id,
            reply,
        }
    }
}

impl CliFailureEnvelope {
    pub fn new(request_id: Option<String>, failure: &RequestFailure) -> Self {
        Self {
            schema_version: 1,
            ok: false,
            request_id,
            error: CliErrorDetails {
                code: failure.code.clone(),
                message: failure.message.clone(),
                retryable: failure.retryable,
                details: failure.details.clone(),
            },
        }
    }
}

pub fn format_success_envelope(request_id: Option<String>, reply: &ToolReply) -> String {
    let envelope = CliSuccessEnvelope::new(request_id, reply.clone());
    serde_json::to_string(&envelope).unwrap_or_default()
}

pub fn format_failure_envelope(request_id: Option<String>, failure: &RequestFailure) -> String {
    let envelope = CliFailureEnvelope::new(request_id, failure);
    serde_json::to_string(&envelope).unwrap_or_default()
}

/// Resolve process exit code from a request execution result.
pub fn resolve_exit_code(result: &Result<ToolReply, RequestFailure>) -> i32 {
    match result {
        Ok(reply) => {
            if reply.is_error() {
                3
            } else {
                0
            }
        }
        Err(failure) => failure.exit_code(),
    }
}

/// Safe stdout writer that terminates with exit 1 on broken pipe.
pub fn write_stdout_safe(content: &str) {
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{}", content).is_err() {
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Legacy / Human Output Formatters
// ---------------------------------------------------------------------------

/// Format a tool output according to the requested format.
pub fn format_output(output: &CliToolOutput, format: OutputFormat, tool_name: &str) -> String {
    match format {
        OutputFormat::Text => format_text(output),
        OutputFormat::Json => format_json(output),
        OutputFormat::Markdown => format_markdown(output, tool_name),
    }
}

/// Text formatter: extracts text content from CallToolResult if available,
/// falling back to pretty-printed JSON.
fn format_text(output: &CliToolOutput) -> String {
    extract_text_items(&output.result)
        .unwrap_or_else(|| serde_json::to_string_pretty(&output.result).unwrap_or_default())
}

/// JSON formatter: pretty-printed JSON of the raw result.
fn format_json(output: &CliToolOutput) -> String {
    serde_json::to_string_pretty(&output.result).unwrap_or_default()
}

/// Markdown formatter: wraps output in a fenced code block with tool name header.
fn format_markdown(output: &CliToolOutput, tool_name: &str) -> String {
    let body = extract_text_items(&output.result)
        .unwrap_or_else(|| serde_json::to_string_pretty(&output.result).unwrap_or_default());

    let mut out = format!("# {}\n\n```\n", tool_name);
    out.push_str(&body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```\n");
    out
}

/// Extract text items from a serialized `CallToolResult`.
fn extract_text_items(result: &Value) -> Option<String> {
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
    use crate::request_engine::RequestReadiness;
    use std::path::PathBuf;

    fn make_output(result: Value) -> CliToolOutput {
        CliToolOutput {
            mode: CliExecutionMode::Standalone,
            workspace_root: PathBuf::from("/tmp/test"),
            result,
            is_error: false,
        }
    }

    fn success_result(text: &str) -> Value {
        serde_json::json!({
            "content": [
                { "type": "text", "text": text }
            ]
        })
    }

    fn error_result(text: &str) -> Value {
        serde_json::json!({
            "content": [
                { "type": "text", "text": text }
            ],
            "isError": true
        })
    }

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

    #[test]
    fn test_json_format_produces_valid_json() {
        let result = success_result("search results here");
        let output = make_output(result.clone());
        let formatted = format_output(&output, OutputFormat::Json, "fast_search");

        let parsed: Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed, result);
    }

    #[test]
    fn test_json_format_preserves_is_error_field() {
        let result = error_result("something went wrong");
        let output = make_output(result);
        let formatted = format_output(&output, OutputFormat::Json, "fast_search");

        let parsed: Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed["isError"], serde_json::json!(true));
        assert_eq!(parsed["content"][0]["text"], "something went wrong");
    }

    #[test]
    fn test_json_format_is_pretty_printed() {
        let result = success_result("test");
        let output = make_output(result);
        let formatted = format_output(&output, OutputFormat::Json, "test_tool");

        assert!(formatted.contains('\n'));
        assert!(formatted.contains("  "));
    }

    #[test]
    fn test_markdown_format_has_header_and_fenced_block() {
        let output = make_output(success_result("search output here"));
        let formatted = format_output(&output, OutputFormat::Markdown, "fast_search");

        assert!(formatted.starts_with("# fast_search\n"));
        assert!(formatted.contains("```\n"));
        assert!(formatted.contains("search output here"));
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

        let expected = "# test_tool\n\n```\nline1\nline2\nline3\n```\n";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_markdown_format_body_without_trailing_newline() {
        let output = make_output(success_result("no trailing newline"));
        let formatted = format_output(&output, OutputFormat::Markdown, "test_tool");

        assert!(formatted.contains("no trailing newline\n```\n"));
    }

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
        let parsed: Value = serde_json::from_str(&formatted).unwrap();
        assert_eq!(parsed["isError"], true);
    }

    #[test]
    fn test_cli_success_and_failure_envelopes() {
        let reply = ToolReply::from_result(
            "fast_search",
            Some("ws-123".to_string()),
            serde_json::json!({"content": [{"type": "text", "text": "found"}]}),
            RequestReadiness::ready(crate::request_engine::SemanticMode::Auto),
        );
        let success_json = format_success_envelope(Some("req-1".to_string()), &reply);
        let parsed_success: Value = serde_json::from_str(&success_json).unwrap();
        assert_eq!(parsed_success["schema_version"], 1);
        assert_eq!(parsed_success["ok"], true);
        assert_eq!(parsed_success["request_id"], "req-1");
        assert_eq!(parsed_success["reply"]["tool"], "fast_search");

        let failure = RequestFailure::invalid_arguments("test invalid");
        let failure_json = format_failure_envelope(Some("req-2".to_string()), &failure);
        let parsed_failure: Value = serde_json::from_str(&failure_json).unwrap();
        assert_eq!(parsed_failure["schema_version"], 1);
        assert_eq!(parsed_failure["ok"], false);
        assert_eq!(parsed_failure["request_id"], "req-2");
        assert_eq!(parsed_failure["error"]["code"], "INVALID_ARGUMENTS");
    }

    #[test]
    fn test_resolve_exit_code() {
        let reply_ok = ToolReply::from_result(
            "test",
            None,
            serde_json::json!({}),
            RequestReadiness::ready(crate::request_engine::SemanticMode::Auto),
        );
        assert_eq!(resolve_exit_code(&Ok(reply_ok)), 0);

        let reply_err = ToolReply::from_result(
            "test",
            None,
            serde_json::json!({"isError": true}),
            RequestReadiness::ready(crate::request_engine::SemanticMode::Auto),
        );
        assert_eq!(resolve_exit_code(&Ok(reply_err)), 3);

        assert_eq!(
            resolve_exit_code(&Err(RequestFailure::invalid_arguments("bad"))),
            2
        );
        assert_eq!(
            resolve_exit_code(&Err(RequestFailure::tool_error("failed"))),
            3
        );
        assert_eq!(
            resolve_exit_code(&Err(RequestFailure::follower_read_only("ro"))),
            4
        );
        assert_eq!(
            resolve_exit_code(&Err(RequestFailure::stale_edit("conflict"))),
            5
        );
        assert_eq!(
            resolve_exit_code(&Err(RequestFailure::deadline_exceeded("timeout"))),
            124
        );
        assert_eq!(
            resolve_exit_code(&Err(RequestFailure::cancelled("cancelled"))),
            130
        );
    }
}
