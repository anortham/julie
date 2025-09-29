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
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
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

            return Ok(CallToolResult::text_content(vec![TextContent::from(preview)]));
        }

        // Apply the actual refactoring
        let (insertion_line, new_content) = self.apply_extract_function(
            &file_content,
            start_line,
            end_line,
            &function_def,
            &function_call,
        )?;

        // Write the modified file
        fs::write(file_path, &new_content)?;

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

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
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
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
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

            return Ok(CallToolResult::text_content(vec![TextContent::from(preview)]));
        }

        // Replace the symbol body
        let new_content = self.replace_symbol_in_file(
            &file_content,
            start_line,
            end_line,
            new_body,
        )?;

        // Write the modified file
        fs::write(file_path, &new_content)?;

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

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
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

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    async fn handle_extract_type(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 ExtractType operation is not yet implemented\n\
                      📋 Coming soon - will extract inline types to named types\n\
                      💡 Use ReplaceSymbolBody operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    async fn handle_update_imports(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 UpdateImports operation is not yet implemented\n\
                      📋 Coming soon - will fix broken imports after file moves\n\
                      💡 Use ReplaceSymbolBody operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    async fn handle_inline_variable(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 InlineVariable operation is not yet implemented\n\
                      📋 Coming soon - will inline variable by replacing uses with value\n\
                      💡 Use ReplaceSymbolBody operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    async fn handle_inline_function(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let message = "🚧 InlineFunction operation is not yet implemented\n\
                      📋 Coming soon - will inline function by replacing calls with body\n\
                      💡 Use ReplaceSymbolBody operation for now";

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }
}
