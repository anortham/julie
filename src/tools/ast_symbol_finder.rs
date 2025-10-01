//! AST-Aware Symbol Finding using Tree-sitter
//!
//! This module provides intelligent symbol finding that uses tree-sitter AST
//! instead of regex patterns. This is the CORE of Julie's value proposition -
//! language-aware code intelligence across 26+ languages.
//!
//! ## Why This Matters
//!
//! **Regex approach (WRONG):**
//! - Matches in string literals: `"UserService"` gets renamed ❌
//! - Matches in comments: `// UserService here` gets renamed ❌
//! - No scope awareness: Renames unrelated symbols ❌
//!
//! **AST approach (CORRECT):**
//! - Only matches actual code symbols ✅
//! - Skips strings and comments ✅
//! - Language-aware: TypeScript ≠ Python ≅ Rust ✅
//! - Scope-aware: Can limit to specific contexts ✅

use tree_sitter::{Node, Tree};
use tracing::debug;

use crate::extractors::base::SymbolKind;

/// Context in which a symbol appears
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolContext {
    /// Symbol definition (class, function, variable declaration)
    Definition,
    /// Symbol usage/reference
    Usage,
    /// Inside import statement
    Import,
    /// Inside export statement
    Export,
    /// Inside string literal (SKIP during rename!)
    StringLiteral,
    /// Inside comment (SKIP during rename!)
    Comment,
    /// Type annotation/signature
    TypeAnnotation,
}

/// A single occurrence of a symbol in the AST
#[derive(Debug, Clone)]
pub struct SymbolOccurrence {
    /// Byte offset where symbol starts
    pub start_byte: usize,
    /// Byte offset where symbol ends
    pub end_byte: usize,
    /// Tree-sitter node kind (e.g., "identifier", "type_identifier")
    pub node_kind: String,
    /// Context in which this symbol appears
    pub context: SymbolContext,
    /// Line number (1-based)
    pub line: usize,
    /// Column number (0-based)
    pub column: usize,
}

/// A symbol definition with its body/scope
#[derive(Debug, Clone)]
pub struct SymbolDefinition {
    /// Byte range of just the symbol name
    pub name_range: (usize, usize),
    /// Byte range of the entire body/block
    pub body_range: (usize, usize),
    /// Kind of symbol (Class, Function, etc.)
    pub kind: SymbolKind,
    /// Line where definition starts (1-based)
    pub line: usize,
    /// Line where definition ends (1-based)
    pub end_line: usize,
}

/// Scope/range information
#[derive(Debug, Clone)]
pub struct ScopeRange {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
}

/// AST-based symbol finder using tree-sitter
pub struct ASTSymbolFinder {
    content: String,
    tree: Tree,
    language: String,
}

impl ASTSymbolFinder {
    /// Create a new AST symbol finder
    pub fn new(content: String, tree: Tree, language: String) -> Self {
        Self {
            content,
            tree,
            language,
        }
    }

    /// Find all occurrences of a symbol by name
    ///
    /// This is the CORE method that makes Julie different from regex-based tools.
    /// It walks the AST and finds only actual symbol nodes, skipping strings/comments.
    pub fn find_symbol_occurrences(&self, symbol_name: &str) -> Vec<SymbolOccurrence> {
        let mut occurrences = Vec::new();
        self.walk_node_for_symbol(self.tree.root_node(), symbol_name, &mut occurrences);

        debug!(
            "🔍 Found {} occurrences of '{}' using AST (language: {})",
            occurrences.len(),
            symbol_name,
            self.language
        );

        occurrences
    }

    /// Recursively walk AST looking for symbol occurrences
    fn walk_node_for_symbol(
        &self,
        node: Node,
        symbol_name: &str,
        occurrences: &mut Vec<SymbolOccurrence>,
    ) {
        // Check if this node is a symbol occurrence
        if self.is_symbol_node(node) {
            if let Some(text) = self.node_text(node) {
                if text == symbol_name {
                    let context = self.determine_context(node);

                    occurrences.push(SymbolOccurrence {
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        node_kind: node.kind().to_string(),
                        context,
                        line: node.start_position().row + 1,
                        column: node.start_position().column,
                    });
                }
            }
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_node_for_symbol(child, symbol_name, occurrences);
        }
    }

    /// Check if node represents a symbol (identifier, type, etc.)
    fn is_symbol_node(&self, node: Node) -> bool {
        matches!(
            node.kind(),
            "identifier"
                | "type_identifier"
                | "property_identifier"
                | "field_identifier"
                | "variable_name"
                | "function_name"
                | "class_name"
                | "method_name"
        )
    }

