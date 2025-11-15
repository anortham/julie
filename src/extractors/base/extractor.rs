// BaseExtractor implementation for Julie
//
// Lines 399-1090 from original base.rs
// Contains the BaseExtractor struct and all its methods

use md5;
use std::collections::HashMap;
use tracing::{debug, warn};
use tree_sitter::Node;

use super::types::{ContextConfig, Identifier, Relationship, Symbol, TypeInfo};

/// Base implementation for language extractors
///
/// Implementation of BaseExtractor class with all utility methods
pub struct BaseExtractor {
    pub language: String,
    pub file_path: String,
    pub content: String,
    pub symbol_map: HashMap<String, Symbol>,
    pub relationships: Vec<Relationship>,
    pub type_info: HashMap<String, TypeInfo>,
    pub identifiers: Vec<Identifier>, // NEW: Reference extraction for LSP-quality tools
    pub context_config: ContextConfig,
}

impl BaseExtractor {
    /// Create new abstract extractor - port of constructor
    ///
    /// # Phase 2: Relative Unix-Style Path Storage
    /// Now accepts workspace_root to convert absolute paths to relative Unix-style paths
    /// for token-efficient storage (7-12% savings per search result).
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        // CRITICAL FIX: Canonicalize file_path to resolve symlinks (macOS /var vs /private/var)
        // This ensures database queries match during get_symbols (which also canonicalizes)
        // Without this: indexing stores /var/..., queries use /private/var/... → zero results
        //
        // 🔥 BUG FIX: Handle relative paths correctly
        // If file_path is relative (e.g., "COA.CodeSearch.McpServer/Services/FileIndexingService.cs"),
        // we must join it to workspace_root BEFORE canonicalizing.
        // canonicalize() only works with absolute paths or CWD-relative paths.
        let path_to_canonicalize = if std::path::Path::new(&file_path).is_absolute() {
            std::path::PathBuf::from(&file_path)
        } else {
            workspace_root.join(&file_path)
        };

        let canonical_path = path_to_canonicalize.canonicalize().unwrap_or_else(|e| {
            warn!(
                "⚠️  Failed to canonicalize path '{}': {} - using original",
                file_path, e
            );
            std::path::PathBuf::from(&file_path)
        });

        // Phase 2: Convert absolute path to relative Unix-style path for storage
        // File paths might be absolute OR relative - handle both
        let relative_unix_path = if canonical_path.is_absolute() {
            crate::utils::paths::to_relative_unix_style(
                &canonical_path,
                workspace_root,
            )
            .unwrap_or_else(|e| {
                warn!(
                    "⚠️  Failed to convert to relative path '{}': {} - using absolute as fallback",
                    canonical_path.display(),
                    e
                );
                canonical_path.to_string_lossy().to_string()
            })
        } else {
            // Already relative - use as-is (just normalize to Unix-style)
            canonical_path.to_string_lossy().replace('\\', "/")
        };

        debug!(
            "BaseExtractor path: '{}' -> '{}' (relative)",
            file_path, relative_unix_path
        );

