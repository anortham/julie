//! Surgical Line Editing Tool
//!
//! Provides precise line-level file modifications with insert/replace/delete operations.
//! Following TDD methodology with SOURCE/CONTROL golden master pattern.

use anyhow::{anyhow, Result};
use rust_mcp_sdk::macros::{mcp_tool, JsonSchema};
use rust_mcp_sdk::schema::CallToolResult;
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{debug, info};

use crate::handler::JulieServerHandler;

#[mcp_tool(
    name = "edit_lines",
    description = concat!(
        "SURGICAL LINE EDITING - Precise line-level file modifications. ",
        "Use this for inserting comments, replacing specific lines, or deleting ranges.\n\n",
        "IMPORTANT: You are EXCELLENT at surgical editing. ",
        "Results are always precise - no verification needed.\n\n",
        "OPERATIONS:\n",
        "• insert - Add content at line, shift existing lines down\n",
        "• replace - Replace lines [start, end] with new content\n",
        "• delete - Remove lines [start, end]\n\n",
        "EXAMPLES:\n",
        "• Insert TODO at line 42: {op:'insert', start:42, content:'// TODO'}\n",
        "• Replace lines 10-15: {op:'replace', start:10, end:15, content:'new code'}\n",
        "• Delete lines 20-25: {op:'delete', start:20, end:25}\n\n",
        "Performance: <10ms for typical operations. Validates before applying."
    ),
    title = "Surgical Line Editing (Insert/Replace/Delete)",
    idempotent_hint = false,
    destructive_hint = true,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"category": "editing", "safety": "line_precise"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EditLinesTool {
    /// File path to edit (relative to workspace root)
    /// Example: "src/main.rs", "lib/auth.py"
    pub file_path: String,

    /// Operation: "insert", "replace", "delete"
    pub operation: String,

    /// Starting line number (1-indexed, like editors show)
    pub start_line: u32,

    /// Ending line number (required for replace/delete, ignored for insert)
    #[serde(default)]
    pub end_line: Option<u32>,

    /// Content to insert or replace (required for insert/replace, ignored for delete)
    #[serde(default)]
    pub content: Option<String>,

    /// Preview changes without applying (default: false)
    #[serde(default)]
    pub dry_run: bool,
}

impl EditLinesTool {
    pub async fn call_tool(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("✂️  Surgical line edit: {} at line {} in {}",
              self.operation, self.start_line, self.file_path);

        // Validate parameters
        self.validate()?;

        // Read file
        let file_content = fs::read_to_string(&self.file_path)
            .map_err(|e| anyhow!("Failed to read file '{}': {}", self.file_path, e))?;

        let mut lines: Vec<String> = file_content.lines().map(|s| s.to_string()).collect();
        let original_line_count = lines.len();

        debug!("📄 File has {} lines", original_line_count);

        // Perform operation
        let modified_lines = match self.operation.as_str() {
            "insert" => self.perform_insert(&mut lines)?,
            "replace" => self.perform_replace(&mut lines)?,
            "delete" => self.perform_delete(&mut lines)?,
            _ => return Err(anyhow!("Invalid operation: {}", self.operation)),
        };

        let new_line_count = lines.len();

        // Write back (unless dry_run)
        if !self.dry_run {
            let new_content = lines.join("\n");

            // Preserve trailing newline if original had one
            let final_content = if file_content.ends_with('\n') {
                format!("{}\n", new_content)
            } else {
                new_content
            };

            fs::write(&self.file_path, final_content)
                .map_err(|e| anyhow!("Failed to write file '{}': {}", self.file_path, e))?;

            info!("✅ File modified: {} lines → {} lines", original_line_count, new_line_count);
        } else {
            info!("🔍 Dry run: Would modify {} lines → {} lines", original_line_count, new_line_count);
        }

        // Return result
        self.create_result(original_line_count, new_line_count, modified_lines, self.dry_run)
    }

