//! Safe Editing Tools - Direct text and line manipulation with proven safety
//!
//! This module provides safe, direct code editing operations that:
//! - Accept explicit locations/text from the agent (no semantic search)
//! - Use Google's diff-match-patch algorithm for all modifications
//! - Apply changes atomically via EditingTransaction (temp file + rename)
//! - Support dry-run preview and validation before applying
//!
//! Unlike smart_refactor (semantic operations), safe_edit requires the agent
//! to explicitly provide the exact text or line numbers to modify.

use anyhow::Result;
use diff_match_patch_rs::{DiffMatchPatch, Efficient, PatchInput};
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{debug, info};

use crate::handler::JulieServerHandler;
use crate::tools::editing::EditingTransaction; // Reuse existing transaction infrastructure
use crate::utils::{progressive_reduction::ProgressiveReducer, token_estimation::TokenEstimator};

fn default_true() -> bool {
    true
}

fn default_exact_replace() -> String {
    "exact_replace".to_string()
}

//******************//
//   Safe Edit Tool //
//******************//

#[mcp_tool(
    name = "safe_edit",
    description = "SAFE DIRECT EDITING - Precise text and line manipulation with Google's proven diff-match-patch",
    title = "Safe Code Editor (DMP-powered)",
    idempotent_hint = false,
    destructive_hint = true,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"category": "editing", "safety": "dmp_transactional"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SafeEditTool {
    /// File path to edit (required for all modes)
    /// Example: "src/user.rs", "/absolute/path/to/file.ts"
    /// For multi_file_replace mode: use empty string "" to trigger multi-file operation
    pub file_path: String,

    /// Editing mode: "exact_replace", "pattern_replace", "multi_file_replace", "line_insert", "line_delete", "line_replace"
    /// Use exact_replace (default) after reading files for single exact matches, pattern_replace for find/replace in one file, multi_file_replace for codebase-wide changes
    /// Line modes (insert/delete/replace) work with specific line numbers. Default: "exact_replace" for safety
    #[serde(default = "default_exact_replace")]
    pub mode: String,

    // Exact replace mode parameters
    /// For exact_replace mode: The exact text to find (must match exactly once)
    /// Example: "function getUserData() {\n  return db.query();\n}"
    /// Safety: Will fail if text appears 0 times (not found) or >1 times (ambiguous)
    #[serde(default)]
    pub old_text: Option<String>,

    /// For exact_replace and pattern_replace modes: The replacement text
    /// Example: "function getUserData() {\n  return cache.get();\n}"
    #[serde(default)]
    pub new_text: Option<String>,

    // Pattern replace mode parameters
    /// For pattern_replace and multi_file_replace modes: Text pattern to find (can match multiple)
    /// Example: "console.log", "getUserData", "import React"
    /// Supports: Exact text matching (regex support planned)
    #[serde(default)]
    pub find_text: Option<String>,

    /// For pattern_replace and multi_file_replace modes: Replacement text
    /// Example: "logger.info", "fetchUserData", "import { React }"
    #[serde(default)]
    pub replace_text: Option<String>,

    // Line operation mode parameters
    /// For line_insert mode: Line number to insert after (1-based indexing)
    /// Example: 5 to insert new content after line 5
    #[serde(default)]
    pub line_number: Option<u32>,

    /// For line_delete and line_replace modes: Starting line number (1-based, inclusive)
    /// Example: 10 to start from line 10
    #[serde(default)]
    pub start_line: Option<u32>,

    /// For line_delete and line_replace modes: Ending line number (1-based, inclusive)
    /// Example: 20 to end at line 20 (lines 10-20 will be affected)
    #[serde(default)]
    pub end_line: Option<u32>,

    /// For line_insert and line_replace modes: Content to insert/replace
    /// Example: "import { logger } from './logger';"
    /// Multi-line content: Use "\n" for line breaks
    #[serde(default)]
    pub content: Option<String>,

    // Multi-file mode parameters
    /// For multi_file_replace mode: File pattern filter using glob syntax
    /// Examples: "src/**/*.rs", "*.test.ts", "**/components/**/*.tsx"
    /// Tip: Use specific patterns to avoid unintended modifications
    #[serde(default)]
    pub file_pattern: Option<String>,

    /// For multi_file_replace mode: Programming language filter
    /// Valid: "rust", "typescript", "javascript", "python", "java", "csharp", "php", "ruby", "swift", "kotlin", "go", "c", "cpp", "lua", "sql", "html", "css", "vue", "bash", "gdscript", "dart", "zig"
    /// Example: "typescript" to only process .ts/.tsx files
    #[serde(default)]
    pub language: Option<String>,

    /// For multi_file_replace mode: Maximum number of files to process
    /// Default: 50, Range: 1-500
    /// Tip: Start small (10-20) for safety, increase if needed
    #[serde(default = "default_limit")]
    pub limit: Option<u32>,

    // Safety parameters (all modes)
    /// Preview changes without applying them (RECOMMENDED for first run)
    /// Default: false
    /// Tip: Always use dry_run=true first to verify changes!
    #[serde(default)]
    pub dry_run: bool,

    /// Validate changes before applying (brace/bracket matching)
    /// Default: true - recommended for safety
    /// Tip: Keep enabled unless you know validation will fail incorrectly
    #[serde(default = "default_true")]
    pub validate: bool,

    /// For line_insert and line_replace modes: Automatically preserve existing indentation
    /// Default: true - maintains consistent code formatting
    #[serde(default = "default_true")]
    pub preserve_indentation: bool,
}

