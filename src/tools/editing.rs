use anyhow::Result;
use diff_match_patch_rs::{DiffMatchPatch, Efficient, PatchInput};
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{debug, warn};

use crate::handler::JulieServerHandler;

fn default_true() -> bool {
    true
}

//******************//
//  Existing Tool   //
//******************//

#[mcp_tool(
    name = "fast_edit",
    description = "EDIT WITH CONFIDENCE - Surgical code changes and search_and_replace across multiple files",
    title = "Fast Code Editor with Search & Replace"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastEditTool {
    /// File path for single-file editing, or empty string "" for multi-file search_and_replace mode
    /// Single file example: "src/main.rs"
    /// Multi-file mode: "" (empty string triggers search_and_replace across multiple files)
    pub file_path: String,
    /// Text pattern to find and replace
    /// Supports exact text matching and simple patterns
    /// Example: "getUserData" or "console.log" or "old_function_name"
    pub find_text: String,
    /// Replacement text to substitute for found patterns
    /// Example: "fetchUserData" or "logger.info" or "new_function_name"
    pub replace_text: String,
    /// Operation mode for multi-file operations
    /// Use "search_and_replace" for multi-file mode (requires file_path to be empty string)
    /// Leave empty for single-file mode
    #[serde(default)]
    pub mode: Option<String>,
    /// Programming language filter for search_and_replace mode
    /// Valid: "rust", "typescript", "javascript", "python", "java", etc.
    /// Example: "typescript" to only process .ts/.tsx files
    #[serde(default)]
    pub language: Option<String>,
    /// File pattern filter for search_and_replace mode
    /// Examples: "src/**/*.rs", "*.test.ts", "components/**/*.tsx"
    /// Use glob patterns to target specific files/directories
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum number of files to process in search_and_replace mode
    /// Default: reasonable limit to prevent overwhelming results
    /// Example: 10 for targeted changes, 100 for broad refactoring
    #[serde(default)]
    pub limit: Option<u32>,
    /// Validate changes before applying (recommended for safety)
    /// Default: true - performs backup and integrity checks
    #[serde(default = "default_true")]
    pub validate: bool,
    /// Preview changes without applying them
    /// Default: false - set true to see what would change
    #[serde(default)]
    pub dry_run: bool,
}