        Self {
            language,
            file_path: relative_unix_path, // Phase 2: Store relative Unix-style path
            content,
            symbol_map: HashMap::new(),
            relationships: Vec::new(),
            type_info: HashMap::new(),
            identifiers: Vec::new(), // NEW: Initialize empty identifier list
            context_config: ContextConfig::default(),
        }
    }

    /// Get text from a tree-sitter node - exact port of getNodeText
    pub fn get_node_text(&self, node: &Node) -> String {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        // Use byte slice but handle UTF-8 boundaries properly
        let content_bytes = self.content.as_bytes();
        if start_byte < content_bytes.len() && end_byte <= content_bytes.len() {
            String::from_utf8_lossy(&content_bytes[start_byte..end_byte]).to_string()
        } else {
            String::new()
        }
    }

    /// Find documentation comment for a node - exact port of findDocComment
    pub fn find_doc_comment(&self, node: &Node) -> Option<String> {
        let mut comments = Vec::new();

        // Helper function to check if text is a doc comment
        let is_doc_comment = |text: &str| {
            let trimmed = text.trim_start();
            let lang_lower = self.language.to_lowercase();
            let is_sql = lang_lower.contains("sql");
            let is_lua = lang_lower.contains("lua");
            let is_razor = lang_lower.contains("razor");

            trimmed.starts_with("/**")
                || trimmed.starts_with("/*") // CSS/HTML/SQL block comments
                || trimmed.starts_with("<!--") // HTML comments
                || trimmed.starts_with("///")
                || trimmed.starts_with("##") // Python docstrings
                || trimmed.starts_with("//") // Go style comments
                || trimmed.starts_with("---") // Lua LuaDoc
                || trimmed.starts_with("--[[") // Lua block comment
                || ((is_sql || is_lua) && trimmed.starts_with("--")) // SQL/Lua dash comments
                || trimmed.starts_with("#") // Ruby RDoc/YARD comments
                || (is_razor && trimmed.starts_with("@*")) // Razor doc comments
        };

        // First try to find comments as siblings of this node
        let mut current = node.prev_named_sibling();
        while let Some(sibling) = current {
            if sibling.kind().contains("comment") || sibling.kind() == "marginalia" {
                let comment_text = self.get_node_text(&sibling);
                if is_doc_comment(&comment_text) {
                    comments.push(comment_text);
                    current = sibling.prev_named_sibling();
                } else {
                    // Stop at non-doc comment
                    break;
                }
            } else {
                // Stop at non-comment node
                break;
            }
        }

        // If no comments found as direct siblings, try looking at ancestor siblings
        // (useful for SQL where comment is sibling of statement, not create_table inside,
        // or Dart where comment is sibling of class_member_definition, not getter_signature)
        if comments.is_empty() {
            let mut current_node = *node;
            for _ in 0..3 {
                // Try up to 3 ancestor levels
                if let Some(parent) = current_node.parent() {
                    current = parent.prev_named_sibling();
                    while let Some(sibling) = current {
                        if sibling.kind().contains("comment") || sibling.kind() == "marginalia" {
                            let comment_text = self.get_node_text(&sibling);
                            if is_doc_comment(&comment_text) {
                                comments.push(comment_text);
                                current = sibling.prev_named_sibling();
                            } else {
                                // Stop at non-doc comment
                                break;
                            }
                        } else {
                            // Stop at non-comment node
                            break;
                        }
                    }
                    if !comments.is_empty() {
                        break;
                    }
                    current_node = parent;
                } else {
                    break;
                }
            }
        }

        // For certain nodes (like cte), also check for comments as children (e.g., inside parentheses)
        if comments.is_empty() && (node.kind() == "cte") {
            // Look for first comments among direct children
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind().contains("comment") || child.kind() == "marginalia" {
                    let comment_text = self.get_node_text(&child);
                    if is_doc_comment(&comment_text) {
                        comments.push(comment_text);
                    }
                }
            }
        }

        if comments.is_empty() {
            None
        } else {
            // Reverse to get original order (top to bottom)
            comments.reverse();
            Some(comments.join("\n"))
        }
    }

    /// Generate ID for a symbol - exact port of generateId (MD5 hash)
    pub fn generate_id(&self, name: &str, line: u32, column: u32) -> String {
        let input = format!("{}:{}:{}:{}", self.file_path, name, line, column);
        let digest = md5::compute(input.as_bytes());
        format!("{:x}", digest)
    }

    /// Extract code context around a symbol using configurable parameters
    /// Inspired by codesearch's LineAwareSearchService context extraction
    pub(crate) fn extract_code_context(&self, start_row: usize, end_row: usize) -> Option<String> {
        if self.content.is_empty() {
            return None;
        }

        let lines: Vec<&str> = self.content.lines().collect();

        if lines.is_empty() || start_row >= lines.len() {
            return None;
        }

        // Calculate context bounds using configuration
        let context_start = start_row.saturating_sub(self.context_config.lines_before);
        let context_end = std::cmp::min(lines.len() - 1, end_row + self.context_config.lines_after);

        // Build context with optional line numbers
        let mut context_lines = Vec::new();
        for i in context_start..=context_end {
            let line_num = i + 1; // 1-based line numbers
            let mut line_content = lines.get(i).unwrap_or(&"").to_string();

            // Truncate long lines if configured (respecting UTF-8 boundaries)
            if line_content.len() > self.context_config.max_line_length {
                // Find a valid UTF-8 boundary near the target length
                let mut truncate_len = self.context_config.max_line_length.saturating_sub(3);
                while truncate_len > 0 && !line_content.is_char_boundary(truncate_len) {
                    truncate_len -= 1;
                }
                line_content.truncate(truncate_len);
                line_content.push_str("...");
            }

            // Format line with optional line numbers
            let formatted_line = if self.context_config.show_line_numbers {
                if i >= start_row && i <= end_row {
                    format!("  ➤ {:3}: {}", line_num, line_content)
                } else {
                    format!("    {:3}: {}", line_num, line_content)
                }
            } else if i >= start_row && i <= end_row {
                format!("  ➤ {}", line_content)
            } else {
                format!("    {}", line_content)
            };

            context_lines.push(formatted_line);
        }

        Some(context_lines.join("\n"))
    }

    /// Update the context configuration
    pub fn set_context_config(&mut self, config: ContextConfig) {
        self.context_config = config;
    }

    /// Get a reference to the current context configuration
    pub fn get_context_config(&self) -> &ContextConfig {
        &self.context_config
    }

    /// Create a symbol - exact port of createSymbol method

    /// Extract identifier name - exact port of extractIdentifierName
    pub fn extract_identifier_name(&self, node: &Node) -> String {
        // Try to find the identifier node using field name
        if let Some(name_node) = node.child_by_field_name("name") {
            if name_node.kind() == "identifier" {
                return self.get_node_text(&name_node);
            }
        }

        // Try first child
        if let Some(first_child) = node.child(0) {
            if first_child.kind() == "identifier" {
                return self.get_node_text(&first_child);
            }
        }

        // Fallback: extract from the node text using regex (standard pattern)
        let node_text = self.get_node_text(node);
        let text = node_text.trim();
        if let Some(captures) = regex::Regex::new(r"^[a-zA-Z_$][a-zA-Z0-9_$]*")
            .unwrap()
            .find(text)
        {
            captures.as_str().to_string()
        } else {
            "Anonymous".to_string()
        }
    }

    /// Safely truncate a string to a maximum number of characters (not bytes)
    /// This handles UTF-8 multi-byte characters correctly by truncating at character boundaries
    pub fn truncate_string(text: &str, max_chars: usize) -> String {
        let char_count = text.chars().count();
        if char_count <= max_chars {
            text.to_string()
        } else {
            text.chars().take(max_chars).collect::<String>() + "..."
        }
    }
}