    /// Get text content of a node
    fn node_text(&self, node: Node) -> Option<String> {
        node.utf8_text(self.content.as_bytes()).ok().map(|s| s.to_string())
    }

    /// Determine the context in which a symbol appears
    fn determine_context(&self, node: Node) -> SymbolContext {
        // Walk up the tree to find context
        let mut current = node;

        loop {
            if let Some(parent) = current.parent() {
                match parent.kind() {
                    // String literals - SKIP during rename!
                    "string" | "string_literal" | "template_string" | "string_content" => {
                        return SymbolContext::StringLiteral;
                    }

                    // Comments - SKIP during rename (usually)
                    "comment" | "line_comment" | "block_comment" => {
                        return SymbolContext::Comment;
                    }

                    // Definitions
                    "class_declaration"
                    | "function_declaration"
                    | "variable_declarator"
                    | "method_definition" => {
                        return SymbolContext::Definition;
                    }

                    // Type annotations
                    "type_annotation" | "type_arguments" => {
                        return SymbolContext::TypeAnnotation;
                    }

                    // Import/Export
                    "import_statement" | "import_specifier" => {
                        return SymbolContext::Import;
                    }
                    "export_statement" => {
                        return SymbolContext::Export;
                    }

                    _ => {
                        // Keep walking up
                        current = parent;
                    }
                }
            } else {
                // Reached root - default to usage
                return SymbolContext::Usage;
            }
        }
    }

    /// Find the definition of a symbol (class, function, etc.)
    pub fn find_symbol_definition(&self, symbol_name: &str) -> Option<SymbolDefinition> {
        self.find_definition_recursive(self.tree.root_node(), symbol_name)
    }

    fn find_definition_recursive(&self, node: Node, symbol_name: &str) -> Option<SymbolDefinition> {
        // Check if this node is a definition with matching name
        if let Some(def) = self.check_if_definition(node, symbol_name) {
            return Some(def);
        }

        // Recursively search children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(def) = self.find_definition_recursive(child, symbol_name) {
                return Some(def);
            }
        }

        None
    }

    fn check_if_definition(&self, node: Node, symbol_name: &str) -> Option<SymbolDefinition> {
        match node.kind() {
            "class_declaration" | "function_declaration" | "method_definition" => {
                // Find the name child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if self.is_symbol_node(child) {
                        if let Some(name) = self.node_text(child) {
                            if name == symbol_name {
                                return Some(SymbolDefinition {
                                    name_range: (child.start_byte(), child.end_byte()),
                                    body_range: (node.start_byte(), node.end_byte()),
                                    kind: self.node_kind_to_symbol_kind(node.kind()),
                                    line: node.start_position().row + 1,
                                    end_line: node.end_position().row + 1,
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn node_kind_to_symbol_kind(&self, kind: &str) -> SymbolKind {
        match kind {
            "class_declaration" => SymbolKind::Class,
            "function_declaration" => SymbolKind::Function,
            "method_definition" => SymbolKind::Method,
            "variable_declarator" => SymbolKind::Variable,
            _ => SymbolKind::Variable, // Default to Variable for unknown types
        }
    }

    /// Find scope boundaries for a symbol
    pub fn find_symbol_scope(&self, symbol_name: &str) -> Option<ScopeRange> {
        if let Some(def) = self.find_symbol_definition(symbol_name) {
            Some(ScopeRange {
                start_byte: def.body_range.0,
                end_byte: def.body_range.1,
                start_line: def.line,
                end_line: def.end_line,
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[test]
    fn test_find_symbol_occurrences_typescript() {
        let code = r#"
class UserService {
    getUserData() {
        return "UserService"; // String literal - should be StringLiteral context!
    }
}

// UserService comment - should be Comment context!
const service = new UserService();
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();

        let finder = ASTSymbolFinder::new(code.to_string(), tree, "typescript".to_string());
        let occurrences = finder.find_symbol_occurrences("UserService");

        // Should find: class definition, type usage, but NOT string literal or comment
        assert!(
            occurrences.len() >= 2,
            "Should find at least 2 occurrences (definition + usage)"
        );

        // Check that string literal is marked correctly
        let string_contexts: Vec<_> = occurrences
            .iter()
            .filter(|occ| occ.context == SymbolContext::StringLiteral)
            .collect();

        assert_eq!(
            string_contexts.len(),
            0,
            "String literals should not be found as symbol occurrences (they're string nodes, not identifiers)"
        );

        // Check that we found the class definition
        let definitions: Vec<_> = occurrences
            .iter()
            .filter(|occ| occ.context == SymbolContext::Definition)
            .collect();

        assert!(
            !definitions.is_empty(),
            "Should find the class definition"
        );
    }
}
