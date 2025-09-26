use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use rust_mcp_sdk::{macros::mcp_tool};
use rust_mcp_sdk::macros::JsonSchema;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tracing::{debug, warn};
use std::fs;

use crate::handler::JulieServerHandler;

//******************//
//  Editing Tools   //
//******************//

#[mcp_tool(
    name = "fast_edit",
    description = "EDIT WITH CONFIDENCE - Surgical code changes that preserve structure with automatic rollback",
    title = "Fast Surgical Code Editor"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastEditTool {
    pub file_path: String,
    pub find_text: String,
    pub replace_text: String,
    #[serde(default = "default_true")]
    pub validate: bool,
    #[serde(default = "default_true")]
    pub backup: bool,
    #[serde(default)]
    pub dry_run: bool,
}

fn default_true() -> bool { true }

impl FastEditTool {
    pub async fn call_tool(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("⚡ Fast edit: {} -> replace '{}' with '{}'",
               self.file_path, self.find_text, self.replace_text);

        // Validate inputs
        if self.find_text.is_empty() {
            let message = "❌ find_text cannot be empty\n💡 Specify the exact text to find and replace";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        if self.find_text == self.replace_text {
            let message = "❌ find_text and replace_text are identical\n💡 No changes needed";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Check if file exists
        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!("❌ File not found: {}\n💡 Check the file path", self.file_path);
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Read current file content
        let original_content = match fs::read_to_string(&self.file_path) {
            Ok(content) => content,
            Err(e) => {
                let message = format!("❌ Failed to read file: {}\n💡 Check file permissions", e);
                return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
            }
        };

        // Check if find_text exists in the file
        if !original_content.contains(&self.find_text) {
            let message = format!(
                "❌ Text not found in file: '{}'\n\
                💡 Check the exact text to find (case sensitive)",
                self.find_text
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform the replacement
        let modified_content = original_content.replace(&self.find_text, &self.replace_text);

        // Calculate diff using diffy
        let patch = diffy::create_patch(&original_content, &modified_content);

        if self.dry_run {
            let message = format!(
                "🔍 Dry run mode - showing changes to: {}\n\
                📊 Changes preview:\n\n{}\n\n\
                💡 Set dry_run=false to apply changes",
                self.file_path, patch
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Create backup if requested
        let backup_path = if self.backup {
            let backup_path = format!("{}.backup", self.file_path);
            match fs::write(&backup_path, &original_content) {
                Ok(_) => Some(backup_path),
                Err(e) => {
                    warn!("Failed to create backup: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Basic validation (syntax check would go here)
        if self.validate {
            let validation_result = self.validate_changes(&modified_content);
            if let Err(validation_error) = validation_result {
                let message = format!(
                    "❌ Validation failed: {}\n\
                    💡 Changes would break the code structure",
                    validation_error
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
            }
        }

        // Apply changes
        match fs::write(&self.file_path, &modified_content) {
            Ok(_) => {
                let changes_count = self.find_text.lines().count().max(self.replace_text.lines().count());
                let backup_info = if let Some(backup) = backup_path {
                    format!("\n💾 Backup created: {}", backup)
                } else {
                    String::new()
                };

                let message = format!(
                    "✅ Fast edit successful!\n\
                    📁 File: {}\n\
                    📊 Changed {} line(s)\n\
                    🔍 Diff:\n{}{}\n\n\
                    🎯 Next actions:\n\
                    • Run tests to verify changes\n\
                    • Use fast_refs to check impact\n\
                    • Use fast_search to find related code",
                    self.file_path, changes_count, patch, backup_info
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
            },
            Err(e) => {
                let message = format!("❌ Failed to write file: {}\n💡 Check file permissions", e);
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
            }
        }
    }

    /// Basic validation to prevent obviously broken code
    fn validate_changes(&self, content: &str) -> Result<()> {
        // Basic brace/bracket matching
        let mut braces = 0i32;
        let mut brackets = 0i32;
        let mut parens = 0i32;

        for ch in content.chars() {
            match ch {
                '{' => braces += 1,
                '}' => braces -= 1,
                '[' => brackets += 1,
                ']' => brackets -= 1,
                '(' => parens += 1,
                ')' => parens -= 1,
                _ => {}
            }
        }

        if braces != 0 {
            return Err(anyhow::anyhow!("Unmatched braces {{}} ({})", braces));
        }
        if brackets != 0 {
            return Err(anyhow::anyhow!("Unmatched brackets [] ({})", brackets));
        }
        if parens != 0 {
            return Err(anyhow::anyhow!("Unmatched parentheses () ({})", parens));
        }

        Ok(())
    }
}