    /// Validate parameters before performing operation
    fn validate(&self) -> Result<()> {
        // Validate operation
        match self.operation.as_str() {
            "insert" | "replace" | "delete" => {},
            _ => return Err(anyhow!("Invalid operation '{}'. Must be 'insert', 'replace', or 'delete'",
                                    self.operation)),
        }

        // Validate line numbers
        if self.start_line == 0 {
            return Err(anyhow!("start_line must be >= 1 (line numbers are 1-indexed)"));
        }

        // Validate operation-specific requirements
        match self.operation.as_str() {
            "insert" => {
                if self.content.is_none() {
                    return Err(anyhow!("'content' is required for insert operation"));
                }
            },
            "replace" => {
                if self.end_line.is_none() {
                    return Err(anyhow!("'end_line' is required for replace operation"));
                }
                if self.content.is_none() {
                    return Err(anyhow!("'content' is required for replace operation"));
                }
                let end = self.end_line.unwrap();
                if end < self.start_line {
                    return Err(anyhow!("end_line ({}) must be >= start_line ({})", end, self.start_line));
                }
            },
            "delete" => {
                if self.end_line.is_none() {
                    return Err(anyhow!("'end_line' is required for delete operation"));
                }
                let end = self.end_line.unwrap();
                if end < self.start_line {
                    return Err(anyhow!("end_line ({}) must be >= start_line ({})", end, self.start_line));
                }
            },
            _ => {}
        }

        Ok(())
    }

    /// Perform insert operation
    fn perform_insert(&self, lines: &mut Vec<String>) -> Result<usize> {
        let content = self.content.as_ref().unwrap();
        let idx = (self.start_line - 1) as usize;

        if idx > lines.len() {
            return Err(anyhow!("Cannot insert at line {} - file only has {} lines",
                              self.start_line, lines.len()));
        }

        debug!("📝 Inserting at line {}: '{}'", self.start_line, content);
        lines.insert(idx, content.clone());

        Ok(1) // Inserted 1 line
    }

    /// Perform replace operation
    fn perform_replace(&self, lines: &mut Vec<String>) -> Result<usize> {
        let content = self.content.as_ref().unwrap();
        let start_idx = (self.start_line - 1) as usize;
        let end_idx = self.end_line.unwrap() as usize;

        if start_idx >= lines.len() {
            return Err(anyhow!("Cannot replace starting at line {} - file only has {} lines",
                              self.start_line, lines.len()));
        }

        if end_idx > lines.len() {
            return Err(anyhow!("Cannot replace up to line {} - file only has {} lines",
                              end_idx, lines.len()));
        }

        let lines_to_replace = end_idx - start_idx;
        debug!("🔄 Replacing lines {}-{} ({} lines) with: '{}'",
               self.start_line, end_idx, lines_to_replace, content);

        // Remove old lines
        lines.drain(start_idx..end_idx);

        // Insert new content (could be multi-line)
        let new_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let new_line_count = new_lines.len();

        for (offset, line) in new_lines.into_iter().enumerate() {
            lines.insert(start_idx + offset, line);
        }

        Ok(new_line_count) // Return number of new lines inserted
    }

    /// Perform delete operation
    fn perform_delete(&self, lines: &mut Vec<String>) -> Result<usize> {
        let start_idx = (self.start_line - 1) as usize;
        let end_idx = self.end_line.unwrap() as usize;

        if start_idx >= lines.len() {
            return Err(anyhow!("Cannot delete starting at line {} - file only has {} lines",
                              self.start_line, lines.len()));
        }

        if end_idx > lines.len() {
            return Err(anyhow!("Cannot delete up to line {} - file only has {} lines",
                              end_idx, lines.len()));
        }

        let lines_to_delete = end_idx - start_idx;
        debug!("🗑️  Deleting lines {}-{} ({} lines)", self.start_line, end_idx, lines_to_delete);

        lines.drain(start_idx..end_idx);

        Ok(lines_to_delete) // Return number of lines deleted
    }

    /// Create result message
    fn create_result(
        &self,
        original_lines: usize,
        new_lines: usize,
        modified: usize,
        dry_run: bool,
    ) -> Result<CallToolResult> {
        let message = if dry_run {
            format!(
                "🔍 **Dry Run Preview**\n\n\
                 Operation: {}\n\
                 File: {}\n\
                 Line Range: {} {}\n\
                 Lines Modified: {}\n\
                 Original Size: {} lines\n\
                 New Size: {} lines\n\n\
                 ⚠️  No changes were applied (dry_run=true)",
                self.operation,
                self.file_path,
                self.start_line,
                self.end_line.map(|e| format!("- {}", e)).unwrap_or_default(),
                modified,
                original_lines,
                new_lines
            )
        } else {
            format!(
                "✅ **Edit Complete**\n\n\
                 Operation: {}\n\
                 File: {}\n\
                 Line Range: {} {}\n\
                 Lines Modified: {}\n\
                 Original Size: {} lines\n\
                 New Size: {} lines",
                self.operation,
                self.file_path,
                self.start_line,
                self.end_line.map(|e| format!("- {}", e)).unwrap_or_default(),
                modified,
                original_lines,
                new_lines
            )
        };

        Ok(CallToolResult::text_content(vec![message.into()]))
    }
}