fn default_limit() -> Option<u32> {
    Some(50)
}

impl SafeEditTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("✏️ Safe edit: {} mode on {}", self.mode, self.file_path);

        // Route to appropriate mode handler
        match self.mode.as_str() {
            "exact_replace" => self.handle_exact_replace().await,
            "pattern_replace" => self.handle_pattern_replace().await,
            "multi_file_replace" => self.handle_multi_file_replace(handler).await,
            "line_insert" => self.handle_line_insert().await,
            "line_delete" => self.handle_line_delete().await,
            "line_replace" => self.handle_line_replace().await,
            _ => {
                let message = format!(
                    "❌ Invalid mode: '{}'\n\
                    💡 Valid modes: exact_replace, pattern_replace, multi_file_replace, line_insert, line_delete, line_replace\n\n\
                    Mode guide:\n\
                    • exact_replace - Replace exact text block (safest, most common)\n\
                    • pattern_replace - Find/replace pattern in file\n\
                    • multi_file_replace - Find/replace across files\n\
                    • line_insert - Insert at line number\n\
                    • line_delete - Delete line range\n\
                    • line_replace - Replace line range",
                    self.mode
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    self.optimize_response(&message),
                )]))
            }
        }
    }

    /// Handle exact_replace mode - Replace exact text block (must match exactly once)
    async fn handle_exact_replace(&self) -> Result<CallToolResult> {
        debug!("🎯 Exact replace mode");

        // Validate required parameters
        let old_text = self.old_text.as_ref().ok_or_else(|| {
            anyhow::anyhow!("exact_replace mode requires old_text parameter")
        })?;
        let new_text = self.new_text.as_ref().ok_or_else(|| {
            anyhow::anyhow!("exact_replace mode requires new_text parameter")
        })?;

        if old_text.is_empty() {
            let message = "❌ old_text cannot be empty\n💡 Provide the exact text to find and replace";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        if old_text == new_text {
            let message = "ℹ️ old_text and new_text are identical - no changes needed";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        // Check file exists
        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        // Read file content
        let original_content = fs::read_to_string(&self.file_path)?;

        // Check if old_text exists and count occurrences
        let match_count = original_content.matches(old_text.as_str()).count();

        if match_count == 0 {
            let message = format!(
                "❌ Text not found in file\n\
                💡 The exact text does not exist in {}\n\
                🔍 Tip: Use fast_search to locate similar text, or verify exact match including whitespace",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        if match_count > 1 {
            let message = format!(
                "❌ Text appears {} times in file - exact_replace requires exactly 1 match\n\
                💡 Use pattern_replace mode for multiple replacements, or provide more context in old_text to make it unique",
                match_count
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        // Perfect! Exactly 1 match - safe to proceed
        debug!("✅ Found exactly 1 match for exact_replace");

        // Create target content
        let target_content = original_content.replace(old_text.as_str(), new_text.as_str());

        // Use DMP to apply changes safely
        self.apply_changes_with_dmp(&original_content, &target_content, "exact replace")
            .await
    }

    /// Handle pattern_replace mode - Find/replace pattern (can match multiple times)
    async fn handle_pattern_replace(&self) -> Result<CallToolResult> {
        debug!("🔍 Pattern replace mode");

        // Validate required parameters
        let find_text = self.find_text.as_ref().ok_or_else(|| {
            anyhow::anyhow!("pattern_replace mode requires find_text parameter")
        })?;
        let replace_text = self.replace_text.as_ref().ok_or_else(|| {
            anyhow::anyhow!("pattern_replace mode requires replace_text parameter")
        })?;

        if find_text.is_empty() {
            let message = "❌ find_text cannot be empty\n💡 Provide the text pattern to find";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        if find_text == replace_text {
            let message = "ℹ️ find_text and replace_text are identical - no changes needed";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        // Check file exists
        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        // Read file content
        let original_content = fs::read_to_string(&self.file_path)?;

        // Check if find_text exists
        let match_count = original_content.matches(find_text.as_str()).count();
        if match_count == 0 {
            let message = format!(
                "🔍 Pattern not found in {}\n\
                💡 The text '{}' does not exist in this file\n\
                🔍 Tip: Use fast_search to locate the pattern first",
                self.file_path, find_text
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        debug!("✅ Found {} matches for pattern_replace", match_count);

        // Create target content
        let target_content = original_content.replace(find_text.as_str(), replace_text.as_str());

        // Use DMP to apply changes safely
        self.apply_changes_with_dmp(
            &original_content,
            &target_content,
            &format!("pattern replace ({} occurrences)", match_count),
        )
        .await
    }

    /// Handle multi_file_replace mode - Find/replace across multiple files
    async fn handle_multi_file_replace(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        debug!("🌍 Multi-file replace mode");

        // Validate that file_path is empty (trigger for multi-file mode)
        if !self.file_path.is_empty() {
            let message = "❌ multi_file_replace mode requires file_path to be empty string \"\"\n💡 Use file_pattern and language parameters to filter files";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        // Validate required parameters
        let find_text = self.find_text.as_ref().ok_or_else(|| {
            anyhow::anyhow!("multi_file_replace mode requires find_text parameter")
        })?;
        let replace_text = self.replace_text.as_ref().ok_or_else(|| {
            anyhow::anyhow!("multi_file_replace mode requires replace_text parameter")
        })?;

        if find_text.is_empty() {
            let message = "❌ find_text cannot be empty\n💡 Provide the text pattern to find";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        if find_text == replace_text {
            let message = "ℹ️ find_text and replace_text are identical - no changes needed";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        // Use fast_search to find files containing the pattern
        let search_tool = crate::tools::search::FastSearchTool {
            query: find_text.clone(),
            mode: "text".to_string(),
            language: self.language.clone(),
            file_pattern: self.file_pattern.clone(),
            limit: self.limit.unwrap_or(50),
            workspace: Some("primary".to_string()),
        };

        let search_result = search_tool.call_tool(handler).await?;
        let file_paths = self.extract_file_paths_from_search_result(&search_result)?;

        if file_paths.is_empty() {
            let message = format!(
                "🔍 No files found matching criteria:\n\
                 📝 Pattern: '{}'\n\
                 🗂️ Language: {:?}\n\
                 📁 File pattern: {:?}\n\
                 📊 0 files found\n\
                 💡 Try broader search criteria or use fast_search to verify pattern exists",
                find_text, self.language, self.file_pattern
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        debug!("✅ Found {} files to process", file_paths.len());

        // Process each file
        let mut results = Vec::new();
        let mut total_files_modified = 0;
        let mut total_replacements = 0;

        for file_path in file_paths.iter().take(self.limit.unwrap_or(50) as usize) {
            match self
                .apply_pattern_replace_to_file(file_path, find_text, replace_text)
                .await
            {
                Ok(Some((modified, count))) => {
                    if modified {
                        total_files_modified += 1;
                        total_replacements += count;
                        results.push(format!("✅ {}: {} replacement(s)", file_path, count));
                    } else {
                        results.push(format!("⏭️ {}: no matches found", file_path));
                    }
                }
                Ok(None) => {
                    results.push(format!("⏭️ {}: skipped", file_path));
                }
                Err(e) => {
                    results.push(format!("❌ {}: {}", file_path, e));
                }
            }
        }

        // Generate summary
        let summary = if self.dry_run {
            format!(
                "🔍 Multi-file replace dry run:\n\
                 📝 Pattern: '{}' → '{}'\n\
                 📊 Would modify {} file(s) with {} total replacement(s)\n\
                 💡 Set dry_run=false to apply changes",
                find_text, replace_text, total_files_modified, total_replacements
            )
        } else {
            format!(
                "✅ Multi-file replace complete:\n\
                 📝 Pattern: '{}' → '{}'\n\
                 📊 Modified {} file(s) with {} total replacement(s)",
                find_text, replace_text, total_files_modified, total_replacements
            )
        };

        let combined_result = format!("{}\n\n📋 File Details:\n{}", summary, results.join("\n"));
        Ok(CallToolResult::text_content(vec![TextContent::from(
            self.optimize_response(&combined_result),
        )]))
    }

    /// Handle line_insert mode - Insert lines at specific position
    async fn handle_line_insert(&self) -> Result<CallToolResult> {
        debug!("➕ Line insert mode");

        // Validate required parameters
        let line_number = self.line_number.ok_or_else(|| {
            anyhow::anyhow!("line_insert mode requires line_number parameter")
        })?;
        let content = self.content.as_ref().ok_or_else(|| {
            anyhow::anyhow!("line_insert mode requires content parameter")
        })?;

        if line_number == 0 {
            let message = "❌ line_number must be >= 1 (1-based indexing)\n💡 Line numbers start at 1, not 0";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        // Check file exists
        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        let original_content = fs::read_to_string(&self.file_path)?;
        let target_content = self.create_target_with_line_insert(
            &original_content,
            line_number,
            content,
        )?;

        self.apply_changes_with_dmp(
            &original_content,
            &target_content,
            &format!("insert at line {}", line_number),
        )
        .await
    }

    /// Handle line_delete mode - Delete specific line range
    async fn handle_line_delete(&self) -> Result<CallToolResult> {
        debug!("➖ Line delete mode");

        // Validate required parameters
        let start_line = self.start_line.ok_or_else(|| {
            anyhow::anyhow!("line_delete mode requires start_line parameter")
        })?;
        let end_line = self.end_line.ok_or_else(|| {
            anyhow::anyhow!("line_delete mode requires end_line parameter")
        })?;

        if start_line == 0 || end_line == 0 {
            let message = "❌ Line numbers must be >= 1 (1-based indexing)\n💡 Line numbers start at 1, not 0";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        if start_line > end_line {
            let message = "❌ start_line must be <= end_line\n💡 Check your line range";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        // Check file exists
        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        let original_content = fs::read_to_string(&self.file_path)?;
        let target_content = self.create_target_with_line_delete(&original_content, start_line, end_line)?;

        self.apply_changes_with_dmp(
            &original_content,
            &target_content,
            &format!("delete lines {}-{}", start_line, end_line),
        )
        .await
    }

    /// Handle line_replace mode - Replace specific line range
    async fn handle_line_replace(&self) -> Result<CallToolResult> {
        debug!("🔄 Line replace mode");

        // Validate required parameters
        let start_line = self.start_line.ok_or_else(|| {
            anyhow::anyhow!("line_replace mode requires start_line parameter")
        })?;
        let end_line = self.end_line.ok_or_else(|| {
            anyhow::anyhow!("line_replace mode requires end_line parameter")
        })?;
        let content = self.content.as_ref().ok_or_else(|| {
            anyhow::anyhow!("line_replace mode requires content parameter")
        })?;

        if start_line == 0 || end_line == 0 {
            let message = "❌ Line numbers must be >= 1 (1-based indexing)\n💡 Line numbers start at 1, not 0";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        if start_line > end_line {
            let message = "❌ start_line must be <= end_line\n💡 Check your line range";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        // Check file exists
        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        let original_content = fs::read_to_string(&self.file_path)?;
        let target_content = self.create_target_with_line_replace(
            &original_content,
            start_line,
            end_line,
            content,
        )?;

        self.apply_changes_with_dmp(
            &original_content,
            &target_content,
            &format!("replace lines {}-{}", start_line, end_line),
        )
        .await
    }

    //**********************//
    //   Helper Functions   //
    //**********************//

    /// Create target content with line insertion
    fn create_target_with_line_insert(
        &self,
        original: &str,
        line_number: u32,
        content: &str,
    ) -> Result<String> {
        let mut lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();

        if (line_number as usize) > lines.len() + 1 {
            return Err(anyhow::anyhow!(
                "line_number {} exceeds file length {} + 1",
                line_number,
                lines.len()
            ));
        }

        // Handle indentation if requested
        let content_to_insert = if self.preserve_indentation && line_number > 1 && (line_number as usize) <= lines.len() {
            self.apply_indentation(content, &lines[(line_number - 1) as usize])
        } else {
            content.to_string()
        };

        // Insert lines
        let insert_idx = (line_number - 1) as usize;
        for (i, line) in content_to_insert.lines().enumerate() {
            lines.insert(insert_idx + i, line.to_string());
        }

        // Preserve trailing newline if original had one
        let mut result = lines.join("\n");
        if original.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }

    /// Create target content with line deletion
    fn create_target_with_line_delete(
        &self,
        original: &str,
        start_line: u32,
        end_line: u32,
    ) -> Result<String> {
        let mut lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();

        if (start_line as usize) > lines.len() {
            return Err(anyhow::anyhow!(
                "start_line {} exceeds file length {}",
                start_line,
                lines.len()
            ));
        }

        let start_idx = (start_line - 1) as usize;
        let end_idx = std::cmp::min(end_line as usize, lines.len());

        lines.drain(start_idx..end_idx);

        // Preserve trailing newline if original had one
        let mut result = lines.join("\n");
        if original.ends_with('\n') && !result.ends_with('\n') && !result.is_empty() {
            result.push('\n');
        }

        Ok(result)
    }

    /// Create target content with line replacement
    fn create_target_with_line_replace(
        &self,
        original: &str,
        start_line: u32,
        end_line: u32,
        content: &str,
    ) -> Result<String> {
        let mut lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();

        if (start_line as usize) > lines.len() {
            return Err(anyhow::anyhow!(
                "start_line {} exceeds file length {}",
                start_line,
                lines.len()
            ));
        }

        let start_idx = (start_line - 1) as usize;
        let end_idx = std::cmp::min(end_line as usize, lines.len());

        // Handle indentation if requested
        let content_to_replace = if self.preserve_indentation && start_line > 1 {
            self.apply_indentation(content, &lines[(start_line - 1) as usize])
        } else {
            content.to_string()
        };

        // Remove old lines
        lines.drain(start_idx..end_idx);

        // Insert new lines
        for (i, line) in content_to_replace.lines().enumerate() {
            lines.insert(start_idx + i, line.to_string());
        }

        // Preserve trailing newline if original had one
        let mut result = lines.join("\n");
        if original.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        Ok(result)
    }

    /// Apply indentation from reference line to content
    fn apply_indentation(&self, content: &str, reference_line: &str) -> String {
        // Detect indentation from reference line
        let mut indent = String::new();
        for ch in reference_line.chars() {
            if ch == ' ' || ch == '\t' {
                indent.push(ch);
            } else {
                break;
            }
        }

        if indent.is_empty() {
            return content.to_string();
        }

        // Apply indentation to each line of content
        content
            .lines()
            .map(|line| {
                if line.trim().is_empty() {
                    line.to_string()
                } else {
                    format!("{}{}", indent, line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Apply pattern replace to a single file (helper for multi_file_replace)
    async fn apply_pattern_replace_to_file(
        &self,
        file_path: &str,
        find_text: &str,
        replace_text: &str,
    ) -> Result<Option<(bool, usize)>> {
        // Check if file exists
        if !std::path::Path::new(file_path).exists() {
            return Ok(None);
        }

        // Read file content
        let original_content = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(_) => return Ok(None),
        };

        // Check if find_text exists
        let match_count = original_content.matches(find_text).count();
        if match_count == 0 {
            return Ok(Some((false, 0)));
        }

        // Create target content
        let target_content = original_content.replace(find_text, replace_text);

        if !self.dry_run {
            // Use DMP to apply changes
            let dmp = DiffMatchPatch::new();
            let diffs = dmp.diff_main::<Efficient>(&original_content, &target_content)
                .map_err(|e| anyhow::anyhow!("Failed to generate diff: {:?}", e))?;
            let patches = dmp.patch_make(PatchInput::new_diffs(&diffs))
                .map_err(|e| anyhow::anyhow!("Failed to create patches: {:?}", e))?;
            let (modified_content, patch_results) = dmp.patch_apply(&patches, &original_content)
                .map_err(|e| anyhow::anyhow!("Failed to apply patches: {:?}", e))?;

            // Ensure all patches applied successfully
            if patch_results.iter().any(|&success| !success) {
                return Err(anyhow::anyhow!("Some patches failed to apply"));
            }

            // Validate if requested
            if self.validate {
                self.validate_changes(&modified_content)?;
            }

            // Apply with EditingTransaction
            let transaction = EditingTransaction::begin(file_path)?;
            transaction.commit(&modified_content)?;
        }

        Ok(Some((true, match_count)))
    }

    /// Apply changes using DMP with full safety checks
    async fn apply_changes_with_dmp(
        &self,
        original_content: &str,
        target_content: &str,
        operation: &str,
    ) -> Result<CallToolResult> {
        if original_content == target_content {
            let message = "ℹ️ No changes needed - content would be identical";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(message),
            )]));
        }

        // Use DMP to generate diffs and patches
        let dmp = DiffMatchPatch::new();
        let diffs = match dmp.diff_main::<Efficient>(original_content, target_content) {
            Ok(diffs) => diffs,
            Err(e) => {
                let message = format!(
                    "❌ Failed to generate diff: {:?}\n💡 Check file encoding and content",
                    e
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    self.optimize_response(&message),
                )]));
            }
        };

        let patches = match dmp.patch_make(PatchInput::new_diffs(&diffs)) {
            Ok(patches) => patches,
            Err(e) => {
                let message = format!(
                    "❌ Failed to create patches: {:?}\n💡 File might be corrupted",
                    e
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    self.optimize_response(&message),
                )]));
            }
        };

        let (modified_content, patch_results) =
            match dmp.patch_apply(&patches, original_content) {
                Ok((content, results)) => (content, results),
                Err(e) => {
                    let message = format!(
                        "❌ Failed to apply patches: {:?}\n💡 File state might be inconsistent",
                        e
                    );
                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                        self.optimize_response(&message),
                    )]));
                }
            };

        // Check if all patches applied successfully
        if patch_results.iter().any(|&success| !success) {
            let failed_count = patch_results.iter().filter(|&&s| !s).count();
            let message = format!(
                "⚠️ Some patches failed to apply ({}/{})\n💡 File might have changed during edit",
                failed_count,
                patch_results.len()
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        let patch_text = dmp.patch_to_text(&patches);

        // Dry run mode - show preview
        if self.dry_run {
            let message = format!(
                "🔍 Dry run: {} in {}\n\
                📊 Changes preview:\n\n{}\n\n\
                ✅ All {} patches would apply successfully\n\
                💡 Set dry_run=false to apply changes",
                operation, self.file_path, patch_text, patch_results.len()
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        // Validate changes if requested
        if self.validate {
            if let Err(validation_error) = self.validate_changes(&modified_content) {
                let message = format!(
                    "❌ Validation failed: {}\n\
                    💡 Changes would break code structure\n\
                    🔧 Try: Set validate=false if you're sure changes are correct",
                    validation_error
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    self.optimize_response(&message),
                )]));
            }
        }

        // Apply changes atomically with EditingTransaction
        match EditingTransaction::begin(&self.file_path) {
            Ok(transaction) => match transaction.commit(&modified_content) {
                Ok(_) => {
                    let message = format!(
                        "✅ Safe edit successful!\n\
                        📁 File: {}\n\
                        📊 Operation: {}\n\
                        🎯 Applied {} patches successfully\n\
                        🔍 Changes:\n{}\n\n\
                        🛡️ Safety: Google's diff-match-patch + atomic transaction\n\n\
                        🎯 Next actions:\n\
                        • Run tests to verify changes\n\
                        • Use fast_refs to check impact\n\
                        • Use fast_search to find related code\n\
                        💡 Tip: Use git to track changes and revert if needed",
                        self.file_path, operation, patch_results.len(), patch_text
                    );
                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        self.optimize_response(&message),
                    )]))
                }
                Err(e) => {
                    let message = format!(
                        "❌ Failed to commit changes: {}\n💡 Original file preserved via transaction rollback",
                        e
                    );
                    Ok(CallToolResult::text_content(vec![TextContent::from(
                        self.optimize_response(&message),
                    )]))
                }
            },
            Err(e) => {
                let message = format!(
                    "❌ Failed to start transaction: {}\n💡 Check file permissions and disk space",
                    e
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    self.optimize_response(&message),
                )]))
            }
        }
    }

    /// Validate changes (basic brace/bracket matching)
    fn validate_changes(&self, content: &str) -> Result<()> {
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

    /// Extract file paths from fast_search result
    fn extract_file_paths_from_search_result(
        &self,
        search_result: &CallToolResult,
    ) -> Result<Vec<String>> {
        let mut paths = Vec::new();

        for content_block in &search_result.content {
            let content_json = serde_json::to_value(content_block)?;

            let search_text = if let Some(text_value) = content_json.get("text") {
                text_value.as_str().unwrap_or("").to_string()
            } else {
                continue;
            };

            // FastSearchTool returns lines like: "   📁 path/to/file.rs:10-20"
            for line in search_text.lines() {
                if line.contains("📁") {
                    if let Some(emoji_pos) = line.find("📁") {
                        let after_emoji = &line[emoji_pos + "📁".len()..].trim();
                        if let Some(colon_pos) = after_emoji.find(':') {
                            let file_path = after_emoji[..colon_pos].trim();
                            if !file_path.is_empty() && !paths.contains(&file_path.to_string()) {
                                paths.push(file_path.to_string());
                            }
                        }
                    }
                }
            }
        }

        debug!("Extracted {} file paths from search result", paths.len());
        Ok(paths)
    }

    /// Apply token optimization to responses
    pub fn optimize_response(&self, message: &str) -> String {
        let token_estimator = TokenEstimator::new();
        let token_limit: usize = 15000; // 15K token limit for editing tools

        let message_tokens = token_estimator.estimate_string(message);

        if message_tokens <= token_limit {
            return message.to_string();
        }

        // Apply progressive reduction
        let lines: Vec<String> = message.lines().map(|s| s.to_string()).collect();
        let progressive_reducer = ProgressiveReducer::new();
        let line_refs: Vec<&String> = lines.iter().collect();

        let estimate_lines_tokens = |line_refs: &[&String]| -> usize {
            let content = line_refs
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            token_estimator.estimate_string(&content)
        };

        let reduced_lines =
            progressive_reducer.reduce(&line_refs, token_limit, estimate_lines_tokens);
        let reduced_count = reduced_lines.len();
        let mut optimized_message = reduced_lines
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");

        if reduced_count < lines.len() {
            optimized_message.push_str("\n\n⚠️ Response truncated to stay within token limits");
            optimized_message.push_str("\n💡 Use more specific parameters for focused results");
        }

        optimized_message
    }
}
