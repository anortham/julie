//! Smart Refactoring Tools - Semantic code transformations
//!
//! This module provides intelligent refactoring operations that combine:
//! - Code understanding (tree-sitter parsing, symbol analysis)
//! - Global code intelligence (fast_refs, fast_goto, search)
//! - Precise text manipulation (diff-match-patch-rs)
//!
//! Unlike simple text editing, these tools understand code semantics and
//! can perform complex transformations safely across entire codebases.

mod helpers;
mod indentation;
mod operations;
mod rename;
mod types;
mod utils;

pub use helpers::{looks_like_doc_comment, replace_identifier_with_boundaries};
pub use types::{
    AutoFixResult, DelimiterError, RefactorOperation, SmartRefactorResult, SyntaxError,
};

use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::handler::JulieServerHandler;
use crate::tools::editing::EditingTransaction;

fn default_dry_run() -> bool {
    true
}

// ===== NEW FOCUSED TOOLS (Phase 2 - Tool Adoption Improvements) =====

/// Edit operation type for EditSymbolTool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditOperation {
    /// Replace function/method body
    ReplaceBody,
    /// Insert code before/after a symbol
    InsertRelative,
    /// Extract symbol to another file
    ExtractToFile,
}

/// Rename a symbol across the entire codebase with workspace-wide updates.
///
/// Uses `fast_refs` to find all references, then applies AST-aware replacement
/// (tree-sitter parsing) to rename only actual code identifiers — string literals
/// and comments are left untouched. Supports scope limiting to a single file.
///
/// **Always use `dry_run=true` first** to preview changes before applying.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RenameSymbolTool {
    /// Current symbol name
    pub old_name: String,
    /// New symbol name
    pub new_name: String,
    /// Scope limitation
    #[serde(default)]
    pub scope: Option<String>,
    /// Preview without applying (default: true)
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}

/// AST-aware symbol editing: replace function/method bodies, insert code before/after
/// symbols, or extract symbols to other files. Finds symbols by name using tree-sitter.
///
/// Three operations:
/// - `replace_body` — rewrite a function's implementation without touching its signature
/// - `insert_relative` — add code adjacent to a symbol (before or after)
/// - `extract_to_file` — move a symbol to another file
///
/// **Always use `dry_run=true` first** to preview changes before applying.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EditSymbolTool {
    /// File path (relative to workspace root)
    pub file_path: String,
    /// Symbol name (function, method, class)
    pub symbol_name: String,
    /// Operation type
    pub operation: EditOperation,
    /// Content to insert or replace
    pub content: String,
    /// Position for insert_relative: "before" or "after" (default: "after")
    #[serde(default)]
    pub position: Option<String>,
    /// Target file for extract_to_file
    #[serde(default)]
    pub target_file: Option<String>,
    /// Preview without applying (default: true)
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}

/// Smart refactoring tool for semantic code transformations
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SmartRefactorTool {
    /// The refactoring operation to perform
    /// Valid operations: "rename_symbol", "replace_symbol_body", "insert_relative_to_symbol", "extract_symbol_to_file"
    /// Examples: "rename_symbol" to rename classes/functions across workspace, "replace_symbol_body" to update method implementations, "extract_symbol_to_file" to move symbols between files
    pub operation: String,

    /// Operation-specific parameters as JSON string
    ///
    /// **replace_symbol_body:**
    /// - file (string, required): Path to file containing the symbol
    /// - symbol_name (string, required): Name of function/method to modify
    /// - new_body (string, required): New body content (indentation will be normalized)
    ///
    /// **insert_relative_to_symbol:**
    /// - file (string, required): Path to file containing the symbol
    /// - target_symbol (string, required): Symbol to insert relative to
    /// - position (string, optional): "before" or "after" (default: "after")
    /// - content (string, required): Code to insert (indentation will be normalized)
    ///
    /// **extract_symbol_to_file:**
    /// - source_file (string, required): File containing symbol to extract
    /// - target_file (string, required): Destination file (created if doesn't exist)
    /// - symbol_name (string, required): Symbol to extract
    /// - update_imports (bool, optional): Add import statement to source (default: false)
    ///
    /// **rename_symbol:**
    /// - old_name (string, required): Current symbol name
    /// - new_name (string, required): New symbol name
    /// - scope (string, optional): Scope limitation
    /// - update_imports (bool, optional): Update import statements (default: false)
    ///
    /// Example: {"file": "src/main.rs", "symbol_name": "calculate_total", "new_body": "items.iter().sum()"}
    #[serde(default = "default_empty_json")]
    pub params: String,

    /// Preview changes without applying them (default: false).
    /// Set true to see what would change before actually modifying files
    #[serde(default)]
    pub dry_run: bool,
}

