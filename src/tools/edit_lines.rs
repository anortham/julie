//! Surgical Line Editing Tool
//!
//! Provides precise line-level file modifications with insert/replace/delete operations.
//! Following TDD methodology with SOURCE/CONTROL golden master pattern.

use anyhow::{anyhow, Result};
use rust_mcp_sdk::macros::{mcp_tool, JsonSchema};
use rust_mcp_sdk::schema::CallToolResult;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{env, fs};
use tracing::{debug, info};

use crate::handler::JulieServerHandler;
use crate::tools::editing::EditingTransaction;

fn default_dry_run() -> bool {
    true
}

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
    /// Examples: "src/main.rs", "lib/services/auth.py"
    pub file_path: String,

    /// Operation: "insert", "replace", "delete"
    pub operation: String,

    /// Starting line number (1-indexed, like editors show)
    pub start_line: u32,

    /// Ending line number (default: None, required for replace/delete, ignored for insert)
    #[serde(default)]
    pub end_line: Option<u32>,

    /// Content to insert or replace (default: None, required for insert/replace, ignored for delete)
    #[serde(default)]
    pub content: Option<String>,

    /// Preview changes without applying (default: true).
    /// RECOMMENDED: Review preview first, then set dry_run=false to apply changes
    /// Set false only when you're confident the changes are correct
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}

