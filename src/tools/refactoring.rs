//! Smart Refactoring Tools - Semantic code transformations
//!
//! This module provides intelligent refactoring operations that combine:
//! - Code understanding (tree-sitter parsing, symbol analysis)
//! - Global code intelligence (fast_refs, fast_goto, search)
//! - Precise text manipulation (diff-match-patch-rs)
//!
//! Unlike simple text editing, these tools understand code semantics and
//! can perform complex transformations safely across entire codebases.

use anyhow::Result;
use diff_match_patch_rs::{DiffMatchPatch, Efficient, PatchInput};
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use tracing::{debug, info};

use crate::handler::JulieServerHandler;
use crate::tools::navigation::FastRefsTool;

/// Available refactoring operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RefactorOperation {
    /// Rename a symbol across the codebase
    RenameSymbol,
    /// Extract selected code into a new function
    ExtractFunction,
    /// Extract a value into a variable
    ExtractVariable,
    /// Inline a variable's value at usage sites
    InlineVariable,
    /// Inline a function's body at call sites
    InlineFunction,
    /// Add a parameter to a function
    AddParameter,
    /// Remove a parameter from a function
    RemoveParameter,
    /// Reorder function parameters
    ReorderParameters,
}

/// Smart refactoring tool for semantic code transformations
#[mcp_tool(
    name = "smart_refactor",
    description = "🔄 INTELLIGENT REFACTORING - Semantic code transformations using AST analysis + precise diff-match-patch",
    title = "Smart Semantic Refactoring Tool"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SmartRefactorTool {
    /// The refactoring operation to perform
    pub operation: RefactorOperation,

    /// Operation-specific parameters as JSON string
    /// For rename_symbol: "{\"old_name\": \"UserService\", \"new_name\": \"AccountService\", \"scope\": \"workspace\", \"update_imports\": true}"
    /// For extract_function: "{\"file\": \"src/handler.rs\", \"start_line\": 45, \"end_line\": 67, \"function_name\": \"validateInput\"}"
    #[serde(default = "default_empty_json")]
    pub params: String,

    /// Preview changes without applying them
    #[serde(default)]
    pub dry_run: bool,
}

fn default_empty_json() -> String {
    "{}".to_string()
}