fn default_empty_json() -> String {
    "{}".to_string()
}

// ===== RENAME SYMBOL TOOL IMPLEMENTATION =====

impl RenameSymbolTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Validation
        if self.old_name.is_empty() || self.new_name.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: old_name and new_name are required and cannot be empty".to_string(),
            )]));
        }

        if self.old_name == self.new_name {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: old_name and new_name are identical - no rename needed".to_string(),
            )]));
        }

        // Delegate to SmartRefactorTool's rename logic
        let smart_refactor = SmartRefactorTool {
            operation: "rename_symbol".to_string(),
            params: serde_json::json!({
                "old_name": self.old_name,
                "new_name": self.new_name,
                "scope": self.scope,
            })
            .to_string(),
            dry_run: self.dry_run,
        };

        smart_refactor.handle_rename_symbol(handler).await
    }
}

// ===== EDIT SYMBOL TOOL IMPLEMENTATION =====

impl EditSymbolTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Validate based on operation type
        match self.operation {
            EditOperation::ReplaceBody => {
                // Delegate to SmartRefactorTool's replace_symbol_body logic
                let smart_refactor = SmartRefactorTool {
                    operation: "replace_symbol_body".to_string(),
                    params: serde_json::json!({
                        "file": self.file_path,
                        "symbol_name": self.symbol_name,
                        "new_body": self.content,
                    })
                    .to_string(),
                    dry_run: self.dry_run,
                };
                smart_refactor.handle_replace_symbol_body(handler).await
            }
            EditOperation::InsertRelative => {
                // Delegate to SmartRefactorTool's insert_relative_to_symbol logic
                let smart_refactor = SmartRefactorTool {
                    operation: "insert_relative_to_symbol".to_string(),
                    params: serde_json::json!({
                        "file": self.file_path,
                        "target_symbol": self.symbol_name,
                        "position": self.position.as_deref().unwrap_or("after"),
                        "content": self.content,
                    })
                    .to_string(),
                    dry_run: self.dry_run,
                };
                smart_refactor
                    .handle_insert_relative_to_symbol(handler)
                    .await
            }
            EditOperation::ExtractToFile => {
                // Validate target_file is provided
                if self.target_file.is_none() {
                    return Ok(CallToolResult::text_content(vec![Content::text(
                        "Error: target_file is required for extract_to_file operation".to_string(),
                    )]));
                }

                // Delegate to SmartRefactorTool's extract_symbol_to_file logic
                let smart_refactor = SmartRefactorTool {
                    operation: "extract_symbol_to_file".to_string(),
                    params: serde_json::json!({
                        "source_file": self.file_path,
                        "target_file": self.target_file.as_ref().unwrap(),
                        "symbol_name": self.symbol_name,
                        "update_imports": false,  // Can add parameter later if needed
                    })
                    .to_string(),
                    dry_run: self.dry_run,
                };
                smart_refactor.handle_extract_symbol_to_file(handler).await
            }
        }
    }
}

impl SmartRefactorTool {
    /// Create result for refactoring operations.
    /// For dry_run: returns before/after preview as text (what agents need).
    /// For applied: returns confirmation summary as text.
    fn create_result(
        &self,
        operation: &str,
        _success: bool,
        files_modified: Vec<String>,
        changes_count: usize,
        preview: Option<String>,
    ) -> Result<CallToolResult> {
        let file_list = files_modified.join(", ");
        let text = if let Some(preview) = preview {
            // Dry run: show before/after preview
            preview
        } else {
            // Applied: confirmation summary
            format!(
                "edit_symbol {} — applied {} change(s) to {}",
                operation, changes_count, file_list
            )
        };

        Ok(CallToolResult::text_content(vec![Content::text(text)]))
    }

