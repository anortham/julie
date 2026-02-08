//! Smart Refactoring Tools - Semantic code transformations
//!
//! This module provides intelligent refactoring operations that combine:
//! - Code understanding (tree-sitter parsing, symbol analysis)
//! - Global code intelligence (fast_refs, search)
//! - Precise text manipulation
//!
//! Unlike simple text editing, these tools understand code semantics and
//! can perform complex transformations safely across entire codebases.

mod rename;
mod utils;

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
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Internal refactoring engine used by RenameSymbolTool.
/// Not exposed as an MCP tool — only used internally for rename operations.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SmartRefactorTool {
    /// The refactoring operation to perform
    pub operation: String,

    /// Operation-specific parameters as JSON string
    #[serde(default = "default_empty_json")]
    pub params: String,

    /// Preview changes without applying them (default: false).
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
                "workspace": self.workspace,
            })
            .to_string(),
            dry_run: self.dry_run,
        };

        smart_refactor.handle_rename_symbol(handler).await
    }
}

impl SmartRefactorTool {
    /// Create result for refactoring operations.
    /// For dry_run: returns before/after preview as text.
    /// For applied: returns confirmation summary as text.
    pub(crate) fn create_result(
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
                "rename_symbol {} — applied {} change(s) to {}",
                operation, changes_count, file_list
            )
        };

        Ok(CallToolResult::text_content(vec![Content::text(text)]))
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

        // AST-AWARE REPLACEMENT: Walk tree to find identifier nodes
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
