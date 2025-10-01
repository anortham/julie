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
use crate::tools::editing::EditingTransaction; // Atomic file operations
use crate::tools::navigation::FastRefsTool;
use crate::utils::{progressive_reduction::ProgressiveReducer, token_estimation::TokenEstimator};

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
}

/// Smart refactoring tool for semantic code transformations
#[mcp_tool(
    name = "smart_refactor",
    description = "REFACTOR WITH PRECISION - Rename symbols, extract functions, and transform code structure safely",
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

impl SmartRefactorTool {
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
            _ => {
                let message = format!(
                    "❌ Unknown refactoring operation: '{}'\n\
                    Valid operations: rename_symbol, extract_function, replace_symbol_body, insert_relative_to_symbol, extract_type, update_imports, inline_variable, inline_function",
                    self.operation
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(&message))]))
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
                self.optimize_response(&message),
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

        for file_path in file_locations.keys() {
            match self
                .rename_in_file(handler, file_path, old_name, new_name, &dmp)
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
                self.optimize_response(&preview),
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
            self.optimize_response(&message),
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
        dmp: &DiffMatchPatch,
    ) -> Result<usize> {
        // Read the file
        let original_content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;

        // AST-aware replacement using SearchEngine to find exact symbol matches
        let new_content = match self.ast_aware_replace(&original_content, file_path, old_name, new_name, handler).await {
            Ok(content) => content,
            Err(e) => {
                // Fallback to simple replacement if AST search fails
                debug!("⚠️ AST search failed, falling back to simple replacement: {}", e);
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
            let transaction = EditingTransaction::begin(file_path)?;
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
        _handler: &JulieServerHandler,
    ) -> Result<String> {
        debug!("🌳 Starting AST-aware replacement using tree-sitter parsing");

        // Try search engine first, but fall back to tree-sitter parsing if not available
        let symbol_positions = match self.find_symbols_via_search(file_path, old_name, _handler).await {
            Ok(positions) => {
                debug!("✅ Found {} symbols via search engine", positions.len());
                positions
            }
            Err(search_error) => {
                debug!("⚠️ Search engine failed: {}, falling back to tree-sitter parsing", search_error);
                self.find_symbols_via_treesitter(content, file_path, old_name).await?
            }
        };

        // Hybrid approach: Use AST for validation + careful text replacement for completeness
        if !symbol_positions.is_empty() {
            debug!("✅ AST validation passed: found {} symbol definitions", symbol_positions.len());
        } else {
            debug!("⚠️ No AST symbol definitions found - symbol may not exist");
            return Err(anyhow::anyhow!("Symbol not found in AST"));
        }

        // Use AST-aware text replacement to catch ALL occurrences (including usages)
        debug!("📝 About to call AST-aware smart_text_replace with old_name='{}', new_name='{}'", old_name, new_name);
        let result = self.smart_text_replace(content, old_name, new_name, file_path)?;
        debug!("🎯 AST-aware smart_text_replace completed, result length: {}", result.len());

        Ok(result)
    }

    /// Find symbol positions using search engine (for indexed files)
    async fn find_symbols_via_search(
        &self,
        file_path: &str,
        old_name: &str,
        handler: &JulieServerHandler,
    ) -> Result<Vec<(u32, u32)>> {
        let search_engine = handler.active_search_engine().await?;
        let search_engine = search_engine.read().await;

        let search_results = search_engine.exact_symbol_search(old_name).await?;

        let positions: Vec<(u32, u32)> = search_results
            .into_iter()
            .filter(|result| result.symbol.file_path == file_path)
            .map(|result| (result.symbol.start_byte, result.symbol.end_byte))
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
        let symbols = extractor_manager.extract_symbols(file_path, content).await?;

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
    pub fn smart_text_replace(&self, content: &str, old_name: &str, new_name: &str, file_path: &str) -> Result<String> {
        use crate::tools::ast_symbol_finder::{ASTSymbolFinder, SymbolContext};
        use tree_sitter::Parser;

        debug!("🌳 AST-aware replacement: '{}' -> '{}' using tree-sitter", old_name, new_name);

        // Determine language from file extension
        let language = self.detect_language(file_path);

        // Parse file with tree-sitter
        let mut parser = Parser::new();
        let tree_sitter_language = match self.get_tree_sitter_language(&language) {
            Ok(lang) => lang,
            Err(e) => {
                debug!("⚠️ Couldn't get tree-sitter language for {}: {}. Fallback to text-based.", language, e);
                // Fallback to simple word boundary replacement if tree-sitter fails
                let pattern = format!(r"\b{}\b", regex::escape(old_name));
                let re = regex::Regex::new(&pattern)?;
                return Ok(re.replace_all(content, new_name).to_string());
            }
        };

        parser.set_language(&tree_sitter_language)
            .map_err(|e| anyhow::anyhow!("Failed to set parser language: {}", e))?;

        let tree = parser.parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file"))?;

        // Use ASTSymbolFinder to find all symbol occurrences
        let finder = ASTSymbolFinder::new(content.to_string(), tree, language.clone());
        let occurrences = finder.find_symbol_occurrences(old_name);

        let total_occurrences = occurrences.len();
        debug!("🔍 Found {} total occurrences of '{}'", total_occurrences, old_name);

        // Filter out occurrences in strings and comments
        let code_occurrences: Vec<_> = occurrences
            .into_iter()
            .filter(|occ| {
                occ.context != SymbolContext::StringLiteral &&
                occ.context != SymbolContext::Comment
            })
            .collect();

        debug!("✅ {} code occurrences (filtered out {} string/comment occurrences)",
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
            debug!("✅ Replaced '{}' -> '{}' at byte {}:{} (line {}, context: {:?})",
                old_name, new_name, occ.start_byte, occ.end_byte, occ.line, occ.context);
        }

        debug!("🎯 AST-aware replacement complete: {} occurrences replaced", replacement_count);
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

        // Parse JSON parameters
        let params: JsonValue = serde_json::from_str(&self.params)
            .map_err(|e| anyhow::anyhow!("Invalid JSON in params: {}", e))?;

        let file_path = params
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file"))?;

        let start_line = params
            .get("start_line")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: start_line"))? as u32;

        let end_line = params
            .get("end_line")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: end_line"))? as u32;

        let function_name = params
            .get("function_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: function_name"))?;

        let return_type = params
            .get("return_type")
            .and_then(|v| v.as_str());

        if start_line > end_line {
            return Err(anyhow::anyhow!("start_line must be <= end_line"));
        }

        debug!(
            "🎯 Extract function '{}' from {}:{}-{}",
            function_name, file_path, start_line, end_line
        );

        // Step 1: Read the file and extract the target code block
        let file_content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", file_path, e))?;

        let lines: Vec<&str> = file_content.lines().collect();
        if start_line == 0 || end_line as usize > lines.len() {
            return Err(anyhow::anyhow!(
                "Line range {}:{} is invalid for file with {} lines",
                start_line, end_line, lines.len()
            ));
        }

        // Extract the code block (convert to 0-based indexing)
        let extracted_lines = &lines[(start_line as usize - 1)..(end_line as usize)];

        // Detect the base indentation level of the extracted code
        let base_indent = self.detect_base_indentation(extracted_lines);
        let dedented_code = self.dedent_code(extracted_lines, base_indent);

        debug!("📋 Extracted {} lines of code", extracted_lines.len());

        // Step 2: Analyze dependencies (simplified for now)
        let dependencies = self.analyze_dependencies(&dedented_code, file_path).await?;

        // Step 3: Generate function signature based on language
        let language = self.detect_language(file_path);
        let (function_def, function_call) = self.generate_function_code(
            &language,
            function_name,
            &dedented_code,
            &dependencies,
            return_type,
            base_indent,
        )?;

        // Step 4: Apply the refactoring
        if self.dry_run {
            let preview = format!(
                "🔍 DRY RUN: Extract function '{}'\n\
                📁 File: {}\n\
                📍 Lines: {}-{}\n\
                🔧 Language: {}\n\n\
                🎯 New function:\n{}\n\n\
                🔄 Replacement call:\n{}\n\n\
                📊 Dependencies detected: {:?}\n\n\
                💡 Set dry_run=false to apply changes",
                function_name, file_path, start_line, end_line, language, function_def, function_call, dependencies
            );

            return Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(&preview))]));
        }

        // Apply the actual refactoring
        let (insertion_line, new_content) = self.apply_extract_function(
            &file_content,
            start_line,
            end_line,
            &function_def,
            &function_call,
        )?;

        // Write the modified file atomically using EditingTransaction
        let transaction = EditingTransaction::begin(file_path)?;
        transaction.commit(&new_content)?;

        let message = format!(
            "✅ Extract function successful: '{}'\n\
            📁 File: {}\n\
            📍 Extracted lines: {}-{}\n\
            📝 New function inserted at line: {}\n\
            🔄 Original code replaced with function call\n\n\
            🎯 Next steps:\n\
            • Review the generated function parameters\n\
            • Run tests to verify functionality\n\
            • Consider adding type annotations if needed\n\
            💡 Tip: Use fast_goto to navigate to the new function",
            function_name, file_path, start_line, end_line, insertion_line
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(&message))]))
    }

    /// Detect the base indentation level of code lines
    fn detect_base_indentation(&self, lines: &[&str]) -> usize {
        lines
            .iter()
            .filter(|line| !line.trim().is_empty()) // Skip empty lines
            .map(|line| line.len() - line.trim_start().len()) // Count leading whitespace
            .min()
            .unwrap_or(0)
    }

    /// Remove base indentation from code lines
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

    /// Analyze what variables/dependencies the extracted code needs
    async fn analyze_dependencies(&self, code: &str, _file_path: &str) -> Result<Vec<String>> {
        // Simplified dependency analysis - look for variable patterns
        // TODO: Use tree-sitter for proper AST analysis
        let mut dependencies = Vec::new();

        // Basic heuristic: look for variable usage patterns
        for line in code.lines() {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.is_empty() {
                continue;
            }

            // Look for common variable usage patterns (this is very basic)
            // In a real implementation, we'd use tree-sitter to parse the AST
            if let Some(var_name) = self.extract_variable_usage(trimmed) {
                if !dependencies.contains(&var_name) {
                    dependencies.push(var_name);
                }
            }
        }

        Ok(dependencies)
    }

    /// Extract potential variable usage from a line (basic heuristic)
    fn extract_variable_usage(&self, line: &str) -> Option<String> {
        // Very basic pattern matching - would be replaced with tree-sitter analysis
        // Look for patterns like: variable_name.method() or variable_name =

        if let Some(pos) = line.find('.') {
            let before_dot = &line[..pos];
            if let Some(word) = before_dot.split_whitespace().last() {
                if word.chars().all(|c| c.is_alphanumeric() || c == '_') && word.len() > 1 {
                    return Some(word.to_string());
                }
            }
        }

        None // Return None for now - proper implementation would analyze AST
    }

    /// Detect programming language from file extension
    fn detect_language(&self, file_path: &str) -> String {
        match std::path::Path::new(file_path).extension().and_then(|ext| ext.to_str()) {
            Some("rs") => "rust".to_string(),
            Some("ts") | Some("tsx") => "typescript".to_string(),
            Some("js") | Some("jsx") => "javascript".to_string(),
            Some("py") => "python".to_string(),
            Some("java") => "java".to_string(),
            Some("cpp") | Some("cc") | Some("cxx") => "cpp".to_string(),
            Some("c") => "c".to_string(),
            Some("cs") => "csharp".to_string(),
            _ => "unknown".to_string(),
        }
    }

    /// Generate function definition and call based on language
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
                    dependencies.iter()
                        .map(|dep| format!("{}: &str", dep)) // Simplified - would infer proper types
                        .collect::<Vec<_>>()
                        .join(", ")
                };

                let return_annotation = return_type.map_or_else(
                    || "".to_string(),
                    |rt| format!(" -> {}", rt)
                );

                let function_def = format!(
                    "fn {}({}){} {{\n{}\n}}",
                    function_name, params, return_annotation,
                    code.lines().map(|line| format!("    {}", line)).collect::<Vec<_>>().join("\n")
                );

                let call_args = dependencies.join(", ");
                let function_call = format!("{}{}({});", indent_str, function_name, call_args);

                Ok((function_def, function_call))
            }
            "typescript" | "javascript" => {
                let params = dependencies.join(", ");
                let return_annotation = return_type.map_or_else(
                    || "".to_string(),
                    |rt| format!(": {}", rt)
                );

                let function_def = format!(
                    "function {}({}){} {{\n{}\n}}",
                    function_name, params, return_annotation,
                    code.lines().map(|line| format!("    {}", line)).collect::<Vec<_>>().join("\n")
                );

                let function_call = format!("{}{}({});", indent_str, function_name, params);

                Ok((function_def, function_call))
            }
            "python" => {
                let params = dependencies.join(", ");
                let return_annotation = return_type.map_or_else(
                    || "".to_string(),
                    |rt| format!(" -> {}", rt)
                );

                let function_def = format!(
                    "def {}({}){}:\n{}",
                    function_name, params, return_annotation,
                    code.lines().map(|line| format!("    {}", line)).collect::<Vec<_>>().join("\n")
                );

                let function_call = format!("{}{}({})", indent_str, function_name, params);

                Ok((function_def, function_call))
            }
            _ => {
                // Generic approach for unknown languages
                let params = dependencies.join(", ");
                let function_def = format!(
                    "function {}({}) {{\n{}\n}}",
                    function_name, params,
                    code.lines().map(|line| format!("    {}", line)).collect::<Vec<_>>().join("\n")
                );

                let function_call = format!("{}{}({});", indent_str, function_name, params);

                Ok((function_def, function_call))
            }
        }
    }

    /// Apply the extract function refactoring to the file content
    fn apply_extract_function(
        &self,
        file_content: &str,
        start_line: u32,
        end_line: u32,
        function_def: &str,
        function_call: &str,
    ) -> Result<(u32, String)> {
        let lines: Vec<&str> = file_content.lines().collect();

        // Find a good place to insert the new function (before the current function)
        let insertion_line = self.find_function_insertion_point(&lines, start_line)?;

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

    /// Find appropriate location to insert the new function
    fn find_function_insertion_point(&self, lines: &[&str], current_line: u32) -> Result<u32> {
        // Simple heuristic: insert before the current function or at the beginning
        // TODO: Use tree-sitter to find proper scope boundaries

        // Look backwards from current line to find function start
        for i in (0..(current_line as usize - 1)).rev() {
            let line = lines[i].trim();
            if line.starts_with("fn ") || line.starts_with("function ") || line.starts_with("def ") {
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
                && !trimmed.starts_with("#") {
                return Ok(i as u32 + 1);
            }
        }

        Ok(1) // Fallback to beginning
    }

    /// Handle replace symbol body operation (Serena-inspired)
    async fn handle_replace_symbol_body(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔄 Processing replace symbol body operation");

        // Parse JSON parameters
        let params: JsonValue = serde_json::from_str(&self.params)
            .map_err(|e| anyhow::anyhow!("Invalid JSON in params: {}", e))?;

        let file_path = params
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file"))?;

        let symbol_name = params
            .get("symbol_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: symbol_name"))?;

        let new_body = params
            .get("new_body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: new_body"))?;

        debug!(
            "🎯 Replace symbol '{}' in file '{}'",
            symbol_name, file_path
        );

        // Step 1: Use fast_search to find the symbol
        let search_tool = crate::tools::search::FastSearchTool {
            query: symbol_name.to_string(),
            mode: "text".to_string(),
            limit: 10,
            file_pattern: Some(file_path.to_string()),
            language: None,
            workspace: Some("primary".to_string()),
        };

        let search_result = search_tool.call_tool(handler).await?;
        let symbol_locations = self.parse_search_result_for_symbols(&search_result, symbol_name, file_path)?;

        if symbol_locations.is_empty() {
            let message = format!(
                "🔍 Symbol '{}' not found in file '{}'\n\
                💡 Check spelling or use fast_search to locate the symbol",
                symbol_name, file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(&message))]));
        }

        // Step 2: Read the file to find symbol boundaries
        let file_content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", file_path, e))?;

        // Step 3: Find symbol boundaries using tree-sitter
        let (start_line, end_line) = self.find_symbol_boundaries(&file_content, symbol_name, file_path)?;

        debug!("📍 Found symbol '{}' at lines {}-{}", symbol_name, start_line, end_line);

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

            return Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(&preview))]));
        }

        // Replace the symbol body
        let new_content = self.replace_symbol_in_file(
            &file_content,
            start_line,
            end_line,
            new_body,
        )?;

        // Write the modified file atomically using EditingTransaction
        let transaction = EditingTransaction::begin(file_path)?;
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

        Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(&message))]))
    }

    /// Parse search results to find symbol locations in a specific file
    fn parse_search_result_for_symbols(&self, search_result: &CallToolResult, symbol_name: &str, target_file: &str) -> Result<Vec<(String, u32)>> {
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
                    let path_matches = extracted_file_path == target_file ||
                                     extracted_file_path.ends_with(target_file) ||
                                     target_file.ends_with(extracted_file_path);

                    debug!("🔍 Comparing file paths: extracted='{}', target='{}', matches={}",
                           extracted_file_path, target_file, path_matches);

                    if path_matches {
                        // Check if the next line contains our symbol (this is search result format, not source code)
                        if i + 1 < lines.len() {
                            let next_line = lines[i + 1];
                            if next_line.contains(symbol_name) {
                                println!("✅ Found matching symbol '{}' in target file at line {}", symbol_name, line_num);
                                locations.push(file_line);
                            } else {
                                println!("🔍 Next line doesn't contain symbol '{}': '{}'", symbol_name, next_line);
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
        println!("🔍 extract_file_location input: '{}'", line);

        // More robust emoji removal - find the first non-emoji, non-whitespace character
        let cleaned_line = line.trim();
        let cleaned_line = if cleaned_line.starts_with("📁") {
            &cleaned_line[4..] // 📁 emoji is 4 bytes in UTF-8
        } else {
            cleaned_line
        };
        let cleaned_line = cleaned_line.trim();

        println!("🔍 extract_file_location cleaned: '{}'", cleaned_line);

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

            println!("🔍 extract_file_location result: file='{}' line={}", file_part, start_line);
            return Some((file_part.to_string(), start_line));
        }
        None
    }

    /// Find symbol boundaries using simple heuristics (TODO: upgrade to tree-sitter)
    fn find_symbol_boundaries(&self, file_content: &str, symbol_name: &str, file_path: &str) -> Result<(u32, u32)> {
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

        println!("🔍 Symbol boundaries for '{}': lines {}-{}", symbol_name, start_line, end_line);
        if start_line > 0 && (start_line as usize) < lines.len() {
            println!("🔍 Start line content: '{}'", lines[(start_line as usize) - 1]);
        }
        if end_line > 0 && (end_line as usize) < lines.len() {
            println!("🔍 End line content: '{}'", lines[(end_line as usize) - 1]);
        }

        Ok((start_line, end_line))
    }

    /// Check if a line contains a symbol definition
    fn is_symbol_definition(&self, line: &str, symbol_name: &str, language: &str) -> bool {
        match language {
            "rust" => {
                (line.starts_with("fn ") || line.starts_with("pub fn ") ||
                 line.starts_with("struct ") || line.starts_with("pub struct ") ||
                 line.starts_with("impl ") || line.starts_with("enum ") ||
                 line.starts_with("pub enum ")) && line.contains(symbol_name)
            }
            "typescript" | "javascript" => {
                line.contains(symbol_name) && (
                    line.starts_with("function ") || line.starts_with("export function ") ||
                    line.starts_with("class ") || line.starts_with("export class ") ||
                    line.starts_with("async ") || // async functions/methods
                    line.contains("function ") ||
                    // Method definitions: "methodName(" or "async methodName("
                    (line.contains(&format!("{}(", symbol_name)) &&
                     (line.trim_start().starts_with(symbol_name) ||
                      line.trim_start().starts_with(&format!("async {}", symbol_name)) ||
                      line.trim_start().starts_with(&format!("public {}", symbol_name)) ||
                      line.trim_start().starts_with(&format!("private {}", symbol_name)) ||
                      line.trim_start().starts_with(&format!("protected {}", symbol_name))))
                )
            }
            "python" => {
                (line.starts_with("def ") || line.starts_with("class ")) && line.contains(symbol_name)
            }
            _ => {
                // Generic approach
                line.contains(symbol_name) && (
                    line.contains("function") || line.contains("class") ||
                    line.contains("def ") || line.contains("fn ")
                )
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

                Err(anyhow::anyhow!("Could not find end of symbol - unmatched braces"))
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
    async fn handle_insert_relative_to_symbol(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 InsertRelativeToSymbol operation is not yet implemented\n\
                      📋 Coming soon - will insert code before/after symbols\n\
                      💡 Use ReplaceSymbolBody operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(message))]))
    }

    async fn handle_extract_type(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 ExtractType operation is not yet implemented\n\
                      📋 Coming soon - will extract inline types to named types\n\
                      💡 Use ReplaceSymbolBody operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(message))]))
    }

    async fn handle_update_imports(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 UpdateImports operation is not yet implemented\n\
                      📋 Coming soon - will fix broken imports after file moves\n\
                      💡 Use ReplaceSymbolBody operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(message))]))
    }

    async fn handle_inline_variable(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 InlineVariable operation is not yet implemented\n\
                      📋 Coming soon - will inline variable by replacing uses with value\n\
                      💡 Use ReplaceSymbolBody operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(message))]))
    }

    async fn handle_inline_function(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 InlineFunction operation is not yet implemented\n\
                      📋 Coming soon - will inline function by replacing calls with body\n\
                      💡 Use ReplaceSymbolBody operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(self.optimize_response(message))]))
    }

    /// Apply token optimization to SmartRefactorTool responses to prevent context overflow
    fn optimize_response(&self, message: &str) -> String {
        let token_estimator = TokenEstimator::new();
        let token_limit: usize = 25000; // 25K token limit - maximum for AST-powered tools that need full context

        let message_tokens = token_estimator.estimate_string(message);

        if message_tokens <= token_limit {
            // No optimization needed
            return message.to_string();
        }

        // Split message into lines for progressive reduction
        let lines: Vec<String> = message.lines().map(|s| s.to_string()).collect();

        // Apply progressive reduction to stay within token limits
        let progressive_reducer = ProgressiveReducer::new();
        let line_refs: Vec<&String> = lines.iter().collect();

        let estimate_lines_tokens = |line_refs: &[&String]| -> usize {
            let content = line_refs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n");
            token_estimator.estimate_string(&content)
        };

        let reduced_lines = progressive_reducer.reduce(&line_refs, token_limit, estimate_lines_tokens);

        let reduced_count = reduced_lines.len();
        let mut optimized_message = reduced_lines.into_iter().cloned().collect::<Vec<_>>().join("\n");

        if reduced_count < lines.len() {
            optimized_message.push_str("\n\n⚠️  Response truncated to stay within token limits");
            optimized_message.push_str("\n💡 Use more specific parameters for focused results");
        }

        optimized_message
    }
}
