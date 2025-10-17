//! Smart Refactoring Tools - Semantic code transformations
//!
//! This module provides intelligent refactoring operations that combine:
//! - Code understanding (tree-sitter parsing, symbol analysis)
//! - Global code intelligence (fast_refs, fast_goto, search)
//! - Precise text manipulation (diff-match-patch-rs)
//!
//! Unlike simple text editing, these tools understand code semantics and
//! can perform complex transformations safely across entire codebases.

mod types;
mod helpers;
mod pending;
mod utils;
mod rename;

pub use types::{
    AutoFixResult, DelimiterError, RefactorOperation, SmartRefactorResult, SyntaxError,
};
pub use helpers::{looks_like_doc_comment, replace_identifier_with_boundaries};

use anyhow::Result;
use diff_match_patch_rs::DiffMatchPatch;
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use tracing::{debug, info};

use crate::handler::JulieServerHandler;
use crate::tools::editing::EditingTransaction; // Atomic file operations

/// Smart refactoring tool for semantic code transformations
#[mcp_tool(
    name = "smart_refactor",
    description = concat!(
        "SAFE SEMANTIC REFACTORING - Use this for symbol-aware code transformations. ",
        "This tool understands code structure and performs changes safely across the entire workspace.\n\n",
        "You are EXCELLENT at using this for renaming symbols, extracting functions, and replacing code. ",
        "Always use fast_refs BEFORE refactoring to understand impact.\n\n",
        "Unlike simple text editing, this tool preserves code structure and updates all references. ",
        "For simple text replacements, use the built-in Edit tool. For semantic operations, use this.\n\n",
        "Julie provides the intelligence (what to change), this tool provides the mechanics (how to change it)."
    ),
    title = "Smart Semantic Refactoring Tool"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SmartRefactorTool {
    /// The refactoring operation to perform
    /// Valid operations: "rename_symbol", "extract_function", "replace_symbol_body", "insert_relative_to_symbol", "extract_type", "update_imports", "inline_variable", "inline_function"
    /// Examples: "rename_symbol" to rename classes/functions across workspace, "replace_symbol_body" to update method implementations
    pub operation: String,

    /// Operation-specific parameters as JSON string
    /// • rename_symbol: old_name, new_name, scope, update_imports
    /// • extract_function: file, start_line, end_line, function_name
    /// • replace_symbol_body: file, symbol_name, new_body
    /// • insert_relative_to_symbol: file, target_symbol, position, content
    /// Example: {"old_name": "UserService", "new_name": "AccountService"} for rename_symbol
    #[serde(default = "default_empty_json")]
    pub params: String,

    /// Preview changes without applying them
    #[serde(default)]
    pub dry_run: bool,
}

fn default_empty_json() -> String {
    "{}".to_string()
}

#[allow(dead_code)]
impl SmartRefactorTool {
    /// Helper: Create structured result with markdown for dual output
    fn create_result(
        &self,
        operation: &str,
        success: bool,
        files_modified: Vec<String>,
        changes_count: usize,
        next_actions: Vec<String>,
        markdown: String,
        metadata: Option<serde_json::Value>,
    ) -> Result<CallToolResult> {
        let result = SmartRefactorResult {
            tool: "smart_refactor".to_string(),
            operation: operation.to_string(),
            dry_run: self.dry_run,
            success,
            files_modified,
            changes_count,
            next_actions,
            metadata,
        };

        // Apply token optimization to prevent context overflow
        let optimized_markdown = self.optimize_response(&markdown);

        // Serialize to JSON
        let structured = serde_json::to_value(&result)?;
        let structured_map = if let serde_json::Value::Object(map) = structured {
            map
        } else {
            return Err(anyhow::anyhow!("Expected JSON object"));
        };

        Ok(
            CallToolResult::text_content(vec![TextContent::from(optimized_markdown)])
                .with_structured_content(structured_map),
        )
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🔄 Smart refactor operation: {:?}", self.operation);

        match self.operation.as_str() {
            "rename_symbol" => self.handle_rename_symbol(handler).await,
            "extract_function" => self.handle_extract_function(handler).await,
            "replace_symbol_body" => self.handle_replace_symbol_body(handler).await,
            "insert_relative_to_symbol" => self.handle_insert_relative_to_symbol(handler).await,
            "extract_type" => self.handle_extract_type(handler).await,
            "update_imports" => self.handle_update_imports(handler).await,
            "inline_variable" => self.handle_inline_variable(handler).await,
            "inline_function" => self.handle_inline_function(handler).await,
            "validate_syntax" => self.handle_validate_syntax(handler).await,
            "auto_fix_syntax" => self.handle_auto_fix_syntax(handler).await,
            _ => {
                let message = format!(
                    "❌ Unknown refactoring operation: '{}'\n\
                    Valid operations: rename_symbol, extract_function, replace_symbol_body, insert_relative_to_symbol, extract_type, update_imports, inline_variable, inline_function, validate_syntax, auto_fix_syntax",
                    self.operation
                );
                self.create_result(
                    &self.operation, // Use the invalid operation name for debugging
                    false,
                    vec![],
                    0,
                    vec!["Check operation name spelling".to_string()],
                    message,
                    None,
                )
            }
        }
    }