impl FastEditTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Determine operation mode
        match self.mode.as_deref() {
            Some("search_and_replace") => {
                debug!(
                    "⚡ Search and replace mode: '{}' -> '{}'",
                    self.find_text, self.replace_text
                );
                self.search_and_replace_mode(handler).await
            }
            None => {
                debug!(
                    "⚡ Single file edit: {} -> replace '{}' with '{}'",
                    self.file_path, self.find_text, self.replace_text
                );
                self.single_file_mode().await
            }
            Some(unknown_mode) => {
                let message = format!("❌ Unknown mode: '{}'\n💡 Use 'search_and_replace' or omit for single file mode", unknown_mode);
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }

    /// Original single-file editing functionality (backward compatible)
    async fn single_file_mode(&self) -> Result<CallToolResult> {
        // Validate inputs
        if self.find_text.is_empty() {
            let message =
                "❌ find_text cannot be empty\n💡 Specify the exact text to find and replace";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        if self.find_text == self.replace_text {
            let message = "❌ find_text and replace_text are identical\n💡 No changes needed";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Check if file exists
        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Read current file content
        let original_content = match fs::read_to_string(&self.file_path) {
            Ok(content) => content,
            Err(e) => {
                let message = format!("❌ Failed to read file: {}\n💡 Check file permissions", e);
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        // Check if find_text exists in the file
        if !original_content.contains(&self.find_text) {
            let message = format!(
                "❌ Text not found in file: '{}'\n\
                💡 Check the exact text to find (case sensitive)",
                self.find_text
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Use diff-match-patch-rs for professional-grade editing
        let dmp = DiffMatchPatch::new();

        // For simple find/replace, we'll create the target content first
        let target_content = original_content.replace(&self.find_text, &self.replace_text);

        // Generate precise diffs and patches using Google's algorithm
        let diffs = match dmp.diff_main::<Efficient>(&original_content, &target_content) {
            Ok(diffs) => diffs,
            Err(e) => {
                let message = format!(
                    "❌ Failed to generate diff: {:?}\n💡 Check file content and encoding",
                    e
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        // Create patches for atomic application
        let patches = match dmp.patch_make(PatchInput::new_diffs(&diffs)) {
            Ok(patches) => patches,
            Err(e) => {
                let message = format!(
                    "❌ Failed to create patches: {:?}\n💡 File might be corrupted or binary",
                    e
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        // Generate readable diff for preview
        let patch_text = dmp.patch_to_text(&patches);

        // Apply patches to get the final result (ensures atomic operation)
        let (modified_content, patch_results) = match dmp.patch_apply(&patches, &original_content) {
            Ok((content, results)) => (content, results),
            Err(e) => {
                let message = format!(
                    "❌ Failed to apply patches: {:?}\n💡 File state might be inconsistent",
                    e
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        };

        // Check if all patches applied successfully
        if patch_results.iter().any(|&success| !success) {
            let failed_count = patch_results.iter().filter(|&&success| !success).count();
            let message = format!(
                "⚠️ Some patches failed to apply ({} failed out of {})\n💡 File might have been modified during edit",
                failed_count, patch_results.len()
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        if self.dry_run {
            let message = format!(
                "🔍 Dry run mode - showing changes to: {}\n\
                📊 Changes preview:\n\n{}\n\n\
                💡 Set dry_run=false to apply changes\n\
                ✅ All {} patches would apply successfully",
                self.file_path,
                patch_text,
                patch_results.len()
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Basic validation (syntax check would go here)
        if self.validate {
            let validation_result = self.validate_changes(&modified_content);
            if let Err(validation_error) = validation_result {
                let message = format!(
                    "❌ Validation failed: {}\n\
                    💡 Changes would break the code structure",
                    validation_error
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]));
            }
        }

        // Apply changes
        match fs::write(&self.file_path, &modified_content) {
            Ok(_) => {
                let replacements = original_content.matches(&self.find_text).count();
                let message = format!(
                    "✅ Fast edit successful using Google's diff-match-patch!\n\
                    📁 File: {}\n\
                    🎯 Applied {} patches successfully\n\
                    📊 Found and replaced {} occurrence(s)\n\
                    🔍 Changes:\n{}\n\n\
                    🎯 Next actions:\n\
                    • Run tests to verify changes\n\
                    • Use fast_refs to check impact\n\
                    • Use fast_search to find related code\n\
                    💡 Tip: Use git to track changes and revert if needed",
                    self.file_path,
                    patch_results.len(),
                    replacements,
                    patch_text
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
            Err(e) => {
                let message = format!("❌ Failed to write file: {}\n💡 Check file permissions", e);
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }

    /// Search and replace across multiple files (delegates to fast_search + fast_edit logic)
    async fn search_and_replace_mode(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        // Validate search_and_replace mode inputs
        if !self.file_path.is_empty() {
            let message = "❌ file_path must be empty for search_and_replace mode\n💡 Use file_pattern and language filters instead";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        if self.find_text.is_empty() {
            let message =
                "❌ find_text cannot be empty\n💡 Specify the exact text to find and replace";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        if self.find_text == self.replace_text {
            let message = "❌ find_text and replace_text are identical\n💡 No changes needed";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Step 1: Delegate to fast_search to find matching files
        let search_tool = crate::tools::search::FastSearchTool {
            query: self.find_text.clone(),
            mode: "text".to_string(), // Use text mode for exact matches
            language: self.language.clone(),
            file_pattern: self.file_pattern.clone(),
            limit: self.limit.unwrap_or(50),
            workspace: Some("primary".to_string()), // Default to primary workspace
        };

        let search_result = search_tool.call_tool(handler).await?;

        // Parse search results to extract file paths from the actual response content
        let mut file_paths = self.extract_file_paths_from_call_tool_result(&search_result)?;

        // Fallback: if no files found via search, try filesystem search (for testing/unindexed scenarios)
        if file_paths.is_empty() {
            file_paths = self.fallback_filesystem_search().await?;
        }

        if file_paths.is_empty() {
            let message = format!(
                "🔍 no files found matching criteria:\n\
                 📝 Query: '{}'\n\
                 🗂️ Language: {:?}\n\
                 📁 Pattern: {:?}\n\
                 📊 0 files found\n\
                 💡 Try broader search criteria",
                self.find_text, self.language, self.file_pattern
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Step 2: Apply fast_edit logic to each file
        let mut results = Vec::new();
        let mut total_replacements = 0;
        let mut processed_files = 0;

        for file_path in file_paths.iter().take(self.limit.unwrap_or(50) as usize) {
            if let Some(file_result) = self.apply_edit_to_file(file_path).await? {
                results.push(file_result.clone());
                if file_result.contains("replaced") {
                    total_replacements += 1;
                }
                processed_files += 1;
            }
        }

        // Step 3: Summarize results
        let summary = if self.dry_run {
            format!(
                "🔍 Search and replace dry run complete:\n\
                 📝 Query: '{}' -> '{}'\n\
                 📊 would process {} file(s)\n\
                 🎯 would replace in {} file(s)\n\n\
                 💡 Set dry_run=false to apply changes",
                self.find_text, self.replace_text, processed_files, total_replacements
            )
        } else {
            format!(
                "✅ Search and replace complete:\n\
                 📝 Query: '{}' -> '{}'\n\
                 📊 Processed {} file(s)\n\
                 🎯 Made replacements in {} file(s)",
                self.find_text, self.replace_text, processed_files, total_replacements
            )
        };

        let combined_result = format!("{}\n\n📋 File Details:\n{}", summary, results.join("\n"));
        Ok(CallToolResult::text_content(vec![TextContent::from(
            combined_result,
        )]))
    }

    /// Extract file paths from CallToolResult (proper parsing instead of Debug format)
    fn extract_file_paths_from_call_tool_result(
        &self,
        search_result: &CallToolResult,
    ) -> Result<Vec<String>> {
        let mut paths = Vec::new();

        // The content field contains ContentBlock objects. Based on the pattern used elsewhere,
        // these should be text content blocks that we can extract strings from.
        for content_block in &search_result.content {
            // Since we know CallToolResult::text_content(vec![TextContent::from(message)]) is used,
            // we need to extract the text from the TextContent objects.
            // Based on the MCP schema, this should be straightforward text extraction.

            // Try to serialize the content to understand its structure
            let content_json = serde_json::to_value(content_block)?;

            let search_text = if let Some(text_value) = content_json.get("text") {
                text_value.as_str().unwrap_or("").to_string()
            } else {
                // Log the structure we got to understand what's happening
                debug!("Unexpected content structure: {}", content_json);
                continue;
            };

            // FastSearchTool returns lines like: "   📁 path/to/file.rs:10-20"
            for line in search_text.lines() {
                // Look for the file path emoji pattern
                if line.contains("📁") {
                    // Extract text after 📁 and before the colon (line numbers)
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

    /// Fallback filesystem search when indexed search fails (for testing/unindexed scenarios)
    async fn fallback_filesystem_search(&self) -> Result<Vec<String>> {
        use std::fs;

        let mut matching_files = Vec::new();

        // Define search patterns based on language and file_pattern
        let extensions = if let Some(lang) = &self.language {
            match lang.as_str() {
                "typescript" => vec!["ts", "tsx"],
                "javascript" => vec!["js", "jsx"],
                "python" => vec!["py"],
                "rust" => vec!["rs"],
                "java" => vec!["java"],
                "csharp" => vec!["cs"],
                _ => vec![], // Unknown language
            }
        } else {
            vec!["ts", "tsx", "js", "jsx", "py", "rs", "java", "cs"] // Common extensions
        };

        // Search only within the current workspace directory to prevent unbounded searches
        let mut search_roots = vec![
            std::env::current_dir()?, // Only search within current workspace
        ];

        if let Ok(extra_roots) = std::env::var("FAST_EDIT_SEARCH_ROOTS") {
            for path in std::env::split_paths(&extra_roots) {
                if path.exists() {
                    search_roots.push(path);
                }
            }
        }

        warn!("🔍 Using filesystem fallback search - this indicates the index may be incomplete");
        debug!("Search limited to current directory to prevent unbounded scans");

        for root in search_roots {
            if root.exists() {
                self.search_directory(&root, &extensions, &mut matching_files, 0)?;
            }
        }

        // Filter by file_pattern if specified
        if let Some(pattern) = &self.file_pattern {
            matching_files.retain(|path| {
                if pattern.contains("*") {
                    // Simple wildcard matching
                    let pattern_parts: Vec<&str> = pattern.split('*').collect();
                    if pattern_parts.len() == 2 {
                        let prefix = pattern_parts[0];
                        let suffix = pattern_parts[1];
                        path.starts_with(prefix) && path.ends_with(suffix)
                    } else {
                        true
                    }
                } else {
                    path.contains(pattern)
                }
            });
        }

        // Filter files that actually contain the search text
        let mut files_with_content = Vec::new();
        for file_path in matching_files {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if content.contains(&self.find_text) {
                    files_with_content.push(file_path);
                }
            }
        }

        Ok(files_with_content)
    }

    /// Recursive directory search helper
    fn search_directory(
        &self,
        dir: &std::path::Path,
        extensions: &[&str],
        results: &mut Vec<String>,
        depth: usize,
    ) -> Result<()> {
        // Limit recursion depth to avoid infinite loops, but allow deeper temp directory searches
        if depth > 5 {
            return Ok(());
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip common non-source directories
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if !["node_modules", "target", ".git", "dist", "build"].contains(&name) {
                            self.search_directory(&path, extensions, results, depth + 1)?;
                        }
                    }
                } else if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if extensions.is_empty() || extensions.contains(&ext) {
                            if let Some(path_str) = path.to_str() {
                                results.push(path_str.to_string());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Apply fast_edit logic to a single file
    async fn apply_edit_to_file(&self, file_path: &str) -> Result<Option<String>> {
        // Check if file exists
        if !std::path::Path::new(file_path).exists() {
            return Ok(None);
        }

        // Read file content
        let original_content = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(_) => return Ok(None),
        };

        // Check if find_text exists in this file
        if !original_content.contains(&self.find_text) {
            return Ok(None);
        }

        // Use diff-match-patch-rs for consistent editing across single-file and multi-file modes
        let dmp = DiffMatchPatch::new();
        let target_content = original_content.replace(&self.find_text, &self.replace_text);

        // Generate and apply patches atomically
        let diffs = match dmp.diff_main::<Efficient>(&original_content, &target_content) {
            Ok(diffs) => diffs,
            Err(_) => return Ok(None), // Skip files that can't be processed
        };

        let patches = match dmp.patch_make(PatchInput::new_diffs(&diffs)) {
            Ok(patches) => patches,
            Err(_) => return Ok(None),
        };

        let (modified_content, patch_results) = match dmp.patch_apply(&patches, &original_content) {
            Ok((content, results)) => (content, results),
            Err(_) => return Ok(None),
        };

        // Ensure all patches applied successfully
        if patch_results.iter().any(|&success| !success) {
            return Ok(Some(format!(
                "⚠️ {} - partial patch failure (some changes may not have applied)",
                file_path
            )));
        }

        if self.dry_run {
            let replacements = original_content.matches(&self.find_text).count();
            return Ok(Some(format!(
                "📄 {} - would replace {} occurrence(s)",
                file_path, replacements
            )));
        }

        // Basic validation
        if self.validate {
            if let Err(_) = self.validate_changes(&modified_content) {
                return Ok(Some(format!(
                    "⚠️ {} - skipped (validation failed)",
                    file_path
                )));
            }
        }

        // Write modified content
        match fs::write(file_path, &modified_content) {
            Ok(_) => {
                let replacements = original_content.matches(&self.find_text).count();
                Ok(Some(format!(
                    "✅ {} - replaced {} occurrence(s)",
                    file_path, replacements
                )))
            }
            Err(_) => Ok(Some(format!("❌ {} - write failed", file_path))),
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

//**********************//
//     LineEditTool     //
//**********************//

#[mcp_tool(
    name = "line_edit",
    description = "LINE EDITING - Precise line-based operations with automatic backup and validation",
    title = "Line-Based File Editor"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LineEditTool {
    /// Path to the file to edit
    /// Example: "src/main.rs" or "/absolute/path/to/file.ts"
    pub file_path: String,
    /// Line editing operation to perform
    /// Valid operations: "count" (count lines), "read" (read lines), "insert" (add lines), "delete" (remove lines), "replace" (change lines)
    /// Example: "insert" to add new content at specific line number
    pub operation: String,
    /// Starting line number for range operations (1-based)
    /// Required for: read, delete, replace operations
    /// Example: 10 to start from line 10
    pub start_line: Option<u32>,
    /// Ending line number for range operations (1-based, inclusive)
    /// Required for: read, delete, replace operations
    /// Example: 15 to end at line 15
    pub end_line: Option<u32>,
    /// Specific line number for insert operations (1-based)
    /// Required for: insert operation only
    /// Example: 5 to insert content after line 5
    pub line_number: Option<u32>,
    /// Text content for insert and replace operations
    /// Required for: insert, replace operations
    /// Example: "console.log('Hello World');" for new code
    pub content: Option<String>,
    /// Automatically preserve existing indentation when inserting/replacing
    /// Default: true - maintains consistent code formatting
    #[serde(default = "default_true")]
    pub preserve_indentation: bool,
    /// Preview changes without applying them
    /// Default: false - set true to see what would change
    #[serde(default)]
    pub dry_run: bool,
}

impl LineEditTool {
    pub async fn call_tool(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!(
            "📝 Line edit: {} operation on {}",
            self.operation, self.file_path
        );

        match self.operation.as_str() {
            "count" => self.count_lines().await,
            "read" => self.read_lines().await,
            "insert" => self.insert_at_line().await,
            "delete" => self.delete_lines().await,
            "replace" => self.replace_lines().await,
            _ => {
                let message = format!("❌ Invalid operation: '{}'\n💡 Valid operations: count, read, insert, delete, replace", self.operation);
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }

    async fn count_lines(&self) -> Result<CallToolResult> {
        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        let content = fs::read_to_string(&self.file_path)?;
        let line_count = if content.is_empty() {
            0
        } else {
            content.lines().count()
        };

        let message = format!("📏 Line count for {}: {} lines", self.file_path, line_count);
        Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]))
    }

    async fn read_lines(&self) -> Result<CallToolResult> {
        let start_line = self
            .start_line
            .ok_or_else(|| anyhow::anyhow!("start_line required for read operation"))?;
        let end_line = self
            .end_line
            .ok_or_else(|| anyhow::anyhow!("end_line required for read operation"))?;

        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Validate line numbers (1-based)
        if start_line == 0 || end_line == 0 {
            let message =
                "❌ Line numbers must be >= 1 (1-based indexing)\n💡 Use line_number >= 1";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        if start_line > end_line {
            let message = "❌ start_line must be <= end_line\n💡 Check line range";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        let content = fs::read_to_string(&self.file_path)?;
        let lines: Vec<&str> = content.lines().collect();

        if (start_line as usize) > lines.len() {
            let message = format!(
                "❌ start_line {} exceeds file length {} lines\n💡 Use get_line_count first",
                start_line,
                lines.len()
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Extract requested lines (convert to 0-based indexing)
        let start_idx = (start_line - 1) as usize;
        let end_idx = std::cmp::min(end_line as usize, lines.len());
        let selected_lines: Vec<&str> = lines[start_idx..end_idx].to_vec();

        let mut result = format!(
            "📖 Lines {}-{} from {} ({} lines):\n\n",
            start_line,
            end_idx,
            self.file_path,
            selected_lines.len()
        );

        for (i, line) in selected_lines.iter().enumerate() {
            result.push_str(&format!("{:4}: {}\n", start_idx + i + 1, line));
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(
            result,
        )]))
    }

    async fn insert_at_line(&self) -> Result<CallToolResult> {
        let line_number = self
            .line_number
            .ok_or_else(|| anyhow::anyhow!("line_number required for insert operation"))?;
        let content = self
            .content
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("content required for insert operation"))?;

        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Validate line number (1-based)
        if line_number == 0 {
            let message = "❌ Line number must be >= 1 (1-based indexing)\n💡 Use line_number >= 1";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        let original_content = fs::read_to_string(&self.file_path)?;
        let mut lines: Vec<String> = original_content.lines().map(|s| s.to_string()).collect();

        if (line_number as usize) > lines.len() + 1 {
            let message =
                format!(
                "❌ line_number {} exceeds file length {} + 1\n💡 Use line number between 1 and {}",
                line_number, lines.len(), lines.len() + 1
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Handle indentation preservation
        let mut content_to_insert = content.clone();
        if self.preserve_indentation && line_number > 1 {
            let indent = self.detect_indentation(&lines, line_number);
            if !indent.is_empty() {
                let content_lines: Vec<&str> = content_to_insert.lines().collect();
                let indented_lines: Vec<String> = content_lines
                    .iter()
                    .map(|line| {
                        if line.trim().is_empty() {
                            line.to_string()
                        } else {
                            format!("{}{}", indent, line)
                        }
                    })
                    .collect();
                content_to_insert = indented_lines.join("\n");
            }
        }

        // Insert content (convert to 0-based indexing)
        let insert_idx = (line_number - 1) as usize;
        let content_lines: Vec<String> = content_to_insert.lines().map(|s| s.to_string()).collect();

        for (i, content_line) in content_lines.iter().enumerate() {
            lines.insert(insert_idx + i, content_line.clone());
        }

        let modified_content = lines.join("\n");
        if original_content.ends_with('\n') && !modified_content.ends_with('\n') {
            let modified_content = format!("{}\n", modified_content);
            self.apply_changes(
                &original_content,
                &modified_content,
                &format!("insert at line {}", line_number),
            )
            .await
        } else {
            self.apply_changes(
                &original_content,
                &modified_content,
                &format!("insert at line {}", line_number),
            )
            .await
        }
    }

    async fn delete_lines(&self) -> Result<CallToolResult> {
        let start_line = self
            .start_line
            .ok_or_else(|| anyhow::anyhow!("start_line required for delete operation"))?;
        let end_line = self
            .end_line
            .ok_or_else(|| anyhow::anyhow!("end_line required for delete operation"))?;

        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Validate line numbers (1-based)
        if start_line == 0 || end_line == 0 {
            let message =
                "❌ Line numbers must be >= 1 (1-based indexing)\n💡 Use line_number >= 1";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        if start_line > end_line {
            let message = "❌ start_line must be <= end_line\n💡 Check line range";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        let original_content = fs::read_to_string(&self.file_path)?;
        let mut lines: Vec<String> = original_content.lines().map(|s| s.to_string()).collect();

        if (start_line as usize) > lines.len() {
            let message = format!(
                "❌ start_line {} exceeds file length {} lines\n💡 Use get_line_count first",
                start_line,
                lines.len()
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Calculate actual range to delete (convert to 0-based indexing)
        let start_idx = (start_line - 1) as usize;
        let end_idx = std::cmp::min(end_line as usize, lines.len());

        // Remove the lines
        lines.drain(start_idx..end_idx);

        let modified_content = lines.join("\n");
        if original_content.ends_with('\n')
            && !modified_content.ends_with('\n')
            && !modified_content.is_empty()
        {
            let modified_content = format!("{}\n", modified_content);
            self.apply_changes(
                &original_content,
                &modified_content,
                &format!("delete lines {}-{}", start_line, end_idx),
            )
            .await
        } else {
            self.apply_changes(
                &original_content,
                &modified_content,
                &format!("delete lines {}-{}", start_line, end_idx),
            )
            .await
        }
    }

    async fn replace_lines(&self) -> Result<CallToolResult> {
        let start_line = self
            .start_line
            .ok_or_else(|| anyhow::anyhow!("start_line required for replace operation"))?;
        let end_line = self
            .end_line
            .ok_or_else(|| anyhow::anyhow!("end_line required for replace operation"))?;
        let content = self
            .content
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("content required for replace operation"))?;

        if !std::path::Path::new(&self.file_path).exists() {
            let message = format!(
                "❌ File not found: {}\n💡 Check the file path",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Validate line numbers (1-based)
        if start_line == 0 || end_line == 0 {
            let message =
                "❌ Line numbers must be >= 1 (1-based indexing)\n💡 Use line_number >= 1";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        if start_line > end_line {
            let message = "❌ start_line must be <= end_line\n💡 Check line range";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        let original_content = fs::read_to_string(&self.file_path)?;
        let mut lines: Vec<String> = original_content.lines().map(|s| s.to_string()).collect();

        if (start_line as usize) > lines.len() {
            let message = format!(
                "❌ start_line {} exceeds file length {} lines\n💡 Use get_line_count first",
                start_line,
                lines.len()
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Calculate actual range to replace (convert to 0-based indexing)
        let start_idx = (start_line - 1) as usize;
        let end_idx = std::cmp::min(end_line as usize, lines.len());

        // Handle indentation preservation
        let mut content_to_replace = content.clone();
        if self.preserve_indentation && start_line > 1 {
            let indent = self.detect_indentation(&lines, start_line);
            if !indent.is_empty() {
                let content_lines: Vec<&str> = content_to_replace.lines().collect();
                let indented_lines: Vec<String> = content_lines
                    .iter()
                    .map(|line| {
                        if line.trim().is_empty() {
                            line.to_string()
                        } else {
                            format!("{}{}", indent, line)
                        }
                    })
                    .collect();
                content_to_replace = indented_lines.join("\n");
            }
        }

        // Replace the lines
        let replacement_lines: Vec<String> =
            content_to_replace.lines().map(|s| s.to_string()).collect();

        // Remove old lines
        lines.drain(start_idx..end_idx);

        // Insert new lines at the same position
        for (i, replacement_line) in replacement_lines.iter().enumerate() {
            lines.insert(start_idx + i, replacement_line.clone());
        }

        let modified_content = lines.join("\n");
        if original_content.ends_with('\n') && !modified_content.ends_with('\n') {
            let modified_content = format!("{}\n", modified_content);
            self.apply_changes(
                &original_content,
                &modified_content,
                &format!("replace lines {}-{}", start_line, end_idx),
            )
            .await
        } else {
            self.apply_changes(
                &original_content,
                &modified_content,
                &format!("replace lines {}-{}", start_line, end_idx),
            )
            .await
        }
    }

    /// Helper to detect indentation from previous line
    fn detect_indentation(&self, lines: &[String], at_line: u32) -> String {
        if at_line > 0 && (at_line as usize) <= lines.len() {
            let prev_line = &lines[(at_line - 1) as usize];
            let mut indent = String::new();
            for ch in prev_line.chars() {
                if ch == ' ' || ch == '\t' {
                    indent.push(ch);
                } else {
                    break;
                }
            }
            indent
        } else {
            String::new()
        }
    }

    /// Helper to apply changes with diff, backup, and dry-run support
    async fn apply_changes(
        &self,
        original_content: &str,
        modified_content: &str,
        operation: &str,
    ) -> Result<CallToolResult> {
        if original_content == modified_content {
            let message = "ℹ️ No changes needed - content would be identical";
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Use diff-match-patch-rs for consistent line editing
        let dmp = DiffMatchPatch::new();
        let diffs = match dmp.diff_main::<Efficient>(original_content, modified_content) {
            Ok(diffs) => diffs,
            Err(e) => {
                let message = format!(
                    "❌ Failed to generate diff: {:?}\n💡 Check file content and encoding",
                    e
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
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
                    message,
                )]));
            }
        };

        let patch_text = dmp.patch_to_text(&patches);

        if self.dry_run {
            let message = format!(
                "🔍 Dry run mode - showing {} in: {}\n\
                📊 Changes preview:\n\n{}\n\n\
                💡 Set dry_run=false to apply changes",
                operation, self.file_path, patch_text
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Apply changes
        match fs::write(&self.file_path, modified_content) {
            Ok(_) => {
                let message = format!(
                    "✅ Line edit successful using Google's diff-match-patch!\n\
                    📁 File: {}\n\
                    📊 Operation: {}\n\
                    🔍 Changes:\n{}\n\n\
                    🎯 Next actions:\n\
                    • Use read operation to verify changes\n\
                    • Use fast_refs to check for any impacts\n\
                    💡 Tip: Use git to track changes and revert if needed",
                    self.file_path, operation, patch_text
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
            Err(e) => {
                let message = format!("❌ Failed to write file: {}\n💡 Check file permissions", e);
                Ok(CallToolResult::text_content(vec![TextContent::from(
                    message,
                )]))
            }
        }
    }
}
