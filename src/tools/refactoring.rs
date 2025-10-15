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
use tree_sitter::Parser;

use crate::handler::JulieServerHandler;
use crate::tools::editing::EditingTransaction; // Atomic file operations
use crate::tools::navigation::FastRefsTool;

/// Structured result from smart refactoring operations
#[derive(Debug, Clone, Serialize)]
pub struct SmartRefactorResult {
    pub tool: String,
    pub operation: String,
    pub dry_run: bool,
    pub success: bool,
    pub files_modified: Vec<String>,
    pub changes_count: usize,
    pub next_actions: Vec<String>,
    /// Operation-specific metadata (flexible JSON for different operation types)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Syntax error detected by tree-sitter
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SyntaxError {
    /// Line number where error occurs (1-based)
    pub line: u32,
    /// Column number where error occurs (0-based)
    pub column: u32,
    /// Error description
    pub message: String,
    /// Severity: "error" or "warning"
    pub severity: String,
    /// Suggested fix if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
    /// Code snippet showing the error context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Result of auto-fix operation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutoFixResult {
    /// Whether any fixes were applied
    pub fixes_applied: bool,
    /// Number of fixes applied
    pub fix_count: u32,
    /// List of fixes that were applied
    pub fixes: Vec<String>,
    /// Errors remaining after fixes
    pub remaining_errors: Vec<SyntaxError>,
    /// Fixed file content (if fixes were applied)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_content: Option<String>,
}

/// Delimiter error detected by tree-sitter (internal use)
#[derive(Debug, Clone)]
struct DelimiterError {
    /// Line number where error occurs
    #[allow(dead_code)]
    line: usize,
    /// Column number where error occurs (for future use)
    #[allow(dead_code)]
    _column: usize,
    /// Missing delimiter character(s)
    missing_delimiter: String,
    /// Type of error (unmatched_brace, unclosed_string, etc.) (for future use)
    #[allow(dead_code)]
    _error_type: String,
}

/// Available refactoring operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RefactorOperation {
    /// Rename a symbol across the codebase
    RenameSymbol,
    /// Extract selected code into a new function
    ExtractFunction,
    /// Replace the entire body/definition of a symbol (Serena-inspired)
    ReplaceSymbolBody,
    /// Insert code before or after a symbol
    InsertRelativeToSymbol,
    /// Extract inline types to named type definitions (TypeScript/Rust)
    ExtractType,
    /// Fix broken import statements after file moves
    UpdateImports,
    /// Inline a variable by replacing all uses with its value
    InlineVariable,
    /// Inline a function by replacing calls with function body
    InlineFunction,
    /// Validate syntax using tree-sitter error detection
    /// Reports errors but doesn't attempt automatic fixes - agent handles corrections
    ValidateSyntax,
}

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
            let message = format!("No references found for symbol '{}'", old_name);
            return self.create_result(
                "rename_symbol",
                false, // Failed to find symbol
                vec![],
                0,
                vec![
                    "Use fast_search to locate the symbol".to_string(),
                    "Check spelling of symbol name".to_string(),
                ],
                message,
                None,
            );
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

        for file_path in file_locations.keys() {
            match self
                .rename_in_file(
                    handler,
                    file_path,
                    old_name,
                    new_name,
                    update_comments,
                    &dmp,
                )
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
            let message = format!(
                "DRY RUN: Rename '{}' -> '{}'\nWould modify {} files with {} changes",
                old_name, new_name, total_files, total_changes
            );

            let files: Vec<String> = renamed_files.iter().map(|(f, _)| f.clone()).collect();
            return self.create_result(
                "rename_symbol",
                true, // Dry run succeeded
                files,
                total_changes,
                vec!["Set dry_run=false to apply changes".to_string()],
                message,
                None,
            );
        }

        // Final success message
        let message = format!(
            "Rename successful: '{}' -> '{}'\nModified {} files with {} changes",
            old_name, new_name, total_files, total_changes
        );

        let files: Vec<String> = renamed_files.iter().map(|(f, _)| f.clone()).collect();
        self.create_result(
            "rename_symbol",
            true,
            files,
            total_changes,
            vec![
                "Run tests to verify changes".to_string(),
                "Use fast_refs to validate rename completion".to_string(),
                "Use git diff to review changes".to_string(),
            ],
            message,
            None,
        )
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

        // Prefer structured payloads when available
        if let Some(structured) = &refs_result.structured_content {
            if let Some(references) = structured.get("references").and_then(|v| v.as_array()) {
                for reference in references {
                    if let (Some(file_path), Some(line_number)) = (
                        reference.get("file_path").and_then(|v| v.as_str()),
                        reference.get("line_number").and_then(|v| v.as_u64()),
                    ) {
                        file_locations
                            .entry(file_path.to_string())
                            .or_default()
                            .push(line_number as u32);
                    }
                }
            }

            if let Some(definitions) = structured.get("definitions").and_then(|v| v.as_array()) {
                for definition in definitions {
                    if let (Some(file_path), Some(line_number)) = (
                        definition.get("file_path").and_then(|v| v.as_str()),
                        definition.get("start_line").and_then(|v| v.as_u64()),
                    ) {
                        file_locations
                            .entry(file_path.to_string())
                            .or_default()
                            .push(line_number as u32);
                    }
                }
            }

            if !file_locations.is_empty() {
                return Ok(file_locations);
            }
        }

        // Parse textual fallback (expected format: "file_path:line_number")
        for line in content.lines() {
            let after_dash = line
                .split_once(" - ")
                .map(|(_, rest)| rest)
                .unwrap_or_else(|| line.trim());

            let mut selected: Option<(&str, &str)> = None;
            for (idx, _) in after_dash.match_indices(':') {
                if let Some(remainder) = after_dash.get(idx + 1..) {
                    let trimmed = remainder.trim_start();
                    if trimmed
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false)
                    {
                        selected = Some((&after_dash[..idx], trimmed));
                        break;
                    }
                }
            }

            if let Some((file_part, line_part)) = selected {
                let digits: String = line_part
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();

                if let Ok(line_num) = digits.parse::<u32>() {
                    let file_path = file_part.trim();
                    file_locations
                        .entry(file_path.to_string())
                        .or_default()
                        .push(line_num);
                }
            }
        }

        Ok(file_locations)
    }

    /// AST-aware rename using Julie's search engine to find exact symbol matches
    /// This replaces only actual symbol references, not string literals or comments
    pub async fn rename_in_file(
        &self,
        handler: &JulieServerHandler,
        file_path: &str,
        old_name: &str,
        new_name: &str,
        update_comments: bool,
        dmp: &DiffMatchPatch,
    ) -> Result<usize> {
        // Read the file
        let original_content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;

        // AST-aware replacement using SearchEngine to find exact symbol matches
        let new_content = match self
            .ast_aware_replace(
                &original_content,
                file_path,
                old_name,
                new_name,
                update_comments,
                handler,
            )
            .await
        {
            Ok(content) => content,
            Err(e) => {
                // Fallback to simple replacement if AST search fails
                debug!(
                    "⚠️ AST search failed, falling back to simple replacement: {}",
                    e
                );
                original_content.replace(old_name, new_name)
            }
        };

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

            // Write the final content atomically using EditingTransaction
        let transaction = EditingTransaction::begin(&file_path)?;
            transaction.commit(&final_content)?;
        }

        Ok(changes_count)
    }

    /// AST-aware replacement using direct tree-sitter parsing for precise symbol renaming
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
        debug!("🌳 Starting AST-aware replacement using tree-sitter parsing");

        // Try search engine first, but fall back to tree-sitter parsing if not available
        let symbol_positions = match self
            .find_symbols_via_search(file_path, old_name, handler)
            .await
        {
            Ok(positions) => {
                debug!("✅ Found {} symbols via search engine", positions.len());
                positions
            }
            Err(search_error) => {
                debug!(
                    "⚠️ Search engine failed: {}, falling back to tree-sitter parsing",
                    search_error
                );
                self.find_symbols_via_treesitter(content, file_path, old_name)
                    .await?
            }
        };

        // Hybrid approach: Use AST for validation + careful text replacement for completeness
        if !symbol_positions.is_empty() {
            debug!(
                "✅ AST validation passed: found {} symbol definitions",
                symbol_positions.len()
            );
        } else {
            debug!("⚠️ No AST symbol definitions found - symbol may not exist");
            return Err(anyhow::anyhow!("Symbol not found in AST"));
        }

        // Use AST-aware text replacement to catch ALL occurrences (including usages)
        debug!(
            "📝 About to call AST-aware smart_text_replace with old_name='{}', new_name='{}'",
            old_name, new_name
        );
        let result =
            self.smart_text_replace(content, old_name, new_name, file_path, update_comments)?;
        debug!(
            "🎯 AST-aware smart_text_replace completed, result length: {}",
            result.len()
        );

        Ok(result)
    }

    /// Find symbol positions using SQLite database (for indexed files)
    async fn find_symbols_via_search(
        &self,
        file_path: &str,
        old_name: &str,
        handler: &JulieServerHandler,
    ) -> Result<Vec<(u32, u32)>> {
        // Get workspace and database
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("Workspace not initialized"))?;

        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Database not initialized"))?;

        let db_lock = db.lock().unwrap();

        // Search for symbols by name in the specific file
        let symbols = db_lock.find_symbols_by_name(old_name)?;

        let positions: Vec<(u32, u32)> = symbols
            .into_iter()
            .filter(|symbol| symbol.file_path == file_path)
            .map(|symbol| (symbol.start_byte, symbol.end_byte))
            .collect();

        Ok(positions)
    }

    /// Find symbol positions using direct tree-sitter parsing (for any file)
    async fn find_symbols_via_treesitter(
        &self,
        content: &str,
        file_path: &str,
        old_name: &str,
    ) -> Result<Vec<(u32, u32)>> {
        use crate::extractors::ExtractorManager;

        debug!("🌳 Using tree-sitter to find symbols in {}", file_path);

        // Create an extractor manager and extract all symbols
        let extractor_manager = ExtractorManager::new();
        let symbols = extractor_manager.extract_symbols(file_path, content)?;

        // Find symbols that match our target name exactly
        let matching_positions: Vec<(u32, u32)> = symbols
            .into_iter()
            .filter(|symbol| symbol.name == old_name)
            .map(|symbol| (symbol.start_byte, symbol.end_byte))
            .collect();

        debug!(
            "🎯 Tree-sitter found {} matching symbols for '{}'",
            matching_positions.len(),
            old_name
        );

        Ok(matching_positions)
    }

    /// AST-AWARE text replacement using tree-sitter
    /// This is Julie's core value proposition - language-aware refactoring!
    /// Uses tree-sitter AST to find ONLY actual code symbols, skipping strings/comments.
    pub fn smart_text_replace(
        &self,
        content: &str,
        old_name: &str,
        new_name: &str,
        file_path: &str,
        update_comments: bool,
    ) -> Result<String> {
        use crate::tools::ast_symbol_finder::{ASTSymbolFinder, SymbolContext};
        use tree_sitter::Parser;

        debug!(
            "🌳 AST-aware replacement: '{}' -> '{}' using tree-sitter",
            old_name, new_name
        );

        // Determine language from file extension
        let language = self.detect_language(file_path);

        // Parse file with tree-sitter
        let mut parser = Parser::new();
        let tree_sitter_language = match self.get_tree_sitter_language(&language) {
            Ok(lang) => lang,
            Err(e) => {
                debug!(
                    "⚠️ Couldn't get tree-sitter language for {}: {}. Fallback to text-based.",
                    language, e
                );
                // Fallback to simple word boundary replacement if tree-sitter fails
                let pattern = format!(r"\b{}\b", regex::escape(old_name));
                let re = regex::Regex::new(&pattern)?;
                return Ok(re.replace_all(content, new_name).to_string());
            }
        };

        parser
            .set_language(&tree_sitter_language)
            .map_err(|e| anyhow::anyhow!("Failed to set parser language: {}", e))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file"))?;

        // Use ASTSymbolFinder to find all symbol occurrences
        let finder = ASTSymbolFinder::new(content.to_string(), tree, language.clone());
        let occurrences = finder.find_symbol_occurrences(old_name);

        let total_occurrences = occurrences.len();
        debug!(
            "🔍 Found {} total occurrences of '{}'",
            total_occurrences, old_name
        );

        // Filter out occurrences in strings and comments
        let code_occurrences: Vec<_> = occurrences
            .into_iter()
            .filter(|occ| {
                occ.context != SymbolContext::StringLiteral && occ.context != SymbolContext::Comment
            })
            .collect();

        debug!(
            "✅ {} code occurrences (filtered out {} string/comment occurrences)",
            code_occurrences.len(),
            total_occurrences - code_occurrences.len()
        );

        if code_occurrences.is_empty() {
            debug!("⚠️ No code occurrences found to replace");
            return Ok(content.to_string());
        }

        // Sort by start_byte descending to apply replacements from end to start
        // This preserves byte offsets as we modify
        let mut sorted_occurrences = code_occurrences;
        sorted_occurrences.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));

        let replacement_count = sorted_occurrences.len();

        // Apply replacements
        let mut result = content.to_string();
        for occ in sorted_occurrences {
            result.replace_range(occ.start_byte..occ.end_byte, new_name);
            debug!(
                "✅ Replaced '{}' -> '{}' at byte {}:{} (line {}, context: {:?})",
                old_name, new_name, occ.start_byte, occ.end_byte, occ.line, occ.context
            );
        }

        // Optionally rename documentation comments inside the symbol's definition scope
        if update_comments {
            if let Some(definition) = finder.find_symbol_definition(old_name) {
                let comment_search_start = definition
                    .name_range
                    .0
                    .saturating_sub(DOC_COMMENT_LOOKBACK_BYTES);
                let mut comment_occurrences = finder.find_comment_occurrences(
                    old_name,
                    Some((comment_search_start, definition.body_range.1)),
                );

                if !comment_occurrences.is_empty() {
                    let identifier_char_checker =
                        |ch: char| ch.is_alphanumeric() || ch == '_' || ch == '$';
                    let quoted_old = format!("\"{}\"", old_name);
                    let single_quoted_old = format!("'{}'", old_name);
                    let backticked_old = format!("`{}`", old_name);

                    debug!(
                        "📝 Considering {} comment occurrences for renaming",
                        comment_occurrences.len()
                    );

                    comment_occurrences.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));

                    for occ in comment_occurrences {
                        let start = occ.start_byte;
                        let end = occ.end_byte;
                        if start >= end || end > result.len() {
                            continue;
                        }

                        let original_segment = &result[start..end];
                        let is_doc_comment = looks_like_doc_comment(original_segment);
                        let near_scope_top = occ.line >= definition.line
                            && occ.line <= definition.line + TOP_OF_SCOPE_COMMENT_LINE_WINDOW;

                        if !is_doc_comment && !near_scope_top {
                            debug!("ℹ️ Skipping deep comment occurrence at line {}", occ.line);
                            continue;
                        }

                        if original_segment.contains(&quoted_old)
                            || original_segment.contains(&single_quoted_old)
                            || original_segment.contains(&backticked_old)
                        {
                            debug!(
                                "ℹ️ Skipping comment occurrence at line {} due to quoted symbol",
                                occ.line
                            );
                            continue;
                        }

                        let (updated_segment, changed) = replace_identifier_with_boundaries(
                            original_segment,
                            old_name,
                            new_name,
                            &identifier_char_checker,
                        );

                        if changed {
                            result.replace_range(start..end, &updated_segment);
                            debug!(
                                "📝 Updated comment at line {} (context: {:?}, doc: {}, near_scope_top: {})",
                                occ.line,
                                occ.context,
                                is_doc_comment,
                                near_scope_top
                            );
                        } else {
                            debug!(
                                "ℹ️ No safe replacements found in comment at line {}",
                                occ.line
                            );
                        }
                    }
                }
            }
        }

        debug!(
            "🎯 AST-aware replacement complete: {} occurrences replaced",
            replacement_count
        );
        Ok(result)
    }

    /// Get tree-sitter language for file type (delegates to shared language module)
    fn get_tree_sitter_language(&self, language: &str) -> Result<tree_sitter::Language> {
        crate::language::get_tree_sitter_language(language)
    }

    /// Handle extract function operation
    async fn handle_extract_function(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        debug!("🔄 Processing extract function operation");

        // Parse JSON parameters (validate required fields even though feature is pending)
        let params: JsonValue = serde_json::from_str(&self.params)
            .map_err(|e| anyhow::anyhow!("Invalid JSON in params: {}", e))?;

        let file_path = params
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file"))?;

        let start_line = params
            .get("start_line")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: start_line"))?
            as u32;

        let end_line = params
            .get("end_line")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: end_line"))?
            as u32;

        let function_name = params
            .get("function_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: function_name"))?;

        if start_line > end_line {
            return Err(anyhow::anyhow!("start_line must be <= end_line"));
        }

        debug!(
            "🎯 Extract function '{}' from {}:{}-{}",
            function_name, file_path, start_line, end_line
        );

        let message = format!(
            "🚧 Extract function is not yet implemented\n\
            📁 File: {}\n\
            📍 Lines: {}-{}\n\
            🎯 Function name: {}\n\n\
            💡 Coming soon - will extract selected code into a new function\n\
            📋 Use ReplaceSymbolBody operation for now",
            file_path, start_line, end_line, function_name
        );

        self.create_result(
            "extract_function",
            false, // Not yet implemented
            vec![],
            0,
            vec!["Use replace_symbol_body for manual refactoring".to_string()],
            message,
            Some(serde_json::json!({
                "file": file_path,
                "start_line": start_line,
                "end_line": end_line,
                "function_name": function_name,
            })),
        )
    }

    /// Detect the base indentation level of code lines
    #[allow(dead_code)]
    fn detect_base_indentation(&self, lines: &[&str]) -> usize {
        lines
            .iter()
            .filter(|line| !line.trim().is_empty()) // Skip empty lines
            .map(|line| line.len() - line.trim_start().len()) // Count leading whitespace
            .min()
            .unwrap_or(0)
    }

    /// Remove base indentation from code lines
    #[allow(dead_code)]
    fn dedent_code(&self, lines: &[&str], base_indent: usize) -> String {
        lines
            .iter()
            .map(|line| {
                if line.trim().is_empty() {
                    "" // Keep empty lines empty
                } else if line.len() > base_indent {
                    &line[base_indent..] // Remove base indentation
                } else {
                    line.trim_start() // Line has less indentation than base
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 🌳 AST-AWARE: Analyze what variables/dependencies the extracted code needs
    ///
    /// Uses tree-sitter to parse the code and identify external dependencies.
    /// A dependency is any identifier that is REFERENCED but not DEFINED within the code block.
    ///
    /// Example:
    /// ```rust
    /// let result = user.name.to_uppercase();  // "user" is a dependency
    /// println!("{}", result);                 // "result" is defined locally, not a dependency
    /// ```
    #[allow(dead_code)]
    async fn analyze_dependencies(&self, code: &str, file_path: &str) -> Result<Vec<String>> {
        use std::collections::HashSet;
        use tree_sitter::Parser;

        debug!("🌳 AST-based dependency analysis for extracted code");

        // Detect language from file extension
        let language = self.detect_language(file_path);

        // Parse code with tree-sitter
        let mut parser = Parser::new();
        let tree_sitter_language = match self.get_tree_sitter_language(&language) {
            Ok(lang) => lang,
            Err(e) => {
                debug!(
                    "⚠️ Tree-sitter not available for {}: {}. Using fallback.",
                    language, e
                );
                return self.analyze_dependencies_fallback(code);
            }
        };

        if let Err(e) = parser.set_language(&tree_sitter_language) {
            debug!("⚠️ Failed to set language: {}. Using fallback.", e);
            return self.analyze_dependencies_fallback(code);
        }

        let tree = match parser.parse(code, None) {
            Some(t) => t,
            None => {
                debug!("⚠️ Failed to parse code. Using fallback.");
                return self.analyze_dependencies_fallback(code);
            }
        };

        let root = tree.root_node();

        // Track all identifiers referenced and defined
        let mut all_references = HashSet::new();
        let mut definitions = HashSet::new();

        // Walk AST to collect identifiers
        self.collect_identifiers(
            root,
            code.as_bytes(),
            &language,
            &mut all_references,
            &mut definitions,
        );

        // Dependencies = referenced but not defined locally
        let mut dependencies: Vec<String> = all_references
            .difference(&definitions)
            .filter(|name| !self.is_builtin_identifier(name, &language))
            .cloned()
            .collect();

        dependencies.sort(); // Consistent ordering
        dependencies.dedup(); // Remove duplicates

        debug!(
            "🎯 AST analysis found {} dependencies: {:?}",
            dependencies.len(),
            dependencies
        );

        Ok(dependencies)
    }

    /// Recursively collect all identifier references and definitions
    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    #[allow(dead_code)]
    fn collect_identifiers(
        &self,
        node: tree_sitter::Node,
        source: &[u8],
        language: &str,
        all_references: &mut std::collections::HashSet<String>,
        definitions: &mut std::collections::HashSet<String>,
    ) {
        // Language-specific AST node handling
        match language {
            "rust" => {
                // Variable references
                if node.kind() == "identifier" {
                    if let Ok(name) = node.utf8_text(source) {
                        // Check if this identifier is being defined or referenced
                        if let Some(parent) = node.parent() {
                            match parent.kind() {
                                "let_declaration" | "parameter" | "closure_parameters" => {
                                    definitions.insert(name.to_string());
                                }
                                _ => {
                                    all_references.insert(name.to_string());
                                }
                            }
                        } else {
                            all_references.insert(name.to_string());
                        }
                    }
                }
            }
            "typescript" | "javascript" => {
                if node.kind() == "identifier" {
                    if let Ok(name) = node.utf8_text(source) {
                        if let Some(parent) = node.parent() {
                            match parent.kind() {
                                "variable_declarator" | "formal_parameters" | "arrow_function" => {
                                    if parent.child_by_field_name("name").map(|n| n.id())
                                        == Some(node.id())
                                    {
                                        definitions.insert(name.to_string());
                                    } else {
                                        all_references.insert(name.to_string());
                                    }
                                }
                                _ => {
                                    all_references.insert(name.to_string());
                                }
                            }
                        } else {
                            all_references.insert(name.to_string());
                        }
                    }
                }
            }
            "python" => {
                if node.kind() == "identifier" {
                    if let Ok(name) = node.utf8_text(source) {
                        if let Some(parent) = node.parent() {
                            match parent.kind() {
                                "assignment" | "parameters" | "lambda_parameters" => {
                                    definitions.insert(name.to_string());
                                }
                                _ => {
                                    all_references.insert(name.to_string());
                                }
                            }
                        } else {
                            all_references.insert(name.to_string());
                        }
                    }
                }
            }
            // Add more languages as needed
            _ => {
                // Generic fallback: track all identifiers as references
                if node.kind() == "identifier" {
                    if let Ok(name) = node.utf8_text(source) {
                        all_references.insert(name.to_string());
                    }
                }
            }
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_identifiers(child, source, language, all_references, definitions);
        }
    }

    /// Check if an identifier is a built-in language construct
    #[allow(dead_code)]
    fn is_builtin_identifier(&self, name: &str, language: &str) -> bool {
        match language {
            "rust" => matches!(
                name,
                "true"
                    | "false"
                    | "None"
                    | "Some"
                    | "Ok"
                    | "Err"
                    | "println"
                    | "print"
                    | "eprintln"
                    | "eprint"
                    | "dbg"
                    | "panic"
                    | "Vec"
                    | "String"
                    | "Option"
                    | "Result"
                    | "Box"
                    | "Rc"
                    | "Arc"
            ),
            "typescript" | "javascript" => matches!(
                name,
                "console"
                    | "window"
                    | "document"
                    | "undefined"
                    | "null"
                    | "true"
                    | "false"
                    | "this"
                    | "super"
                    | "Promise"
                    | "Array"
                    | "Object"
                    | "String"
                    | "Number"
                    | "Boolean"
                    | "Math"
                    | "JSON"
                    | "Date"
            ),
            "python" => matches!(
                name,
                "True"
                    | "False"
                    | "None"
                    | "print"
                    | "len"
                    | "range"
                    | "str"
                    | "int"
                    | "float"
                    | "list"
                    | "dict"
                    | "tuple"
                    | "set"
                    | "bool"
            ),
            _ => false,
        }
    }

    /// Fallback dependency analysis when tree-sitter is not available
    #[allow(dead_code)]
    fn analyze_dependencies_fallback(&self, code: &str) -> Result<Vec<String>> {
        debug!("⚠️ Using fallback string-based dependency analysis");

        let mut dependencies = std::collections::HashSet::new();

        // Simple heuristic: find identifiers before dots (e.g., "user.name")
        for line in code.lines() {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with("#")
                || trimmed.is_empty()
            {
                continue;
            }

            // Look for patterns like: identifier.method()
            if let Some(pos) = trimmed.find('.') {
                let before_dot = &trimmed[..pos];
                if let Some(word) = before_dot.split_whitespace().last() {
                    if word.chars().all(|c| c.is_alphanumeric() || c == '_') && word.len() > 1 {
                        dependencies.insert(word.to_string());
                    }
                }
            }
        }

        Ok(dependencies.into_iter().collect())
    }

    /// Detect programming language from file extension using shared language module
    fn detect_language(&self, file_path: &str) -> String {
        std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(crate::language::detect_language_from_extension)
            .unwrap_or("unknown")
            .to_string()
    }

    /// Generate function definition and call based on language
    #[allow(dead_code)]
    fn generate_function_code(
        &self,
        language: &str,
        function_name: &str,
        code: &str,
        dependencies: &[String],
        return_type: Option<&str>,
        base_indent: usize,
    ) -> Result<(String, String)> {
        let indent_str = " ".repeat(base_indent);

        match language {
            "rust" => {
                let params = if dependencies.is_empty() {
                    String::new()
                } else {
                    dependencies
                        .iter()
                        .map(|dep| format!("{}: &str", dep)) // Simplified - would infer proper types
                        .collect::<Vec<_>>()
                        .join(", ")
                };

                let return_annotation =
                    return_type.map_or_else(|| "".to_string(), |rt| format!(" -> {}", rt));

                let function_def = format!(
                    "fn {}({}){} {{\n{}\n}}",
                    function_name,
                    params,
                    return_annotation,
                    code.lines()
                        .map(|line| format!("    {}", line))
                        .collect::<Vec<_>>()
                        .join("\n")
                );

                let call_args = dependencies.join(", ");
                let function_call = format!("{}{}({});", indent_str, function_name, call_args);

                Ok((function_def, function_call))
            }
            "typescript" | "javascript" => {
                let params = dependencies.join(", ");
                let return_annotation =
                    return_type.map_or_else(|| "".to_string(), |rt| format!(": {}", rt));

                let function_def = format!(
                    "function {}({}){} {{\n{}\n}}",
                    function_name,
                    params,
                    return_annotation,
                    code.lines()
                        .map(|line| format!("    {}", line))
                        .collect::<Vec<_>>()
                        .join("\n")
                );

                let function_call = format!("{}{}({});", indent_str, function_name, params);

                Ok((function_def, function_call))
            }
            "python" => {
                let params = dependencies.join(", ");
                let return_annotation =
                    return_type.map_or_else(|| "".to_string(), |rt| format!(" -> {}", rt));

                let function_def = format!(
                    "def {}({}){}:\n{}",
                    function_name,
                    params,
                    return_annotation,
                    code.lines()
                        .map(|line| format!("    {}", line))
                        .collect::<Vec<_>>()
                        .join("\n")
                );

                let function_call = format!("{}{}({})", indent_str, function_name, params);

                Ok((function_def, function_call))
            }
            _ => {
                // Generic approach for unknown languages
                let params = dependencies.join(", ");
                let function_def = format!(
                    "function {}({}) {{\n{}\n}}",
                    function_name,
                    params,
                    code.lines()
                        .map(|line| format!("    {}", line))
                        .collect::<Vec<_>>()
                        .join("\n")
                );

                let function_call = format!("{}{}({});", indent_str, function_name, params);

                Ok((function_def, function_call))
            }
        }
    }

    /// Apply the extract function refactoring to the file content
    #[allow(dead_code)]
    fn apply_extract_function(
        &self,
        file_content: &str,
        start_line: u32,
        end_line: u32,
        function_def: &str,
        function_call: &str,
        file_path: &str,
    ) -> Result<(u32, String)> {
        let lines: Vec<&str> = file_content.lines().collect();

        // Find a good place to insert the new function (before the current function)
        let insertion_line = self.find_function_insertion_point(&lines, start_line, file_path)?;

        let mut new_lines = Vec::new();

        // Add lines before insertion point
        for (i, line) in lines.iter().enumerate() {
            let line_num = i as u32 + 1;

            if line_num == insertion_line {
                // Insert the new function
                new_lines.push(function_def.to_string());
                new_lines.push("".to_string()); // Empty line after function
            }

            if line_num < start_line || line_num > end_line {
                // Keep original lines (outside extracted range)
                new_lines.push(line.to_string());
            } else if line_num == start_line {
                // Replace first line of extracted code with function call
                new_lines.push(function_call.to_string());
                // Skip the remaining extracted lines
            }
        }

        Ok((insertion_line, new_lines.join("\n")))
    }

    /// 🌳 AST-AWARE: Find appropriate location to insert the new function
    ///
    /// Uses tree-sitter to find:
    /// 1. The containing function (insert extracted function just before it)
    /// 2. Or the end of imports section (insert after imports at module level)
    ///
    /// This is smarter than string matching because it understands code structure.
    #[allow(dead_code)]
    fn find_function_insertion_point(
        &self,
        lines: &[&str],
        current_line: u32,
        file_path: &str,
    ) -> Result<u32> {
        use tree_sitter::Parser;

        debug!(
            "🌳 AST-based insertion point search for line {}",
            current_line
        );

        let content = lines.join("\n");
        let language = self.detect_language(file_path);

        // Try AST-based approach
        let mut parser = Parser::new();
        let tree_sitter_language = match self.get_tree_sitter_language(&language) {
            Ok(lang) => lang,
            Err(e) => {
                debug!(
                    "⚠️ Tree-sitter not available for {}: {}. Using fallback.",
                    language, e
                );
                return self.find_insertion_point_fallback(lines, current_line);
            }
        };

        if let Err(e) = parser.set_language(&tree_sitter_language) {
            debug!("⚠️ Failed to set language: {}. Using fallback.", e);
            return self.find_insertion_point_fallback(lines, current_line);
        }

        let tree = match parser.parse(&content, None) {
            Some(t) => t,
            None => {
                debug!("⚠️ Failed to parse code. Using fallback.");
                return self.find_insertion_point_fallback(lines, current_line);
            }
        };

        let root = tree.root_node();

        // Find the function containing current_line
        if let Some(containing_function) =
            self.find_containing_function(root, current_line, &language)
        {
            let function_start_line = containing_function.start_position().row + 1; // 1-based
            debug!(
                "🎯 Found containing function at line {}, inserting before it",
                function_start_line
            );
            return Ok(function_start_line as u32);
        }

        // If no containing function, find end of imports
        if let Some(after_imports_line) = self.find_end_of_imports(root, &language) {
            debug!(
                "🎯 No containing function, inserting after imports at line {}",
                after_imports_line
            );
            return Ok(after_imports_line);
        }

        // Ultimate fallback: insert at beginning
        debug!("⚠️ No imports found, inserting at beginning");
        Ok(1)
    }

    /// Find the function node containing the specified line
    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    #[allow(dead_code)]
    fn find_containing_function<'a>(
        &self,
        node: tree_sitter::Node<'a>,
        target_line: u32,
        language: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        // Language-specific function node types
        // Use shared language configuration for AST node types
        let function_kinds = crate::language::get_function_node_kinds(language);

        // Check if this node is a function containing the target line
        if function_kinds.contains(&node.kind()) {
            let start_line = (node.start_position().row + 1) as u32; // 1-based
            let end_line = (node.end_position().row + 1) as u32; // 1-based

            if start_line <= target_line && target_line <= end_line {
                return Some(node);
            }
        }

        // Recursively search children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = self.find_containing_function(child, target_line, language) {
                return Some(found);
            }
        }

        None
    }

    /// Find the line number after imports/use statements
    #[allow(dead_code)]
    fn find_end_of_imports(&self, root: tree_sitter::Node, language: &str) -> Option<u32> {
        // Use shared language configuration for AST node types
        let import_kinds = crate::language::get_import_node_kinds(language);

        let mut last_import_line = 0u32;

        // Walk the tree to find all imports
        let mut cursor = root.walk();
        self.find_last_import_line(root, &import_kinds, &mut last_import_line, &mut cursor);

        if last_import_line > 0 {
            Some(last_import_line + 2) // Insert 1 blank line after imports
        } else {
            None
        }
    }

    /// Helper to recursively find the last import line
    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    #[allow(dead_code)]
    fn find_last_import_line<'a>(
        &self,
        node: tree_sitter::Node<'a>,
        import_kinds: &[&str],
        last_line: &mut u32,
        cursor: &mut tree_sitter::TreeCursor<'a>,
    ) {
        if import_kinds.contains(&node.kind()) {
            let node_end_line = (node.end_position().row + 1) as u32; // 1-based
            *last_line = (*last_line).max(node_end_line);
        }

        for child in node.children(cursor) {
            let mut child_cursor = child.walk();
            self.find_last_import_line(child, import_kinds, last_line, &mut child_cursor);
        }
    }

    /// Fallback insertion point finder when tree-sitter is not available
    #[allow(dead_code)]
    fn find_insertion_point_fallback(&self, lines: &[&str], current_line: u32) -> Result<u32> {
        debug!("⚠️ Using fallback string-based insertion point detection");

        // Look backwards from current line to find function start
        for i in (0..(current_line as usize - 1)).rev() {
            let line = lines[i].trim();
            if line.starts_with("fn ") || line.starts_with("function ") || line.starts_with("def ")
            {
                return Ok(i as u32 + 1);
            }
        }

        // If no function found, insert at the beginning after imports
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if !trimmed.is_empty()
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("import ")
                && !trimmed.starts_with("use ")
                && !trimmed.starts_with("from ")
                && !trimmed.starts_with("#")
            {
                return Ok(i as u32 + 1);
            }
        }

        Ok(1) // Fallback to beginning
    }

    /// Handle replace symbol body operation (Serena-inspired)
    async fn handle_replace_symbol_body(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        debug!("🔄 Processing replace symbol body operation");

        // Parse JSON parameters
        let params: JsonValue = serde_json::from_str(&self.params)
            .map_err(|e| anyhow::anyhow!("Invalid JSON in params: {}", e))?;

        let file_path = params
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file"))?
            .to_string();

        let symbol_name = params
            .get("symbol_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: symbol_name"))?
            .to_string();

        let new_body = params
            .get("new_body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: new_body"))?
            .to_string();

        debug!(
            "🎯 Replace symbol '{}' in file '{}'",
            symbol_name, file_path
        );

        // Canonicalize the target file path to handle macOS /var -> /private/var symlinks
        let canonical_file_path = std::fs::canonicalize(&file_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file_path.clone());

        let symbols_in_file = if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = workspace.db.as_ref() {
                // Query for symbols by name and filter by file path
                let symbol_name_clone = symbol_name.to_string();
                let file_path_clone = file_path.clone();
                let canonical_file_path_clone = canonical_file_path.clone();
                let db_arc = db.clone();
                tokio::task::spawn_blocking(move || {
                    let db_lock = db_arc.lock().unwrap();
                    match db_lock.get_symbols_by_name(&symbol_name_clone) {
                        Ok(all_symbols) => {
                            debug!("Found {} symbols with name '{}'", all_symbols.len(), symbol_name_clone);
                            // Filter symbols to only those in the target file (compare canonical paths)
                            let filtered: Vec<_> = all_symbols.into_iter()
                                .filter(|symbol| {
                                    // Canonicalize the symbol's file path too for comparison
                                    let symbol_canonical = std::fs::canonicalize(&symbol.file_path)
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_else(|_| symbol.file_path.clone());
                                    let matches = symbol_canonical == canonical_file_path_clone;
                                    debug!("Symbol '{}' in file '{}' (canonical: '{}') matches target '{}' (canonical: '{}'): {}",
                                          symbol.name, symbol.file_path, symbol_canonical, file_path_clone, canonical_file_path_clone, matches);
                                    matches
                                })
                                .collect();
                            debug!("After filtering by file path, {} symbols remain", filtered.len());
                            filtered
                        }
                        Err(e) => {
                            eprintln!("Database error: {}", e);
                            Vec::new()
                        }
                    }
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        if symbols_in_file.is_empty() {
            let message = format!(
                "🔍 Symbol '{}' not found in file '{}'\n\
                💡 Check spelling or use fast_refs to locate the symbol",
                symbol_name, file_path
            );
            return self.create_result(
                "replace_symbol_body",
                false,
                vec![],
                0,
                vec![
                    "Use fast_refs to locate the symbol".to_string(),
                    "Check spelling of symbol name".to_string(),
                ],
                message,
                None,
            );
        }

        // Step 2: Use symbol information from database to get boundaries
        // For replace_symbol_body, we expect exactly one symbol match
        if symbols_in_file.len() != 1 {
            let message = format!(
                "🔍 Expected exactly 1 symbol '{}' in file '{}', but found {}\n\
                💡 Use fast_refs to see all matches",
                symbol_name, file_path, symbols_in_file.len()
            );
            return self.create_result(
                "replace_symbol_body",
                false,
                vec![],
                0,
                vec![
                    "Use fast_refs to see all symbol matches".to_string(),
                    "Specify a more unique symbol name".to_string(),
                ],
                message,
                None,
            );
        }

        let symbol = &symbols_in_file[0];
        let start_line = symbol.start_line;
        let end_line = symbol.end_line;

        // Step 3: Read the file content
        let file_content = fs::read_to_string(&file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", file_path, e))?;

        debug!(
            "📍 Found symbol '{}' at lines {}-{}",
            symbol_name, start_line, end_line
        );

        // Step 4: Apply the replacement
        if self.dry_run {
            let preview = format!(
                "🔍 DRY RUN: Replace symbol '{}' body\n\
                📁 File: {}\n\
                📍 Lines: {}-{}\n\n\
                🔄 New body:\n{}\n\n\
                💡 Set dry_run=false to apply changes",
                symbol_name, file_path, start_line, end_line, new_body
            );

            return self.create_result(
                "replace_symbol_body",
                true,
                vec![file_path.to_string()],
                1, // One symbol replacement
                vec!["Set dry_run=false to apply changes".to_string()],
                preview,
                None,
            );
        }

        // Replace the symbol body
        let new_content =
            self.replace_symbol_in_file(&file_content, start_line, end_line, &new_body)?;

        // Write the modified file atomically using EditingTransaction
        let transaction = EditingTransaction::begin(&file_path)?;
        transaction.commit(&new_content)?;

        let message = format!(
            "✅ Replace symbol body successful: '{}'\n\
            📁 File: {}\n\
            📍 Lines {}-{} replaced\n\n\
            🎯 Next steps:\n\
            • Run tests to verify changes\n\
            • Use fast_goto to navigate to the updated symbol\n\
            💡 Tip: Use git to track changes and revert if needed",
            symbol_name, file_path, start_line, end_line
        );

        self.create_result(
            "replace_symbol_body",
            true,
            vec![file_path.to_string()],
            1, // One symbol replacement
            vec![
                "Run tests to verify changes".to_string(),
                format!("Use fast_goto to navigate to {}", symbol_name),
                "Use git diff to review changes".to_string(),
            ],
            message,
            None,
        )
    }

    /// Parse search results to find symbol locations in a specific file
    fn parse_search_result_for_symbols(
        &self,
        search_result: &CallToolResult,
        symbol_name: &str,
        target_file: &str,
    ) -> Result<Vec<(String, u32)>> {
        let mut locations = Vec::new();

        // Extract text content from the result
        let content = search_result
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

        // Parse Julie's search results format
        let lines: Vec<&str> = content.lines().collect();

        for i in 0..lines.len() {
            let line = lines[i];

            // Look for file path lines (contain 📁 emoji)
            if line.contains("📁") {
                if let Some(file_line) = self.extract_file_location(line) {
                    let (extracted_file_path, line_num) = &file_line;

                    // Check if this file path matches our target file (handle both absolute and relative paths)
                    let path_matches = extracted_file_path == target_file
                        || extracted_file_path.ends_with(target_file)
                        || target_file.ends_with(extracted_file_path);

                    debug!(
                        "🔍 Comparing file paths: extracted='{}', target='{}', matches={}",
                        extracted_file_path, target_file, path_matches
                    );

                    if path_matches {
                        // Check if the next line contains our symbol (this is search result format, not source code)
                        if i + 1 < lines.len() {
                            let next_line = lines[i + 1];
                            if next_line.contains(symbol_name) {
                                debug!(
                                    "✅ Found matching symbol '{}' in target file at line {}",
                                    symbol_name, line_num
                                );
                                locations.push(file_line);
                            } else {
                                debug!(
                                    "🔍 Next line doesn't contain symbol '{}': '{}'",
                                    symbol_name, next_line
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(locations)
    }

    /// Extract file path and line number from search result line
    fn extract_file_location(&self, line: &str) -> Option<(String, u32)> {
        // Julie's search format: "📁 /path/to/file.ts:30-40"
        // Remove the 📁 emoji and whitespace first

        // More robust emoji removal - find the first non-emoji, non-whitespace character
        let cleaned_line = line.trim();
        let cleaned_line = cleaned_line
            .strip_prefix("📁") // 📁 emoji is 4 bytes in UTF-8
            .unwrap_or(cleaned_line);
        let cleaned_line = cleaned_line.trim();


        if let Some(colon_pos) = cleaned_line.rfind(':') {
            let file_part = &cleaned_line[..colon_pos];
            let line_range_part = &cleaned_line[colon_pos + 1..];

            // Parse line range (e.g., "30-40" or just "30")
            let start_line = if let Some(dash_pos) = line_range_part.find('-') {
                // Format: "30-40" - take the start line
                line_range_part[..dash_pos].parse::<u32>().ok()?
            } else {
                // Format: "30" - single line number
                line_range_part.parse::<u32>().ok()?
            };

            println!(
                "🔍 extract_file_location result: file='{}' line={}",
                file_part, start_line
            );
            return Some((file_part.to_string(), start_line));
        }
        None
    }

    /// 🌳 AST-AWARE: Find symbol boundaries using tree-sitter
    ///
    /// Uses tree-sitter AST to find the exact start and end lines of a symbol definition.
    /// This is far more accurate than string matching and brace counting.
    ///
    /// Returns: (start_line, end_line) in 1-based indexing
    fn find_symbol_boundaries(
        &self,
        file_content: &str,
        symbol_name: &str,
        file_path: &str,
    ) -> Result<(u32, u32)> {
        use tree_sitter::Parser;

        debug!(
            "🌳 AST-based symbol boundary detection for '{}'",
            symbol_name
        );

        let language = self.detect_language(file_path);

        // Try AST-based approach
        let mut parser = Parser::new();
        let tree_sitter_language = match self.get_tree_sitter_language(&language) {
            Ok(lang) => lang,
            Err(e) => {
                debug!(
                    "⚠️ Tree-sitter not available for {}: {}. Using fallback.",
                    language, e
                );
                return self.find_symbol_boundaries_fallback(file_content, symbol_name, file_path);
            }
        };

        if let Err(e) = parser.set_language(&tree_sitter_language) {
            debug!("⚠️ Failed to set language: {}. Using fallback.", e);
            return self.find_symbol_boundaries_fallback(file_content, symbol_name, file_path);
        }

        let tree = match parser.parse(file_content, None) {
            Some(t) => t,
            None => {
                debug!("⚠️ Failed to parse code. Using fallback.");
                return self.find_symbol_boundaries_fallback(file_content, symbol_name, file_path);
            }
        };

        let root = tree.root_node();

        // Find the symbol node in the AST
        if let Some(symbol_node) =
            self.find_symbol_node(root, symbol_name, file_content.as_bytes(), &language)
        {
            let start_line = (symbol_node.start_position().row + 1) as u32; // 1-based
            let end_line = (symbol_node.end_position().row + 1) as u32; // 1-based

            debug!(
                "🎯 AST found symbol '{}' at lines {}-{}",
                symbol_name, start_line, end_line
            );

            return Ok((start_line, end_line));
        }

        // If AST search failed, try fallback
        debug!(
            "⚠️ Symbol '{}' not found in AST, trying fallback",
            symbol_name
        );
        self.find_symbol_boundaries_fallback(file_content, symbol_name, file_path)
    }

    /// Recursively find a symbol node by name in the AST
    fn find_symbol_node<'a>(
        &self,
        node: tree_sitter::Node<'a>,
        symbol_name: &str,
        source: &[u8],
        language: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        // Language-specific symbol definition node types
        // Use shared language configuration for AST node types
        let symbol_kinds = crate::language::get_symbol_node_kinds(language);

        // Check if this node is a symbol definition
        if symbol_kinds.contains(&node.kind()) {
            // Try to get the name of this symbol
            if let Some(name_node) = self.get_symbol_name_node(node, language) {
                if let Ok(name) = name_node.utf8_text(source) {
                    if name == symbol_name {
                        debug!("✨ Found symbol '{}' as {} node", symbol_name, node.kind());
                        return Some(node);
                    }
                }
            }
        }

        // Recursively search children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = self.find_symbol_node(child, symbol_name, source, language) {
                return Some(found);
            }
        }

        None
    }

    /// Get the name node of a symbol definition (language-specific)
    fn get_symbol_name_node<'a>(
        &self,
        node: tree_sitter::Node<'a>,
        language: &str,
    ) -> Option<tree_sitter::Node<'a>> {
        // Use shared language configuration for field name
        let field_name = crate::language::get_symbol_name_field(language);

        // C/C++ use nested declarator nodes, handle specially
        if matches!(language, "cpp" | "c") {
            node.child_by_field_name("declarator")
                .and_then(|d| d.child_by_field_name("declarator"))
        } else {
            // Most languages use simple "name" field
            node.child_by_field_name(field_name)
                .or_else(|| node.child_by_field_name("identifier")) // Fallback
        }
    }

    /// Fallback boundary detection using primitive string matching
    fn find_symbol_boundaries_fallback(
        &self,
        file_content: &str,
        symbol_name: &str,
        file_path: &str,
    ) -> Result<(u32, u32)> {
        debug!("⚠️ Using fallback string-based boundary detection");

        let lines: Vec<&str> = file_content.lines().collect();
        let language = self.detect_language(file_path);

        // Find the symbol definition line
        let mut start_line = None;
        for (i, line) in lines.iter().enumerate() {
            if line.contains(symbol_name) {
                let trimmed = line.trim();
                // Look for function/class/struct definitions
                if self.is_symbol_definition(trimmed, symbol_name, &language) {
                    start_line = Some(i as u32 + 1); // Convert to 1-based indexing
                    break;
                }
            }
        }

        let start_line = start_line.ok_or_else(|| {
            anyhow::anyhow!("Could not find symbol '{}' definition in file", symbol_name)
        })?;

        // Find the end of the symbol (simple brace matching)
        let end_line = self.find_symbol_end(&lines, start_line as usize - 1, &language)?;

        debug!(
            "🎯 Fallback found symbol '{}' at lines {}-{}",
            symbol_name, start_line, end_line
        );

        Ok((start_line, end_line))
    }

    /// Check if a line contains a symbol definition
    fn is_symbol_definition(&self, line: &str, symbol_name: &str, language: &str) -> bool {
        match language {
            "rust" => {
                (line.starts_with("fn ")
                    || line.starts_with("pub fn ")
                    || line.starts_with("struct ")
                    || line.starts_with("pub struct ")
                    || line.starts_with("impl ")
                    || line.starts_with("enum ")
                    || line.starts_with("pub enum "))
                    && line.contains(symbol_name)
            }
            "typescript" | "javascript" => {
                line.contains(symbol_name)
                    && (line.starts_with("function ") || line.starts_with("export function ") ||
                    line.starts_with("class ") || line.starts_with("export class ") ||
                    line.starts_with("async ") || // async functions/methods
                    line.contains("function ") ||
                    // Method definitions: "methodName(" or "async methodName("
                    (line.contains(&format!("{}(", symbol_name)) &&
                     (line.trim_start().starts_with(symbol_name) ||
                      line.trim_start().starts_with(&format!("async {}", symbol_name)) ||
                      line.trim_start().starts_with(&format!("public {}", symbol_name)) ||
                      line.trim_start().starts_with(&format!("private {}", symbol_name)) ||
                      line.trim_start().starts_with(&format!("protected {}", symbol_name)))))
            }
            "python" => {
                (line.starts_with("def ") || line.starts_with("class "))
                    && line.contains(symbol_name)
            }
            _ => {
                // Generic approach
                line.contains(symbol_name)
                    && (line.contains("function")
                        || line.contains("class")
                        || line.contains("def ")
                        || line.contains("fn "))
            }
        }
    }

    /// Find the end line of a symbol definition (simple brace counting)
    fn find_symbol_end(&self, lines: &[&str], start_idx: usize, language: &str) -> Result<u32> {
        if start_idx >= lines.len() {
            return Err(anyhow::anyhow!("Invalid start line index"));
        }

        match language {
            "python" => {
                // Python uses indentation
                let start_line = lines[start_idx];
                let base_indent = start_line.len() - start_line.trim_start().len();

                #[allow(clippy::needless_range_loop)] // Index needed for return statement
                for i in (start_idx + 1)..lines.len() {
                    let line = lines[i];
                    if !line.trim().is_empty() {
                        let line_indent = line.len() - line.trim_start().len();
                        if line_indent <= base_indent {
                            return Ok(i as u32); // Convert to 1-based
                        }
                    }
                }
                Ok(lines.len() as u32)
            }
            _ => {
                // Brace-based languages
                let mut brace_count = 0;
                let mut found_opening_brace = false;

                #[allow(clippy::needless_range_loop)] // Index needed for return statement
                for i in start_idx..lines.len() {
                    let line = lines[i];
                    for ch in line.chars() {
                        match ch {
                            '{' => {
                                brace_count += 1;
                                found_opening_brace = true;
                            }
                            '}' => {
                                brace_count -= 1;
                                if found_opening_brace && brace_count == 0 {
                                    return Ok(i as u32 + 1); // Convert to 1-based
                                }
                            }
                            _ => {}
                        }
                    }
                }

                Err(anyhow::anyhow!(
                    "Could not find end of symbol - unmatched braces"
                ))
            }
        }
    }

    /// Replace symbol content in file
    fn replace_symbol_in_file(
        &self,
        file_content: &str,
        start_line: u32,
        end_line: u32,
        new_body: &str,
    ) -> Result<String> {
        let lines: Vec<&str> = file_content.lines().collect();
        let mut new_lines = Vec::new();

        // Single loop to process all lines correctly
        for (i, line) in lines.iter().enumerate() {
            let line_num = i as u32 + 1;

            if line_num < start_line {
                // Keep lines before the symbol
                new_lines.push(line.to_string());
            } else if line_num == start_line {
                // Replace the symbol with new body
                new_lines.push(new_body.to_string());
                // Skip lines between start_line+1 and end_line (they are part of the old symbol)
            } else if line_num > end_line {
                // Keep lines after the symbol
                new_lines.push(line.to_string());
            }
            // Lines between start_line+1 and end_line are implicitly skipped
        }

        Ok(new_lines.join("\n"))
    }

    /// Placeholder implementations for other operations
    async fn handle_insert_relative_to_symbol(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        let message = "🚧 InsertRelativeToSymbol operation is not yet implemented\n\
                      📋 Coming soon - will insert code before/after symbols\n\
                      💡 Use ReplaceSymbolBody operation for now";

        self.create_result(
            "insert_relative_to_symbol",
            false,
            vec![],
            0,
            vec!["Use replace_symbol_body for manual insertion".to_string()],
            message.to_string(),
            None,
        )
    }

    async fn handle_extract_type(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 ExtractType operation is not yet implemented\n\
                      📋 Coming soon - will extract inline types to named types\n\
                      💡 Use ReplaceSymbolBody operation for now";

        self.create_result(
            "extract_type",
            false,
            vec![],
            0,
            vec!["Use replace_symbol_body for manual type extraction".to_string()],
            message.to_string(),
            None,
        )
    }

    async fn handle_update_imports(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 UpdateImports operation is not yet implemented\n\
                      📋 Coming soon - will fix broken imports after file moves\n\
                      💡 Use ReplaceSymbolBody operation for now";

        self.create_result(
            "update_imports",
            false,
            vec![],
            0,
            vec!["Manually update imports for now".to_string()],
            message.to_string(),
            None,
        )
    }

    async fn handle_inline_variable(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        let message = "🚧 InlineVariable operation is not yet implemented\n\
                      📋 Coming soon - will inline variable by replacing uses with value\n\
                      💡 Use ReplaceSymbolBody operation for now";

        self.create_result(
            "inline_variable",
            false,
            vec![],
            0,
            vec!["Manually inline variable for now".to_string()],
            message.to_string(),
            None,
        )
    }

    async fn handle_inline_function(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        let message = "🚧 InlineFunction operation is not yet implemented\n\
                      📋 Coming soon - will inline function by replacing calls with body\n\
                      💡 Use ReplaceSymbolBody operation for now";

        self.create_result(
            "inline_function",
            false,
            vec![],
            0,
            vec!["Manually inline function for now".to_string()],
            message.to_string(),
            None,
        )
    }

    /// Handle validate_syntax operation - Week 3: AST Syntax Fix
    /// Uses tree-sitter error nodes to detect syntax errors across all 26 languages
    async fn handle_validate_syntax(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Parse params to get file_path
        let params: serde_json::Value = serde_json::from_str(&self.params)?;
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

        // Get workspace and normalize path
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

        let absolute_path = if std::path::Path::new(file_path).is_absolute() {
            file_path.to_string()
        } else {
            workspace.root.join(file_path).to_string_lossy().to_string()
        };

        // Read file content
        let content = tokio::fs::read_to_string(&absolute_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", absolute_path, e))?;

        // Detect language
        let language = self.detect_language(file_path);

        // Get tree-sitter parser
        let tree_sitter_lang = self.get_tree_sitter_language(&language)?;
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_lang)?;

        // Parse the code
        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file"))?;

        // Collect syntax errors from tree-sitter error nodes
        let mut errors = Vec::new();
        let mut cursor = tree.walk();

        fn collect_errors(
            cursor: &mut tree_sitter::TreeCursor,
            content: &str,
            errors: &mut Vec<SyntaxError>,
        ) {
            loop {
                let node = cursor.node();

                // Check if this node is an error or missing node
                if node.is_error() || node.is_missing() {
                    let start_pos = node.start_position();
                    let error_type = if node.is_missing() {
                        "missing"
                    } else {
                        "error"
                    };

                    // Extract context (3 lines before and after)
                    let lines: Vec<&str> = content.lines().collect();
                    let error_line = start_pos.row;
                    let context_start = error_line.saturating_sub(1);
                    let context_end = (error_line + 2).min(lines.len());

                    let context_lines: Vec<String> = lines[context_start..context_end]
                        .iter()
                        .enumerate()
                        .map(|(i, line)| {
                            let line_num = context_start + i + 1;
                            if line_num == error_line + 1 {
                                format!("  ➤ {}: {}", line_num, line)
                            } else {
                                format!("    {}: {}", line_num, line)
                            }
                        })
                        .collect();

                    errors.push(SyntaxError {
                        line: start_pos.row as u32 + 1, // 1-based
                        column: start_pos.column as u32,
                        message: format!("Syntax {}: {}", error_type, node.kind()),
                        severity: "error".to_string(),
                        suggested_fix: None, // TODO: Add heuristics for common fixes
                        context: Some(context_lines.join("\n")),
                    });
                }

                // Recurse into children
                if cursor.goto_first_child() {
                    collect_errors(cursor, content, errors);
                    cursor.goto_parent();
                }

                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        collect_errors(&mut cursor, &content, &mut errors);

        // Build response
        let errors_count = errors.len();
        let metadata = serde_json::json!({
            "errors": errors,
            "file_path": file_path,
            "language": language,
        });

        let message = if errors_count == 0 {
            format!(
                "✅ **Syntax validation passed**\n\n📄 File: `{}`\n🎉 No syntax errors found!",
                file_path
            )
        } else {
            let mut msg = format!("❌ **Syntax validation failed**\n\n📄 File: `{}`\n🐛 Found {} syntax error(s):\n\n", file_path, errors_count);
            for (i, error) in errors.iter().enumerate() {
                msg.push_str(&format!(
                    "{}. Line {}:{} - {}\n",
                    i + 1,
                    error.line,
                    error.column,
                    error.message
                ));
                if let Some(ref context) = error.context {
                    msg.push_str(&format!("```\n{}\n```\n\n", context));
                }
            }
            msg
        };

        let next_actions = if errors_count > 0 {
            vec![format!(
                "Run smart_refactor operation=auto_fix_syntax to fix {} error(s) automatically",
                errors_count
            )]
        } else {
            vec!["Code is valid - ready for deployment".to_string()]
        };

        self.create_result(
            "validate_syntax",
            true,
            vec![],
            errors_count,
            next_actions,
            message,
            Some(metadata),
        )
    }

    /// Handle auto_fix_syntax operation - Week 3: AST Syntax Fix
    /// Automatically fixes common syntax errors (semicolons, braces, etc.)
    async fn handle_auto_fix_syntax(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Parse params to get file_path
        let params: serde_json::Value = serde_json::from_str(&self.params)?;
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

        // Normalize path (handle both absolute and relative)
        let absolute_path = if std::path::Path::new(file_path).is_absolute() {
            file_path.to_string()
        } else {
            // Try to get workspace for relative paths
            if let Ok(Some(workspace)) = handler.get_workspace().await {
                workspace.root.join(file_path).to_string_lossy().to_string()
            } else {
                // No workspace but path is relative - use as-is and hope it works
                file_path.to_string()
            }
        };

        // Read file content
        let original_content = tokio::fs::read_to_string(&absolute_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", absolute_path, e))?;

        // Detect language
        let language = self.detect_language(file_path);

        // Get tree-sitter parser
        let tree_sitter_lang = self.get_tree_sitter_language(&language)?;
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_lang)?;

        // Apply fixes iteratively until no more errors (max 10 iterations to prevent infinite loops)
        let mut current_content = original_content.clone();
        let mut total_fixes = 0;
        const MAX_ITERATIONS: usize = 10;

        for iteration in 0..MAX_ITERATIONS {
            // Parse to find errors
            let tree = parser.parse(&current_content, None).ok_or_else(|| {
                anyhow::anyhow!("Failed to parse file at iteration {}", iteration)
            })?;

            // Apply fixes for this iteration
            let fixed_content = self.apply_syntax_fixes(&current_content, &tree, &language)?;

            if fixed_content == current_content {
                // No more fixes applied, we're done
                debug!("✅ No more fixes after {} iterations", iteration);
                break;
            }

            // Count this fix
            total_fixes += 1;
            debug!("🔧 Applied fix #{} at iteration {}", total_fixes, iteration);
            current_content = fixed_content;
        }

        let fixed_content = current_content;
        let fixes_count = total_fixes;

        // Write fixed content if not dry_run
        if !self.dry_run && fixes_count > 0 {
            tokio::fs::write(&absolute_path, &fixed_content)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to write fixed file: {}", e))?;
        }

        // Build response
        let message = if fixes_count == 0 {
            format!(
                "✅ **No syntax errors to fix**\n\n📄 File: `{}`\n🎉 Code is already valid!",
                file_path
            )
        } else if self.dry_run {
            format!("🔍 **Dry run - {} fix(es) would be applied**\n\n📄 File: `{}`\n💡 Run without dry_run to apply fixes", fixes_count, file_path)
        } else {
            format!("✅ **Auto-fixed {} syntax error(s)**\n\n📄 File: `{}`\n🔧 Fixes applied successfully!", fixes_count, file_path)
        };

        let next_actions = if fixes_count > 0 && !self.dry_run {
            vec!["Run validate_syntax to confirm all errors are fixed".to_string()]
        } else if fixes_count > 0 && self.dry_run {
            vec!["Remove dry_run flag to apply fixes".to_string()]
        } else {
            vec!["No further action needed".to_string()]
        };

        let files_modified = if !self.dry_run && fixes_count > 0 {
            vec![file_path.to_string()]
        } else {
            vec![]
        };

        self.create_result(
            "auto_fix_syntax",
            true,
            files_modified,
            fixes_count,
            next_actions,
            message,
            None,
        )
    }

    /// Check if content parses without errors using tree-sitter
    fn parses_without_errors(&self, content: &str, lang: &tree_sitter::Language) -> bool {
        let mut parser = Parser::new();
        if parser.set_language(lang).is_err() {
            return false;
        }

        if let Some(tree) = parser.parse(content, None) {
            !self.tree_has_errors(&tree.root_node())
        } else {
            false
        }
    }

    /// Recursively check if a node or any of its children have errors
    fn tree_has_errors(&self, node: &tree_sitter::Node) -> bool {
        if node.is_error() || node.is_missing() {
            return true;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if self.tree_has_errors(&child) {
                return true;
            }
        }

        false
    }

    /// Apply syntax fixes to code based on tree-sitter errors
    /// Focus: REAL parse-breaking errors (unmatched braces, unclosed strings, missing delimiters)
    /// Uses VALIDATION-DRIVEN approach: tries multiple positions and validates with tree-sitter
    fn apply_syntax_fixes(
        &self,
        content: &str,
        tree: &tree_sitter::Tree,
        language: &str,
    ) -> Result<String> {
        // Find the FIRST delimiter error (fix one at a time to avoid cascades)
        let delimiter_errors = self.find_delimiter_errors(tree, content)?;

        if delimiter_errors.is_empty() {
            return Ok(content.to_string());
        }

        // Get tree-sitter language for validation
        let tree_sitter_lang = self.get_tree_sitter_language(language)?;

        debug!(
            "🔍 Found {} delimiter errors, attempting to fix the first one",
            delimiter_errors.len()
        );

        // Fix the FIRST error using validation-driven approach
        if let Some(error) = delimiter_errors.first() {
            let delimiter = &error.missing_delimiter;
            debug!("🎯 Trying to fix missing: {}", delimiter);

            // Try multiple insertion strategies, validate each one
            if let Some(fixed) =
                self.try_inline_insertion_strategies(content, delimiter, &tree_sitter_lang)
            {
                debug!("✅ INLINE insertion validated successfully!");
                return Ok(fixed);
            }

            // Try end-of-line insertion (for unclosed strings)
            if delimiter == "\"" || delimiter == "'" {
                if let Some(fixed) =
                    self.try_end_of_line_insertion(content, delimiter, &tree_sitter_lang)
                {
                    debug!("✅ END-OF-LINE insertion validated successfully!");
                    return Ok(fixed);
                }
            }

            if let Some(fixed) =
                self.try_newline_insertion_strategies(content, delimiter, &tree_sitter_lang)
            {
                debug!("✅ NEWLINE insertion validated successfully!");
                return Ok(fixed);
            }

            // If no strategy worked, fall back to original content
            debug!("⚠️ No valid insertion position found - returning original content");
        }

        Ok(content.to_string())
    }

    /// Try INLINE insertion strategies (e.g., `)` before `{` on same line)
    /// Returns the first valid fix or None
    fn try_inline_insertion_strategies(
        &self,
        content: &str,
        delimiter: &str,
        lang: &tree_sitter::Language,
    ) -> Option<String> {
        let lines: Vec<String> = content.lines().map(String::from).collect();

        // Strategy 1: Insert before `{` on the same line (for cases like `taxRate: number {` → `taxRate: number) {`)
        for (line_idx, line) in lines.iter().enumerate() {
            // Look for opening braces, brackets, semicolons that might need the delimiter before them
            for pattern in &[" {", " [", " }", ";"] {
                if let Some(pos) = line.find(pattern) {
                    // Create modified version of this line
                    let mut modified_line = line.clone();
                    modified_line.insert_str(pos, delimiter);

                    // Build test content with the modified line
                    let mut test_lines = lines.clone();
                    test_lines[line_idx] = modified_line.clone();
                    let test_content = test_lines.join("\n");

                    debug!(
                        "  🔍 Trying INLINE before '{}' at line {}: {:?}",
                        pattern.trim(),
                        line_idx + 1,
                        modified_line.trim()
                    );

                    if self.parses_without_errors(&test_content, lang) {
                        debug!("  ✅ Valid! INLINE before '{}'", pattern.trim());
                        return Some(test_content);
                    }
                }
            }
        }

        None
    }

    /// Try END-OF-LINE insertion strategy (for unclosed strings)
    /// Appends the missing quote at the end of lines, going backwards from end
    fn try_end_of_line_insertion(
        &self,
        content: &str,
        delimiter: &str,
        lang: &tree_sitter::Language,
    ) -> Option<String> {
        let lines: Vec<String> = content.lines().map(String::from).collect();

        // Try appending delimiter at end of each line (from end backwards)
        for (line_idx, line) in lines.iter().enumerate().rev() {
            // Skip empty lines and comments
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with("#")
            {
                continue;
            }

            // Build test content with delimiter appended at end of line
            let mut test_lines = lines.clone();
            test_lines[line_idx] = format!("{}{}", line, delimiter);
            let test_content = test_lines.join("\n");

            debug!(
                "  🔍 Trying END-OF-LINE at line {}: appending '{}'",
                line_idx + 1,
                delimiter
            );

            if self.parses_without_errors(&test_content, lang) {
                debug!("  ✅ Valid! END-OF-LINE at line {}", line_idx + 1);
                return Some(test_content);
            }
        }

        None
    }

    /// Try NEWLINE insertion strategies (delimiter on new line with reduced indentation)
    /// Returns the first valid fix or None
    fn try_newline_insertion_strategies(
        &self,
        content: &str,
        delimiter: &str,
        lang: &tree_sitter::Language,
    ) -> Option<String> {
        let lines: Vec<String> = content.lines().map(String::from).collect();

        // Strategy 1: Try inserting on new line after each line (from end backwards)
        for line_idx in (0..lines.len()).rev() {
            let line = &lines[line_idx];

            // Skip empty lines and comments
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with("#")
            {
                continue;
            }

            // Calculate indentation (reduce by 4 spaces for closing delimiters)
            let current_indent = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();
            let closing_indent = if current_indent.len() >= 4 {
                &current_indent[..current_indent.len() - 4]
            } else {
                ""
            };

            // Build test content with delimiter on new line
            let mut test_lines = lines.clone();
            let delimiter_line = format!("{}{}", closing_indent, delimiter);
            test_lines.insert(line_idx + 1, delimiter_line.clone());
            let test_content = test_lines.join("\n");

            debug!(
                "  🔍 Trying NEWLINE after line {}: inserting '{}'",
                line_idx + 1,
                delimiter_line.trim()
            );

            if self.parses_without_errors(&test_content, lang) {
                debug!("  ✅ Valid! NEWLINE after line {}", line_idx + 1);
                return Some(test_content);
            }
        }

        None
    }

    /// Find delimiter errors (unmatched braces, brackets, parentheses, unclosed strings)
    /// Returns only the FIRST error overall to avoid cascade effects
    ///
    /// Strategy: Fix one error at a time. After fixing, code should be re-parsed
    /// to find the next error. This prevents cascade effects where one missing
    /// delimiter causes tree-sitter to report spurious additional errors.
    fn find_delimiter_errors(
        &self,
        tree: &tree_sitter::Tree,
        content: &str,
    ) -> Result<Vec<DelimiterError>> {
        let mut all_errors = Vec::new();
        let root = tree.root_node();

        // Walk the tree to find ERROR and MISSING nodes
        let mut cursor = root.walk();
        self.walk_for_delimiter_errors(&mut cursor, content, &mut all_errors);

        // Return only the FIRST error to avoid cascades
        // After fixing this one error, code should be re-parsed for next error
        if let Some(first_error) = all_errors.into_iter().next() {
            Ok(vec![first_error])
        } else {
            Ok(vec![])
        }
    }

    /// Recursively walk the tree to find delimiter errors
    fn walk_for_delimiter_errors(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        content: &str,
        errors: &mut Vec<DelimiterError>,
    ) {
        loop {
            let node = cursor.node();

            // Check if this is an error node
            if node.is_error() || node.is_missing() {
                // Determine what delimiter is missing
                if let Some(error) = self.analyze_delimiter_error(&node, content) {
                    errors.push(error);
                }
            }

            // Recurse into children
            if cursor.goto_first_child() {
                self.walk_for_delimiter_errors(cursor, content, errors);
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Analyze an error node to determine what delimiter is missing
    /// Returns the line where content ends (skipping comments/empty lines)
    fn analyze_delimiter_error(
        &self,
        node: &tree_sitter::Node,
        content: &str,
    ) -> Option<DelimiterError> {
        // Use the error node's END position instead of START position
        // The error span often extends closer to where the fix should go
        let end_byte = node.end_byte();
        let error_line = node.end_position().row;

        // DEBUG: Print error node info
        debug!(
            "🔍 ERROR NODE: start={}, end={}, end_line={}, kind={}",
            node.start_position().row,
            node.end_position().row,
            error_line,
            node.kind()
        );

        // Get the text before the error END to determine context
        let context_start = 0;
        let context = &content[context_start..end_byte.min(content.len())];

        // Count unmatched delimiters AND track line of last opening delimiter
        let mut brace_count = 0;
        let mut bracket_count = 0;
        let mut paren_count = 0;
        let mut _last_opening_brace_line = 0;
        let mut _last_opening_bracket_line = 0;
        let mut _last_opening_paren_line = 0;
        let mut in_string = false;
        let mut string_char = '"';
        let mut current_line = 0;

        for ch in context.chars() {
            if ch == '\n' {
                current_line += 1;
            }
            match ch {
                '{' if !in_string => {
                    brace_count += 1;
                    _last_opening_brace_line = current_line;
                }
                '}' if !in_string => brace_count -= 1,
                '[' if !in_string => {
                    bracket_count += 1;
                    _last_opening_bracket_line = current_line;
                }
                ']' if !in_string => bracket_count -= 1,
                '(' if !in_string => {
                    paren_count += 1;
                    _last_opening_paren_line = current_line;
                }
                ')' if !in_string => paren_count -= 1,
                '"' | '\'' => {
                    if in_string && ch == string_char {
                        in_string = false;
                    } else if !in_string {
                        in_string = true;
                        string_char = ch;
                    }
                }
                _ => {}
            }
        }

        // Use current_line (last line of context) as the base insertion line
        // This is the line just before the error, which is where we want to insert
        let lines: Vec<&str> = content.lines().collect();
        let mut insertion_line = current_line;

        // Skip backwards over empty lines and comments
        while insertion_line > 0 {
            if let Some(line) = lines.get(insertion_line) {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("/*") {
                    break; // Found last content line
                }
            }
            insertion_line -= 1;
        }

        // DEBUG: Print delimiter counts
        debug!(
            "  📊 Counts: braces={}, brackets={}, parens={}, in_string={}",
            brace_count, bracket_count, paren_count, in_string
        );
        debug!("  📍 Insertion line calculated: {}", insertion_line);

        // Determine what's missing and return insertion position
        if brace_count > 0 {
            debug!("  ✅ Returning missing '}}' for line {}", insertion_line);
            return Some(DelimiterError {
                line: insertion_line,
                _column: 0,
                missing_delimiter: "}".to_string(),
                _error_type: "unmatched_brace".to_string(),
            });
        }

        if bracket_count > 0 {
            debug!("  ✅ Returning missing ']' for line {}", insertion_line);
            return Some(DelimiterError {
                line: insertion_line,
                _column: 0,
                missing_delimiter: "]".to_string(),
                _error_type: "unmatched_bracket".to_string(),
            });
        }

        if paren_count > 0 {
            debug!("  ✅ Returning missing ')' for line {}", insertion_line);
            return Some(DelimiterError {
                line: insertion_line,
                _column: 0,
                missing_delimiter: ")".to_string(),
                _error_type: "unmatched_paren".to_string(),
            });
        }

        if in_string {
            return Some(DelimiterError {
                line: insertion_line,
                _column: 0,
                missing_delimiter: format!("{}", string_char),
                _error_type: "unclosed_string".to_string(),
            });
        }

        None
    }

    /// Apply token optimization to SmartRefactorTool responses to prevent context overflow
    /// Pass-through for minimal messages (no optimization needed)
    fn optimize_response(&self, message: &str) -> String {
        // Messages are now minimal 2-line summaries - no optimization needed
        message.to_string()
    }
}

fn looks_like_doc_comment(comment: &str) -> bool {
    let trimmed = comment.trim_start();

    trimmed.starts_with("///")
        || trimmed.starts_with("//!")
        || trimmed.starts_with("///<")
        || trimmed.starts_with("//!<")
        || trimmed.starts_with("/**")
        || trimmed.starts_with("/*!")
}

fn replace_identifier_with_boundaries<F>(
    text: &str,
    old: &str,
    new: &str,
    is_identifier_char: &F,
) -> (String, bool)
where
    F: Fn(char) -> bool,
{
    if old.is_empty() {
        return (text.to_string(), false);
    }

    let mut result = String::with_capacity(text.len());
    let mut last_index = 0;
    let mut changed = false;

    for (idx, _) in text.match_indices(old) {
        let mut valid = true;

        if let Some(prev_char) = text[..idx].chars().rev().next() {
            if is_identifier_char(prev_char) {
                valid = false;
            }
        }

        let end = idx + old.len();
        if valid {
            if let Some(next_char) = text[end..].chars().next() {
                if is_identifier_char(next_char) {
                    valid = false;
                }
            }
        }

        if !valid {
            continue;
        }

        result.push_str(&text[last_index..idx]);
        result.push_str(new);
        last_index = end;
        changed = true;
    }

    result.push_str(&text[last_index..]);
    if changed {
        (result, true)
    } else {
        (text.to_string(), false)
    }
}

const DOC_COMMENT_LOOKBACK_BYTES: usize = 256;
const TOP_OF_SCOPE_COMMENT_LINE_WINDOW: usize = 2;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_refs_result_handles_confidence_suffix() {
        let tool = SmartRefactorTool {
            operation: "rename_symbol".to_string(),
            params: "{}".to_string(),
            dry_run: true,
        };

        let content = "🔗 Reference: OldSymbol - src/lib.rs:42 (confidence: 0.95)";
        let result = CallToolResult::text_content(vec![TextContent::from(content)]);

        let parsed = tool
            .parse_refs_result(&result)
            .expect("parse should succeed");
        let lines = parsed.get("src/lib.rs").expect("file should be captured");
        assert_eq!(lines, &vec![42]);
    }

    #[test]
    fn parse_refs_result_prefers_structured_content() {
        let tool = SmartRefactorTool {
            operation: "rename_symbol".to_string(),
            params: "{}".to_string(),
            dry_run: true,
        };

        let structured = json!({
            "references": [
                {
                    "file_path": "src/main.rs",
                    "line_number": 128
                }
            ],
            "definitions": [
                {
                    "file_path": "src/lib.rs",
                    "start_line": 12
                }
            ]
        });

        let result = if let serde_json::Value::Object(map) = structured {
            CallToolResult::text_content(vec![]).with_structured_content(map)
        } else {
            panic!("expected structured object");
        };

        let parsed = tool
            .parse_refs_result(&result)
            .expect("parse should succeed");
        assert_eq!(parsed.get("src/main.rs"), Some(&vec![128]));
        assert_eq!(parsed.get("src/lib.rs"), Some(&vec![12]));
    }
}