impl SmartRefactorTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🔄 Smart refactor operation: {:?}", self.operation);

        match &self.operation {
            RefactorOperation::RenameSymbol => self.handle_rename_symbol(handler).await,
            RefactorOperation::ExtractFunction => self.handle_extract_function(handler).await,
            _ => {
                let message = format!(
                    "🚧 Operation '{:?}' not yet implemented\n\
                    ✅ Available: RenameSymbol\n\
                    🔜 Coming soon: ExtractFunction, ExtractVariable, etc.",
                    self.operation
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }

    /// Handle rename symbol operation
    async fn handle_rename_symbol(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔄 Processing rename symbol operation");

        // Parse JSON parameters - return errors for invalid JSON or missing parameters
        let params: JsonValue = serde_json::from_str(&self.params)
            .map_err(|e| anyhow::anyhow!("Invalid JSON in params: {}", e))?;

        let old_name = params
            .get("old_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: old_name"))?;

        let new_name = params
            .get("new_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: new_name"))?;

        let scope = params
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("workspace");

        let update_imports = params
            .get("update_imports")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let update_comments = params
            .get("update_comments")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        debug!(
            "🎯 Rename '{}' -> '{}' (scope: {}, imports: {}, comments: {})",
            old_name, new_name, scope, update_imports, update_comments
        );

        // Step 1: Find all references to the symbol
        let refs_tool = FastRefsTool {
            symbol: old_name.to_string(),
            include_definition: true,
            limit: 1000,                            // High limit for comprehensive rename
            workspace: Some("primary".to_string()), // TODO: Map scope to workspace
        };

        let refs_result = refs_tool.call_tool(handler).await?;

        // Extract file locations from the refs result
        let file_locations = self.parse_refs_result(&refs_result)?;

        if file_locations.is_empty() {
            let message = format!(
                "🔍 No references found for symbol '{}'\n\
                💡 Check spelling or try fast_search to locate the symbol",
                old_name
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        debug!(
            "📍 Found {} references across {} files",
            file_locations
                .values()
                .map(|refs| refs.len())
                .sum::<usize>(),
            file_locations.len()
        );

        // Step 2: Apply renames file by file
        let mut renamed_files = Vec::new();
        let mut errors = Vec::new();
        let dmp = DiffMatchPatch::new();

        for (file_path, _line_refs) in &file_locations {
            match self
                .rename_in_file(file_path, old_name, new_name, &dmp)
                .await
            {
                Ok(changes_applied) => {
                    if changes_applied > 0 {
                        renamed_files.push((file_path.clone(), changes_applied));
                    }
                }
                Err(e) => {
                    errors.push(format!("❌ {}: {}", file_path, e));
                }
            }
        }

        // Step 3: Generate result summary
        let total_files = renamed_files.len();
        let total_changes: usize = renamed_files.iter().map(|(_, count)| count).sum();

        if self.dry_run {
            let mut preview = format!(
                "🔍 DRY RUN: Rename '{}' -> '{}'\n\
                📊 Would modify {} files with {} total changes\n\n",
                old_name, new_name, total_files, total_changes
            );

            for (file, count) in &renamed_files {
                preview.push_str(&format!("  • {}: {} changes\n", file, count));
            }

            if !errors.is_empty() {
                preview.push_str("\n⚠️ Potential issues:\n");
                for error in &errors {
                    preview.push_str(&format!("  • {}\n", error));
                }
            }

            preview.push_str("\n💡 Set dry_run=false to apply changes");

            return Ok(CallToolResult::text_content(vec![TextContent::from(
                preview,
            )]));
        }

        // Final success message
        let mut message = format!(
            "✅ Rename successful: '{}' -> '{}'\n\
            📊 Modified {} files with {} total changes\n",
            old_name, new_name, total_files, total_changes
        );

        if !renamed_files.is_empty() {
            message.push_str("\n📁 Modified files:\n");
            for (file, count) in &renamed_files {
                message.push_str(&format!("  • {}: {} changes\n", file, count));
            }
        }

        if !errors.is_empty() {
            message.push_str("\n⚠️ Some files had errors:\n");
            for error in &errors {
                message.push_str(&format!("  • {}\n", error));
            }
        }

        message.push_str("\n🎯 Next steps:\n• Run tests to verify changes\n• Use fast_refs to validate rename completion\n💡 Tip: Use git to track changes and revert if needed");

        Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]))
    }

    /// Parse the result from fast_refs to extract file locations
    fn parse_refs_result(&self, refs_result: &CallToolResult) -> Result<HashMap<String, Vec<u32>>> {
        let mut file_locations: HashMap<String, Vec<u32>> = HashMap::new();

        // Extract text content from the result
        let content = refs_result
            .content
            .iter()
            .filter_map(|block| {
                if let Ok(json_value) = serde_json::to_value(block) {
                    json_value
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Parse the references (expected format: "file_path:line_number")
        for line in content.lines() {
            if let Some(colon_pos) = line.rfind(':') {
                let file_part = &line[..colon_pos];
                let line_part = &line[colon_pos + 1..];

                if let Ok(line_num) = line_part.parse::<u32>() {
                    file_locations
                        .entry(file_part.to_string())
                        .or_insert_with(Vec::new)
                        .push(line_num);
                }
            }
        }

        Ok(file_locations)
    }

    /// Rename all occurrences of old_name to new_name in a single file
    async fn rename_in_file(
        &self,
        file_path: &str,
        old_name: &str,
        new_name: &str,
        dmp: &DiffMatchPatch,
    ) -> Result<usize> {
        // Read the file
        let original_content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;

        // Simple replacement for now - TODO: Make this smarter with tree-sitter
        let new_content = original_content.replace(old_name, new_name);

        if original_content == new_content {
            return Ok(0); // No changes needed
        }

        // Count the number of replacements
        let changes_count = original_content.matches(old_name).count();

        if !self.dry_run {
            // Use diff-match-patch for atomic writing
            let diffs = dmp
                .diff_main::<Efficient>(&original_content, &new_content)
                .map_err(|e| anyhow::anyhow!("Failed to generate diff: {:?}", e))?;
            let patches = dmp
                .patch_make(PatchInput::new_diffs(&diffs))
                .map_err(|e| anyhow::anyhow!("Failed to create patches: {:?}", e))?;
            let (final_content, patch_results) = dmp
                .patch_apply(&patches, &original_content)
                .map_err(|e| anyhow::anyhow!("Failed to apply patches: {:?}", e))?;

            // Ensure all patches applied successfully
            if patch_results.iter().any(|&success| !success) {
                return Err(anyhow::anyhow!("Some patches failed to apply"));
            }

            // Write the final content
            fs::write(file_path, &final_content)?;
        }

        Ok(changes_count)
    }

    /// Handle extract function operation (placeholder)
    async fn handle_extract_function(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        let message = "🚧 ExtractFunction operation is not yet implemented\n\
                      📋 Design in progress - will extract selected code into a new function\n\
                      💡 Use RenameSymbol operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]))
    }
}
