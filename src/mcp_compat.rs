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

/// Extension trait to add structured content support (compatibility with old API)
pub trait WithStructuredContent {
    /// Add structured JSON content to the result
    fn with_structured_content(self, structured: serde_json::Map<String, serde_json::Value>) -> Self;
}

impl WithStructuredContent for CallToolResult {
    fn with_structured_content(mut self, structured: serde_json::Map<String, serde_json::Value>) -> Self {
        // Convert structured content to JSON text and add to content
        if !structured.is_empty() {
            let json_text = serde_json::to_string_pretty(&serde_json::Value::Object(structured))
                .unwrap_or_else(|_| "{}".to_string());
            // Add the JSON as text content - content is a Vec in rmcp
            self.content.push(Content::text(json_text));
        }
        self
    }
}

/// Extension trait to extract structured content from CallToolResult
/// This provides backward compatibility with tests that used the old structured_content field
pub trait StructuredContentExt {
    /// Get structured content as a JSON Map (compatibility shim for old API)
    /// Attempts to parse the last content item as JSON
    fn structured_content(&self) -> Option<serde_json::Map<String, serde_json::Value>>;
}

impl StructuredContentExt for CallToolResult {
    fn structured_content(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        // Try to find JSON content in the content vec
        for content in self.content.iter().rev() {
            if let Some(text_content) = content.as_text() {
                // Try to parse as JSON object (access .text field on RawTextContent)
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&text_content.text) {
                    return Some(map);
                }
            }
        }
        None
    }
}