    /// Dispatcher used by tests to route operations to the appropriate handler method.
    /// Production code uses the focused MCP tools (RenameSymbolTool, EditSymbolTool) directly.
    #[allow(dead_code)]
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        match self.operation.as_str() {
            "rename_symbol" => self.handle_rename_symbol(handler).await,
            "replace_symbol_body" => self.handle_replace_symbol_body(handler).await,
            "insert_relative_to_symbol" => self.handle_insert_relative_to_symbol(handler).await,
            "extract_symbol_to_file" => self.handle_extract_symbol_to_file(handler).await,
            other => Err(anyhow::anyhow!("Unknown refactoring operation: '{}'", other)),
        }
    }

    /// Rename symbol occurrences in a single file using tree-sitter AST-aware replacement
    async fn rename_in_file(
        &self,
        handler: &JulieServerHandler,
        file_path: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<usize> {
        // Resolve file path relative to workspace root
        let workspace_guard = handler.workspace.read().await;
        let workspace = workspace_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Workspace not initialized"))?;

        let absolute_path = if std::path::Path::new(file_path).is_absolute() {
            file_path.to_string()
        } else {
            workspace.root.join(file_path).to_string_lossy().to_string()
        };

        // Read file content
        let content = fs::read_to_string(&absolute_path)?;

        // Tree-sitter AST-aware replacement: only renames identifiers, skips strings/comments
        let updated_content =
            self.smart_text_replace(&content, old_name, new_name, file_path, false)?;

        if updated_content == content {
            return Ok(0); // No changes
        }

        // Write back using atomic operations (skip if dry-run)
        if !self.dry_run {
            let tx = EditingTransaction::begin(&absolute_path)?;
            tx.commit(&updated_content)?;
        }

        // Count changes by line differences
        let content_lines = content.lines().count();
        let updated_lines = updated_content.lines().count();
        let changes = if content_lines != updated_lines {
            (content_lines.abs_diff(updated_lines)) + 1
        } else {
            1
        };

        Ok(changes)
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

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse {} file", language))?;

        // 🌳 AST-AWARE REPLACEMENT: Walk tree to find identifier nodes
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();
        let content_bytes = content.as_bytes();

        self.collect_identifier_replacements(
            tree.root_node(),
            content_bytes,
            old_name,
            new_name,
            &mut replacements,
        );

        // Apply replacements in reverse order (end to start) to preserve byte positions
        replacements.sort_by(|a, b| b.0.cmp(&a.0));

        let mut result = content.to_string();
        for (start, end, replacement) in replacements {
            result.replace_range(start..end, &replacement);
        }

        Ok(result)
    }

    /// Recursively walk tree and collect identifier nodes to replace
    fn collect_identifier_replacements(
        &self,
        node: tree_sitter::Node,
        content_bytes: &[u8],
        old_name: &str,
        new_name: &str,
        replacements: &mut Vec<(usize, usize, String)>,
    ) {
        let node_kind = node.kind();

        // Skip string literals and comments - these should NOT be renamed
        if node_kind.contains("string")
            || node_kind.contains("comment")
            || node_kind == "template_string"
            || node_kind == "string_fragment"
        {
            return;
        }

        // Check if this node is an identifier matching old_name
        if node_kind == "identifier" || node_kind == "type_identifier" {
            let start = node.start_byte();
            let end = node.end_byte();

            if let Ok(text) = std::str::from_utf8(&content_bytes[start..end]) {
                if text == old_name {
                    replacements.push((start, end, new_name.to_string()));
                    return; // Don't recurse into children of identifiers
                }
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_identifier_replacements(
                child,
                content_bytes,
                old_name,
                new_name,
                replacements,
            );
        }
    }

    /// Get tree-sitter language for file type (delegates to shared language module)
    fn get_tree_sitter_language(&self, language: &str) -> Result<tree_sitter::Language> {
        crate::language::get_tree_sitter_language(language)
    }
}
