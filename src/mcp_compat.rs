//! MCP Compatibility Layer
//!
//! This module provides compatibility types and helpers for the migration
//! from rust-mcp-sdk to rmcp (official Anthropic SDK).

pub use rmcp::model::{CallToolResult, Content};
pub use schemars::JsonSchema;

/// Type alias for backward compatibility (TextContent -> Content)
pub type TextContent = Content;

/// Helper to create text content (compatibility with old TextContent::from pattern)
pub fn text_content(text: impl Into<String>) -> Content {
    Content::text(text)
}

/// Extension trait to make Content behave like old TextContent with `from()` pattern
pub trait TextContentCompat {
    fn from_text(text: impl Into<String>) -> Self;
}

impl TextContentCompat for Content {
    fn from_text(text: impl Into<String>) -> Self {
        Content::text(text)
    }
}

/// Extension trait for CallToolResult to provide compatibility with old API
pub trait CallToolResultExt {
    /// Create a successful result with text content (compatibility with old text_content method)
    fn text_content(contents: Vec<Content>) -> CallToolResult;
}

impl CallToolResultExt for CallToolResult {
    fn text_content(contents: Vec<Content>) -> CallToolResult {
        CallToolResult::success(contents)
    }
}