impl EditLinesTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let resolved_path = self.resolve_file_path(handler).await?;
        info!(
            "✂️  Surgical line edit: {} at line {} in {}",
            self.operation,
            self.start_line,
            resolved_path.display()
        );

        // Validate parameters
        self.validate()?;

        // Read file
        let file_content = fs::read_to_string(&resolved_path)
            .map_err(|e| anyhow!("Failed to read file '{}': {}", resolved_path.display(), e))?;

        let newline = Self::detect_line_ending(&file_content);
        let had_trailing_newline = file_content.ends_with(newline);

        let mut lines: Vec<String> = file_content
            .lines()
            .map(|s| s.trim_end_matches('\r').to_string())
            .collect();
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
            let mut final_content = lines.join(newline);
            if had_trailing_newline {
                final_content.push_str(newline);
            }

            let target_path = resolved_path.to_string_lossy().to_string();
            let transaction = EditingTransaction::begin(&target_path)?;
            transaction.commit(&final_content)?;

            info!(
                "✅ File modified: {} lines → {} lines",
                original_line_count, new_line_count
            );
        } else {
            info!(
                "🔍 Dry run: Would modify {} lines → {} lines",
                original_line_count, new_line_count
            );
        }

        // Return result
        let display_path = resolved_path.to_string_lossy().to_string();
        self.create_result(
            &display_path,
            original_line_count,
            new_line_count,
            modified_lines,
            self.dry_run,
        )
    }

    /// Validate parameters before performing operation
    fn validate(&self) -> Result<()> {
        // Validate operation
        match self.operation.as_str() {
            "insert" | "replace" | "delete" => {}
            _ => {
                return Err(anyhow!(
                    "Invalid operation '{}'. Must be 'insert', 'replace', or 'delete'",
                    self.operation
                ))
            }
        }

        // Validate line numbers
        if self.start_line == 0 {
            return Err(anyhow!(
                "start_line must be >= 1 (line numbers are 1-indexed)"
            ));
        }

        // Validate operation-specific requirements
        match self.operation.as_str() {
            "insert" => {
                if self.content.is_none() {
                    return Err(anyhow!("'content' is required for insert operation"));
                }
            }
            "replace" => {
                if self.end_line.is_none() {
                    return Err(anyhow!("'end_line' is required for replace operation"));
                }
                if self.content.is_none() {
                    return Err(anyhow!("'content' is required for replace operation"));
                }
                if let Some(end) = self.end_line {
                    if end < self.start_line {
                        return Err(anyhow!(
                            "end_line ({}) must be >= start_line ({})",
                            end,
                            self.start_line
                        ));
                    }
                }
            }
            "delete" => {
                if self.end_line.is_none() {
                    return Err(anyhow!("'end_line' is required for delete operation"));
                }
                if let Some(end) = self.end_line {
                    if end < self.start_line {
                        return Err(anyhow!(
                            "end_line ({}) must be >= start_line ({})",
                            end,
                            self.start_line
                        ));
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Perform insert operation
    fn perform_insert(&self, lines: &mut Vec<String>) -> Result<usize> {
        let content = self.content.as_ref()
            .ok_or_else(|| anyhow!("Internal error: content is required for insert operation"))?;
        let idx = (self.start_line - 1) as usize;

        if idx > lines.len() {
            return Err(anyhow!(
                "Cannot insert at line {} - file only has {} lines",
                self.start_line,
                lines.len()
            ));
        }

        let new_lines = Self::normalize_input_lines(content);
        debug!(
            "📝 Inserting {} line(s) at line {}",
            new_lines.len(),
            self.start_line
        );

        for (offset, line) in new_lines.iter().enumerate() {
            lines.insert(idx + offset, line.clone());
        }

        Ok(new_lines.len())
    }

    /// Perform replace operation
    fn perform_replace(&self, lines: &mut Vec<String>) -> Result<usize> {
        let content = self.content.as_ref()
            .ok_or_else(|| anyhow!("Internal error: content is required for replace operation"))?;
        let start_idx = (self.start_line - 1) as usize;
        let end_idx = self.end_line
            .ok_or_else(|| anyhow!("Internal error: end_line is required for replace operation"))? as usize;

        if start_idx >= lines.len() {
            return Err(anyhow!(
                "Cannot replace starting at line {} - file only has {} lines",
                self.start_line,
                lines.len()
            ));
        }

        if end_idx > lines.len() {
            return Err(anyhow!(
                "Cannot replace up to line {} - file only has {} lines",
                end_idx,
                lines.len()
            ));
        }

        let lines_to_replace = end_idx - start_idx;
        debug!(
            "🔄 Replacing lines {}-{} ({} lines) with: '{}'",
            self.start_line, end_idx, lines_to_replace, content
        );

        // Remove old lines
        lines.drain(start_idx..end_idx);

        // Insert new content (could be multi-line)
        let new_lines = Self::normalize_input_lines(content);
        let new_line_count = new_lines.len();

        for (offset, line) in new_lines.into_iter().enumerate() {
            lines.insert(start_idx + offset, line);
        }

        Ok(new_line_count) // Return number of new lines inserted
    }

    /// Perform delete operation
    fn perform_delete(&self, lines: &mut Vec<String>) -> Result<usize> {
        let start_idx = (self.start_line - 1) as usize;
        let end_idx = self.end_line
            .ok_or_else(|| anyhow!("Internal error: end_line is required for delete operation"))? as usize;

        if start_idx >= lines.len() {
            return Err(anyhow!(
                "Cannot delete starting at line {} - file only has {} lines",
                self.start_line,
                lines.len()
            ));
        }

        if end_idx > lines.len() {
            return Err(anyhow!(
                "Cannot delete up to line {} - file only has {} lines",
                end_idx,
                lines.len()
            ));
        }

        let lines_to_delete = end_idx - start_idx;
        debug!(
            "🗑️  Deleting lines {}-{} ({} lines)",
            self.start_line, end_idx, lines_to_delete
        );

        lines.drain(start_idx..end_idx);

        Ok(lines_to_delete) // Return number of lines deleted
    }

    /// Create result message
    fn create_result(
        &self,
        display_path: &str,
        original_lines: usize,
        new_lines: usize,
        modified: usize,
        dry_run: bool,
    ) -> Result<CallToolResult> {
        // Format line range differently for insert vs replace/delete
        let line_description = match self.operation.as_str() {
            "insert" => format!("at line {}", self.start_line),
            _ => {
                // For replace/delete operations, end_line should always be present
                // If it's None, this is a logic error in validation
                let end_line = self.end_line.unwrap_or_else(|| {
                    // Fallback for defensive programming, should never happen
                    self.start_line
                });
                format!("lines {} - {}", self.start_line, end_line)
            }
        };

        let message = if dry_run {
            format!(
                "Dry run: {} operation on {} ({})\nWould modify {} lines: {} -> {} lines (no changes applied)",
                self.operation,
                display_path,
                line_description,
                modified,
                original_lines,
                new_lines
            )
        } else {
            format!(
                "Edit complete: {} operation on {} ({})\nModified {} lines: {} -> {} lines",
                self.operation, display_path, line_description, modified, original_lines, new_lines
            )
        };

        Ok(CallToolResult::text_content(vec![message.into()]))
    }

    async fn resolve_file_path(&self, handler: &JulieServerHandler) -> Result<PathBuf> {
        use crate::utils::file_utils::secure_path_resolution;

        // Get workspace root for security validation
        let workspace_root = if let Some(workspace) = handler.get_workspace().await? {
            workspace.root.clone()
        } else {
            env::current_dir()
                .map_err(|e| anyhow!("Failed to determine current directory: {}", e))?
        };

        // Use secure path resolution to prevent traversal attacks
        secure_path_resolution(&self.file_path, &workspace_root)
    }

    fn detect_line_ending(content: &str) -> &'static str {
        if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        }
    }

    fn normalize_input_lines(content: &str) -> Vec<String> {
        content
            .lines()
            .map(|line| line.trim_end_matches('\r').to_string())
            .collect()
    }
}