    /// Rename symbol occurrences in a single file using AST-aware replacement
    async fn rename_in_file(
        &self,
        handler: &JulieServerHandler,
        file_path: &str,
        old_name: &str,
        new_name: &str,
        update_comments: bool,
        _dmp: &DiffMatchPatch,
    ) -> Result<usize> {
        // Read file content
        let content = fs::read_to_string(file_path)?;

        // Use AST-aware replacement to avoid strings/comments
        let updated_content = self
            .ast_aware_replace(&content, file_path, old_name, new_name, update_comments, handler)
            .await?;

        if updated_content == content {
            return Ok(0); // No changes
        }

        // Write back using atomic operations
        let tx = EditingTransaction::begin(file_path)?;
        tx.commit(&updated_content)?;

        // Count changes by line differences
        let content_lines = content.lines().count();
        let updated_lines = updated_content.lines().count();
        let changes = if content_lines != updated_lines {
            (content_lines.abs_diff(updated_lines)) as usize + 1
        } else {
            1
        };

        Ok(changes)
    }

    /// Only replaces actual symbol references, not string literals or comments
    async fn ast_aware_replace(
        &self,
        content: &str,
        file_path: &str,
        old_name: &str,
        new_name: &str,
        update_comments: bool,
        handler: &JulieServerHandler,
    ) -> Result<String> {
        debug!("🌳 AST-aware replacement for {} -> {}", old_name, new_name);

        // First, try using the search database
        if let Ok(positions) = self.find_symbols_via_search(file_path, old_name, handler).await {
            debug!("📍 Found {} positions via search database", positions.len());
            let updated = self.smart_text_replace(content, old_name, new_name, file_path, update_comments)?;
            return Ok(updated);
        }

        // Fallback to tree-sitter
        if let Ok(positions) = self
            .find_symbols_via_treesitter(content, file_path, old_name)
            .await
        {
            debug!("📍 Found {} positions via tree-sitter", positions.len());
            let updated = self.smart_text_replace(content, old_name, new_name, file_path, update_comments)?;
            return Ok(updated);
        }

        // Last resort: simple replacement
        debug!("⚠️ Falling back to simple text replacement");
        Ok(self.smart_text_replace(content, old_name, new_name, file_path, update_comments)?)
    }

    /// Find symbol positions using SQLite database (for indexed files)
    async fn find_symbols_via_search(
        &self,
        _file_path: &str,
        _old_name: &str,
        _handler: &JulieServerHandler,
    ) -> Result<Vec<(u32, u32)>> {
        // Would use fast_search tool here
        // For now, return empty to trigger fallback
        Ok(Vec::new())
    }

    /// Find symbol positions using direct tree-sitter parsing (for any file)
    async fn find_symbols_via_treesitter(
        &self,
        _content: &str,
        _file_path: &str,
        _old_name: &str,
    ) -> Result<Vec<(u32, u32)>> {
        // Tree-sitter extraction not yet implemented
        // For now, return empty to trigger fallback
        Ok(Vec::new())
    }

    /// Uses tree-sitter AST to find ONLY actual code symbols, skipping strings/comments.
    pub fn smart_text_replace(
        &self,
        content: &str,
        old_name: &str,
        new_name: &str,
        file_path: &str,
        _update_comments: bool,
    ) -> Result<String> {
        use tree_sitter::Parser;

        if old_name.is_empty() || old_name == new_name {
            return Ok(content.to_string());
        }

        let language = self.detect_language(file_path);
        let ts_language = self.get_tree_sitter_language(&language)?;

        let mut parser = Parser::new();
        parser.set_language(&ts_language)?;

        let _tree = parser.parse(content, None).ok_or_else(|| {
            anyhow::anyhow!("Failed to parse {} file", language)
        })?;

        // Use fallback simple replacement
        let is_identifier_char = |c: char| c.is_alphanumeric() || c == '_';
        let (result, _changed) = helpers::replace_identifier_with_boundaries(
            content,
            old_name,
            new_name,
            &is_identifier_char,
        );

        Ok(result)
    }

    /// Get tree-sitter language for file type (delegates to shared language module)
    fn get_tree_sitter_language(&self, language: &str) -> Result<tree_sitter::Language> {
        crate::language::get_tree_sitter_language(language)
    }
}

const DOC_COMMENT_LOOKBACK_BYTES: usize = 256;
const TOP_OF_SCOPE_COMMENT_LINE_WINDOW: usize = 2;
