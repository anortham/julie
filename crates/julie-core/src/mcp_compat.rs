//! MCP Compatibility Layer
//!
//! This module provides compatibility types and helpers for rmcp 3.0.1,
//! bridging older Content/TextContent APIs to modern ContentBlock models
//! and exposing protocol versioning helpers.

pub use rmcp::model::{
    CallToolResponse, CallToolResult, ContentBlock, ErrorCode, ErrorData, ProtocolVersion,
    ResultType,
};
pub use schemars::JsonSchema;

/// Compatibility alias: `Content` was unified into `ContentBlock` in rmcp 3.0.0+.
pub type Content = ContentBlock;

/// Compatibility alias: `TextContent` mapped to `ContentBlock`.
pub type TextContent = ContentBlock;

/// Helper to create text content (compatibility with old TextContent::from pattern).
pub fn text_content(text: impl Into<String>) -> Content {
    ContentBlock::text(text)
}

/// Helper to extract text from a ContentBlock.
pub fn extract_text_from_content(block: &ContentBlock) -> Option<&str> {
    match block {
        ContentBlock::Text(t) => Some(&t.text),
        _ => None,
    }
}

/// Trait allowing functions to view either CallToolResult or CallToolResponse as CallToolResult.
pub trait AsCallToolResult {
    fn as_call_tool_result(&self) -> &CallToolResult;
}

impl AsCallToolResult for CallToolResult {
    fn as_call_tool_result(&self) -> &CallToolResult {
        self
    }
}

impl AsCallToolResult for CallToolResponse {
    fn as_call_tool_result(&self) -> &CallToolResult {
        match self {
            CallToolResponse::Complete(r) => r,
            _ => panic!("expected Complete, got non-complete response"),
        }
    }
}

/// Helper to extract all concatenated text from a CallToolResult or CallToolResponse.
pub fn call_tool_result_text(result: &impl AsCallToolResult) -> String {
    result
        .as_call_tool_result()
        .content
        .iter()
        .filter_map(extract_text_from_content)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extension trait on CallToolResponse for extracting the underlying CallToolResult.
pub trait CallToolResponseExt {
    fn into_result(self) -> CallToolResult;
    fn as_result(&self) -> &CallToolResult;
}

impl CallToolResponseExt for CallToolResponse {
    fn into_result(self) -> CallToolResult {
        match self {
            CallToolResponse::Complete(r) => r,
            _ => panic!("expected Complete, got non-complete response"),
        }
    }

    fn as_result(&self) -> &CallToolResult {
        match self {
            CallToolResponse::Complete(r) => r,
            _ => panic!("expected Complete, got non-complete response"),
        }
    }
}

/// Return modern protocol version (2026-07-28).
pub fn modern_protocol_version() -> ProtocolVersion {
    ProtocolVersion::V_2026_07_28
}

/// Return supported legacy protocol version (2025-11-25).
pub fn legacy_protocol_version() -> ProtocolVersion {
    ProtocolVersion::V_2025_11_25
}

/// Extension trait to make Content behave like old TextContent with `from()` pattern.
pub trait TextContentCompat {
    fn from_text(text: impl Into<String>) -> Self;
}

impl TextContentCompat for ContentBlock {
    fn from_text(text: impl Into<String>) -> Self {
        ContentBlock::text(text)
    }
}

/// Extension trait for CallToolResult to provide compatibility with old API.
pub trait CallToolResultExt {
    /// Create a successful result with text content.
    fn text_content(contents: Vec<Content>) -> CallToolResult;
    /// Create a successful result from a single string.
    fn from_text(text: impl Into<String>) -> CallToolResult;
}

impl CallToolResultExt for CallToolResult {
    fn text_content(contents: Vec<Content>) -> CallToolResult {
        CallToolResult::success(contents)
    }

    fn from_text(text: impl Into<String>) -> CallToolResult {
        CallToolResult::success(vec![ContentBlock::text(text)])
    }
}
