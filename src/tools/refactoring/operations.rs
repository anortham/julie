//! Refactoring operations - extract, replace, and insert operations

use anyhow::{anyhow, Result};
use rust_mcp_sdk::schema::CallToolResult;
use serde_json::Value as JsonValue;

use super::SmartRefactorTool;
use crate::handler::JulieServerHandler;

impl SmartRefactorTool {
    /// Handle extract symbol to file operation
    pub async fn handle_extract_symbol_to_file(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        use std::fs;
        use std::path::Path;
        use tree_sitter::Parser;

        // Parse parameters
        let params: JsonValue = serde_json::from_str(&self.params)
            .map_err(|e| anyhow!("Invalid JSON in params: {}", e))?;

        let source_file = params
            .get("source_file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: source_file"))?;

        let target_file = params
            .get("target_file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: target_file"))?;

        let symbol_name = params
            .get("symbol_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: symbol_name"))?;

        let update_imports = params
            .get("update_imports")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Validate source file exists
        if !Path::new(source_file).exists() {
            return Err(anyhow!("Source file does not exist: {}", source_file));
        }

        // Read source file
        let source_content = fs::read_to_string(source_file)?;

        // Detect language
        let language = self.detect_language(source_file);
        if language == "unknown" {
            return Err(anyhow!("Unsupported file type: {}", source_file));
        }

        // Parse with tree-sitter
        let ts_language = self.get_tree_sitter_language(&language)?;
        let mut parser = Parser::new();
        parser.set_language(&ts_language)?;

        let tree = parser
            .parse(&source_content, None)
            .ok_or_else(|| anyhow!("Failed to parse {} file", language))?;

        // Find the symbol to extract
        let root = tree.root_node();
        let symbol_node = self.find_any_symbol(root, symbol_name, &source_content)?;

        // Extract the complete symbol text (including signature and body)
        let symbol_start = symbol_node.start_byte();
        let symbol_end = symbol_node.end_byte();
        let _symbol_text = &source_content[symbol_start..symbol_end];

        // Find line boundaries for clean extraction
        let line_start = source_content[..symbol_start]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(0);
        let line_end = source_content[symbol_end..]
            .find('\n')
            .map(|pos| symbol_end + pos + 1) // Include the newline
            .unwrap_or(source_content.len());

        let extracted_text = source_content[line_start..line_end].trim_end().to_string();

        // Build new source content (remove the symbol)
        let mut new_source_content = String::new();
        new_source_content.push_str(&source_content[..line_start]);
        // Skip the extracted lines
        new_source_content.push_str(&source_content[line_end..]);

        // Clean up: collapse multiple consecutive blank lines to just one blank line
        // (two newlines total: one ends the previous content, one is the blank line)
        let cleaned = new_source_content
            .replace("\n\n\n\n", "\n\n")
            .replace("\n\n\n", "\n\n");
        new_source_content = cleaned;

        // If update_imports is true, add import statement at top of source file
        if update_imports {
            let import_statement =
                self.generate_import_statement(symbol_name, source_file, target_file, &language)?;
            new_source_content = self.add_import_to_top(&new_source_content, &import_statement);
        }

        // Handle target file (create or append)
        let target_content = if Path::new(target_file).exists() {
            // Append to existing file
            let existing = fs::read_to_string(target_file)?;
            format!("{}\n\n{}", existing.trim_end(), extracted_text)
        } else {
            // Create new file with extracted symbol
            extracted_text.clone()
        };

        // Write files if not dry run
        if !self.dry_run {
            use crate::tools::editing::EditingTransaction;

            // Write source file (symbol removed)
            let tx_source = EditingTransaction::begin(source_file)?;
            tx_source.commit(&new_source_content)?;

            // Write target file (symbol added)
            fs::write(target_file, &target_content)?;
        }

        let message = if self.dry_run {
            format!(
                "DRY RUN: Would extract '{}' from {} to {}\n\n\
                Symbol: {} lines\n\
                Update imports: {}",
                symbol_name,
                source_file,
                target_file,
                extracted_text.lines().count(),
                update_imports
            )
        } else {
            format!(
                "✅ Successfully extracted '{}' from {} to {}\n\n\
                Symbol: {} lines\n\
                Import added: {}",
                symbol_name,
                source_file,
                target_file,
                extracted_text.lines().count(),
                update_imports
            )
        };

        self.create_result(
            "extract_symbol_to_file",
            true,
            vec![source_file.to_string(), target_file.to_string()],
            1,
            vec!["Review both source and target files".to_string()],
            message,
            Some(serde_json::json!({
                "source_file": source_file,
                "target_file": target_file,
                "symbol": symbol_name,
                "lines_extracted": extracted_text.lines().count(),
                "import_added": update_imports,
            })),
        )
    }

    /// Generate import statement for extracted symbol
    fn generate_import_statement(
        &self,
        symbol_name: &str,
        source_file: &str,
        target_file: &str,
        language: &str,
    ) -> Result<String> {
        // Extract relative path without extension
        let source_path = std::path::Path::new(source_file);
        let target_path = std::path::Path::new(target_file);

        // Get target file name without extension for import
        let target_name = target_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("module");

        // Get relative path from source to target
        let source_dir = source_path.parent().unwrap_or(std::path::Path::new("."));
        let relative_path = if target_path.parent() == Some(source_dir) {
            // Same directory
            format!("./{}", target_name)
        } else {
            // Different directory - use simple relative path
            format!("./{}", target_name)
        };

        // Generate language-specific import
        let import = match language {
            "typescript" | "javascript" => {
                format!("import {{ {} }} from '{}';", symbol_name, relative_path)
            }
            "python" => {
                format!("from {} import {}", target_name, symbol_name)
            }
            "rust" => {
                // Convert file path to module path (e.g., "helpers.rs" -> "crate::helpers")
                let module_path = target_name.replace('/', "::").replace('-', "_");
                format!("use crate::{}::{};", module_path, symbol_name)
            }
            _ => {
                // Default for other languages
                format!("// Import {} from {}", symbol_name, target_name)
            }
        };

        Ok(import)
    }

    /// Add import statement at the top of file, after any existing imports
    fn add_import_to_top(&self, content: &str, import_statement: &str) -> String {
        // Find the last import/use statement
        let lines: Vec<&str> = content.lines().collect();

        let mut insert_after_line = 0;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("import ")
                || trimmed.starts_with("from ")
                || trimmed.starts_with("use ")
                || trimmed.starts_with("#")
            // Python shebang/encoding
            {
                insert_after_line = i + 1;
            }
        }

        // Insert the import statement
        let mut result = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i == insert_after_line {
                result.push(import_statement.to_string());
                if !lines.get(i).map(|l| l.trim().is_empty()).unwrap_or(true) {
                    result.push(String::new()); // Add blank line
                }
            }
            result.push(line.to_string());
        }

        result.join("\n")
    }

    /// Handle replace symbol body operation
    pub async fn handle_replace_symbol_body(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        use std::fs;
        use tree_sitter::Parser;

        // Parse parameters
        let params: JsonValue = serde_json::from_str(&self.params)
            .map_err(|e| anyhow!("Invalid JSON in params: {}", e))?;

        let file_path = params
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: file"))?;

        let symbol_name = params
            .get("symbol_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: symbol_name"))?;

        let new_body = params
            .get("new_body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: new_body"))?;

        // Validate file exists
        if !std::path::Path::new(file_path).exists() {
            return Err(anyhow!("File does not exist: {}", file_path));
        }

        // Read file content
        let content = fs::read_to_string(file_path)?;

        // Detect language
        let language = self.detect_language(file_path);
        if language == "unknown" {
            return Err(anyhow!("Unsupported file type: {}", file_path));
        }

        // Parse with tree-sitter
        let ts_language = self.get_tree_sitter_language(&language)?;
        let mut parser = Parser::new();
        parser.set_language(&ts_language)?;

        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| anyhow!("Failed to parse {} file", language))?;

        // Find the function/method definition
        let root = tree.root_node();
        let symbol_node = self.find_function_or_method(root, symbol_name, &content)?;

        // Find the body node (the part between braces or indented block for Python)
        let body_node = symbol_node
            .child_by_field_name("body")
            .ok_or_else(|| anyhow!("Symbol '{}' has no body", symbol_name))?;

        // Get the byte range of the body content
        let mut body_start = body_node.start_byte();
        let body_end = body_node.end_byte();

        // For Python, the body node starts at the first statement, but we need to include
        // the newline after the colon. Find the previous newline.
        if language == "python" {
            // Find the colon before the body
            if let Some(colon_pos) = content[..body_start].rfind(':') {
                // Find the newline after the colon
                if let Some(newline_pos) = content[colon_pos..body_start].find('\n') {
                    body_start = colon_pos + newline_pos + 1;
                }
            }
        }

        // Extract surrounding context to preserve indentation
        let body_text = &content[body_start..body_end];

        // Detect target indentation from the first non-empty code line in the body
        // Skip brace-only lines like "{" or "}"
        let body_first_code_line = body_text.lines().find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && trimmed != "{" && trimmed != "}"
        });

        let target_body_indent = if let Some(line) = body_first_code_line {
            line.len() - line.trim_start().len()
        } else {
            // Fallback: detect from function signature + 4 spaces
            let lines_before_body: Vec<&str> = content[..body_start].lines().collect();
            if let Some(last_line) = lines_before_body.last() {
                last_line.len() - last_line.trim_start().len() + 4
            } else {
                4
            }
        };

        // Format the new body with proper indentation using our indentation helpers
        let formatted_body = if body_text.trim().starts_with('{') {
            // Brace-based language (JavaScript, TypeScript, Rust, etc.)
            // Keep the braces, replace content inside
            let reindented = super::indentation::reindent(new_body, target_body_indent);
            let closing_brace_indent = target_body_indent.saturating_sub(4);
            format!("{{\n{}\n{}}}", reindented, " ".repeat(closing_brace_indent))
        } else {
            // Indentation-based language (Python) or other non-brace languages
            // Just reindent the content to match the body indent level
            super::indentation::reindent(new_body, target_body_indent)
        };

        // Replace the body
        let mut result = String::new();
        result.push_str(&content[..body_start]);
        result.push_str(&formatted_body);
        result.push_str(&content[body_end..]);

        // Write back if not dry run
        if !self.dry_run {
            use crate::tools::editing::EditingTransaction;
            let tx = EditingTransaction::begin(file_path)?;
            tx.commit(&result)?;
        }

        let message = if self.dry_run {
            format!(
                "DRY RUN: Would replace body of '{}' in {}\n\n\
                Old body: {} chars\n\
                New body: {} chars",
                symbol_name,
                file_path,
                body_text.len(),
                formatted_body.len()
            )
        } else {
            format!(
                "✅ Successfully replaced body of '{}' in {}\n\n\
                Old body: {} chars\n\
                New body: {} chars",
                symbol_name,
                file_path,
                body_text.len(),
                formatted_body.len()
            )
        };

        self.create_result(
            "replace_symbol_body",
            true,
            vec![file_path.to_string()],
            1,
            vec!["Review the updated function/method".to_string()],
            message,
            Some(serde_json::json!({
                "file": file_path,
                "symbol": symbol_name,
                "old_size": body_text.len(),
                "new_size": formatted_body.len(),
            })),
        )
    }

    /// Find a function or method node by name
    fn find_function_or_method<'a>(
        &self,
        node: tree_sitter::Node<'a>,
        name: &str,
        content: &str,
    ) -> Result<tree_sitter::Node<'a>> {
        // Check if this node is a function/method with the matching name
        if self.is_function_or_method(&node) {
            if let Some(name_node) = node.child_by_field_name("name") {
                if let Ok(node_name) = name_node.utf8_text(content.as_bytes()) {
                    if node_name == name {
                        return Ok(node);
                    }
                }
            }
        }

        // Recursively search children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Ok(found) = self.find_function_or_method(child, name, content) {
                return Ok(found);
            }
        }

        Err(anyhow!("Symbol '{}' not found", name))
    }

    /// Check if a node is a function or method definition
    fn is_function_or_method(&self, node: &tree_sitter::Node) -> bool {
        matches!(
            node.kind(),
            "function_declaration"
                | "function_definition"  // Python, C, C++
                | "method_declaration"
                | "method_definition"
                | "function_item"       // Rust
                | "arrow_function"      // JavaScript/TypeScript
                | "function_expression" // JavaScript/TypeScript
        )
    }

    /// Handle insert relative to symbol operation
    pub async fn handle_insert_relative_to_symbol(
        &self,
        _handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        use std::fs;
        use tree_sitter::Parser;

        // Parse parameters
        let params: JsonValue = serde_json::from_str(&self.params)
            .map_err(|e| anyhow!("Invalid JSON in params: {}", e))?;

        let file_path = params
            .get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: file"))?;

        let target_symbol = params
            .get("target_symbol")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: target_symbol"))?;

        let position = params
            .get("position")
            .and_then(|v| v.as_str())
            .unwrap_or("after"); // Default to "after"

        let content_to_insert = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: content"))?;

        // Validate position
        if position != "before" && position != "after" {
            return Err(anyhow!(
                "Position must be 'before' or 'after', got: {}",
                position
            ));
        }

        // Validate file exists
        if !std::path::Path::new(file_path).exists() {
            return Err(anyhow!("File does not exist: {}", file_path));
        }

        // Read file content
        let content = fs::read_to_string(file_path)?;

        // Detect language
        let language = self.detect_language(file_path);
        if language == "unknown" {
            return Err(anyhow!("Unsupported file type: {}", file_path));
        }

        // Parse with tree-sitter
        let ts_language = self.get_tree_sitter_language(&language)?;
        let mut parser = Parser::new();
        parser.set_language(&ts_language)?;

        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| anyhow!("Failed to parse {} file", language))?;

        // Find the target symbol
        let root = tree.root_node();
        let symbol_node = self.find_any_symbol(root, target_symbol, &content)?;

        // Find the line containing the symbol
        let symbol_start_byte = symbol_node.start_byte();
        let symbol_end_byte = symbol_node.end_byte();

        // Find the start of the line containing the symbol
        let line_start = content[..symbol_start_byte]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(0);

        // Find the end of the line containing the symbol
        let line_end = content[symbol_end_byte..]
            .find('\n')
            .map(|pos| symbol_end_byte + pos)
            .unwrap_or(content.len());

        // Calculate insertion point based on position
        let insertion_byte = if position == "before" {
            line_start // Insert at start of line
        } else {
            line_end // Insert at end of line
        };

        // Get the symbol's line for indentation detection
        let symbol_line = &content[line_start..line_end];
        let target_indentation = symbol_line.len() - symbol_line.trim_start().len();

        // Use proper indentation handling: normalize source content, then reapply at target level
        let formatted_content = super::indentation::reindent(content_to_insert, target_indentation);

        let insertion_text = if position == "before" {
            format!("{}\n", formatted_content)
        } else {
            format!("\n{}", formatted_content)
        };

        // Build result string
        let mut result = String::new();
        result.push_str(&content[..insertion_byte]);
        result.push_str(&insertion_text);
        result.push_str(&content[insertion_byte..]);

        // Write back if not dry run
        if !self.dry_run {
            use crate::tools::editing::EditingTransaction;
            let tx = EditingTransaction::begin(file_path)?;
            tx.commit(&result)?;
        }

        let lines_inserted = formatted_content.lines().count();

        let message = if self.dry_run {
            format!(
                "DRY RUN: Would insert {} '{}' in {}\n\n\
                Inserting {} lines",
                position, target_symbol, file_path, lines_inserted
            )
        } else {
            format!(
                "✅ Successfully inserted {} '{}' in {}\n\n\
                Inserted {} lines",
                position, target_symbol, file_path, lines_inserted
            )
        };

        self.create_result(
            "insert_relative_to_symbol",
            true,
            vec![file_path.to_string()],
            1,
            vec!["Review the inserted content".to_string()],
            message,
            Some(serde_json::json!({
                "file": file_path,
                "symbol": target_symbol,
                "position": position,
                "lines_inserted": lines_inserted,
            })),
        )
    }

    /// Find any symbol (function, class, variable, etc.) by name
    /// ONLY matches DEFINITIONS/DECLARATIONS, not usage sites
    ///
    /// ## Behavior
    /// - Uses depth-first traversal, returning the FIRST match found
    /// - In practice, this means TOP-LEVEL symbols are found first
    /// - Works correctly for the vast majority of refactoring scenarios
    ///
    /// ## Edge Cases
    /// - If a nested symbol has the same name as a top-level symbol,
    ///   the top-level symbol will be matched (usually desired behavior)
    /// - To target a nested symbol with a conflicting name, the user
    ///   would need to rename it first or use file-scoped refactoring
    ///
    /// ## Test Coverage
    /// - test_extract_outer_function_with_nested_same_name: Validates top-level precedence
    /// - test_extract_specifies_ambiguous_symbol: Validates first-match behavior
    fn find_any_symbol<'a>(
        &self,
        node: tree_sitter::Node<'a>,
        name: &str,
        content: &str,
    ) -> Result<tree_sitter::Node<'a>> {
        // Only match declaration/definition nodes, not calls/references
        let is_declaration = matches!(
            node.kind(),
            "function_declaration"
                | "function_definition"
                | "method_declaration"
                | "method_definition"
                | "function_item"  // Rust function
                | "class_declaration"
                | "class_definition"
                | "struct_item"    // Rust struct
                | "enum_item"      // Rust enum
                | "impl_item"      // Rust impl
                | "lexical_declaration" // JS/TS const/let/var
                | "variable_declaration"
        );

        // Check if this declaration node has a name that matches
        if is_declaration {
            if let Some(name_node) = node.child_by_field_name("name") {
                if let Ok(node_name) = name_node.utf8_text(content.as_bytes()) {
                    if node_name == name {
                        return Ok(node);
                    }
                }
            }
        }

        // Recursively search children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Ok(found) = self.find_any_symbol(child, name, content) {
                return Ok(found);
            }
        }

        Err(anyhow!("Symbol '{}' not found", name))
    }
}
