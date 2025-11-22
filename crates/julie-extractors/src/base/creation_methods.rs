// Symbol, Identifier, Relationship, and Visibility creation methods
//
// Extracted from extractor.rs to keep modules under 500 lines

use std::collections::HashMap;
use tree_sitter::Node;

use super::extractor::BaseExtractor;
use super::types::IdentifierKind;
use super::types::{
    Identifier, Relationship, RelationshipKind, Symbol, SymbolKind, SymbolOptions, Visibility,
};

impl BaseExtractor {
    /// Create a symbol - exact port of createSymbol method
    pub fn create_symbol(
        &mut self,
        node: &Node,
        name: String,
        kind: SymbolKind,
        options: SymbolOptions,
    ) -> Symbol {
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        let id = self.generate_id(&name, start_pos.row as u32, start_pos.column as u32);

        // Extract code context around the symbol
        let code_context = self.extract_code_context(start_pos.row, end_pos.row);

        // Mark markdown symbols as documentation
        let content_type = if self.language == "markdown" {
            Some("documentation".to_string())
        } else {
            None
        };

        let symbol = Symbol {
            id: id.clone(),
            name,
            kind,
            language: self.language.clone(),
            file_path: self.file_path.clone(),
            start_line: (start_pos.row + 1) as u32, // Uses 1-based line numbers
            start_column: start_pos.column as u32,  // Uses 0-based column numbers
            end_line: (end_pos.row + 1) as u32,
            end_column: end_pos.column as u32,
            start_byte: node.start_byte() as u32,
            end_byte: node.end_byte() as u32,
            signature: options.signature,
            doc_comment: options.doc_comment.or_else(|| self.find_doc_comment(node)),
            visibility: options.visibility,
            parent_id: options.parent_id,
            metadata: Some(options.metadata.unwrap_or_default()),
            semantic_group: None, // Will be populated during cross-language analysis
            confidence: None,     // Will be calculated based on parsing context
            code_context,
            content_type,
        };

        self.symbol_map.insert(id, symbol.clone());
        symbol
    }

    /// Create an identifier (reference/usage) - NEW for LSP-quality reference tracking
    ///
    /// Unlike symbols (definitions), identifiers represent usage sites.
    /// They are stored unresolved (target_symbol_id = None) and resolved on-demand
    /// during queries for optimal incremental update performance.
    pub fn create_identifier(
        &mut self,
        node: &Node,
        name: String,
        kind: IdentifierKind,
        containing_symbol_id: Option<String>,
    ) -> Identifier {
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        // Generate unique ID for this identifier
        let id = self.generate_id(&name, start_pos.row as u32, start_pos.column as u32);

        // Extract code context around the identifier (lighter context for identifiers)
        let code_context = self.extract_code_context(start_pos.row, end_pos.row);

        let identifier = Identifier {
            id,
            name,
            kind,
            language: self.language.clone(),
            file_path: self.file_path.clone(),
            start_line: (start_pos.row + 1) as u32, // 1-based line numbers
            start_column: start_pos.column as u32,  // 0-based column numbers
            end_line: (end_pos.row + 1) as u32,
            end_column: end_pos.column as u32,
            start_byte: node.start_byte() as u32,
            end_byte: node.end_byte() as u32,
            containing_symbol_id,
            target_symbol_id: None, // Unresolved - will be resolved on-demand in C#
            confidence: 1.0,        // Default high confidence for tree-sitter extractions
            code_context,
        };

        self.identifiers.push(identifier.clone());
        identifier
    }

    /// Create a relationship - exact port of createRelationship
    pub fn create_relationship(
        &self,
        from_symbol_id: String,
        to_symbol_id: String,
        kind: RelationshipKind,
        node: &Node,
        confidence: Option<f32>,
        metadata: Option<HashMap<String, serde_json::Value>>,
    ) -> Relationship {
        Relationship {
            id: format!(
                "{}_{}_{:?}_{}",
                from_symbol_id,
                to_symbol_id,
                kind,
                node.start_position().row
            ),
            from_symbol_id,
            to_symbol_id,
            kind,
            file_path: self.file_path.clone(),
            line_number: (node.start_position().row + 1) as u32, // 1-based standard format
            confidence: confidence.unwrap_or(1.0),
            metadata,
        }
    }

    /// Find containing symbol - exact port of findContainingSymbol
    pub fn find_containing_symbol<'a>(
        &self,
        node: &Node,
        symbols: &'a [Symbol],
    ) -> Option<&'a Symbol> {
        let position = node.start_position();

        // Find symbols that contain this position
        let mut containing_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| {
                let pos_line = (position.row + 1) as u32;
                let pos_column = position.column as u32;

                let line_contains = s.start_line <= pos_line && s.end_line >= pos_line;

                // For column containment, handle multi-line spans exactly standard format
                let col_contains = if pos_line == s.start_line && pos_line == s.end_line {
                    // Single line span
                    s.start_column <= pos_column && s.end_column >= pos_column
                } else if pos_line == s.start_line {
                    // First line of multi-line span
                    s.start_column <= pos_column
                } else if pos_line == s.end_line {
                    // Last line of multi-line span
                    s.end_column >= pos_column
                } else {
                    // Middle line of multi-line span
                    true
                };

                line_contains && col_contains
            })
            .collect();

        if containing_symbols.is_empty() {
            return None;
        }

        // Priority order - reference implementation
        let get_priority = |kind: &SymbolKind| -> u32 {
            match kind {
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => 1,
                SymbolKind::Class | SymbolKind::Interface => 2,
                SymbolKind::Namespace => 3,
                SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Property => 10,
                _ => 5,
            }
        };

        containing_symbols.sort_by(|a, b| {
            // First, sort by priority (functions first)
            let priority_a = get_priority(&a.kind);
            let priority_b = get_priority(&b.kind);
            if priority_a != priority_b {
                return priority_a.cmp(&priority_b);
            }

            // Then by size (smaller first) - reference calculation
            let size_a = (a.end_line - a.start_line) * 1000 + (a.end_column - a.start_column);
            let size_b = (b.end_line - b.start_line) * 1000 + (b.end_column - b.start_column);
            size_a.cmp(&size_b)
        });

        Some(containing_symbols[0])
    }

    /// Extract visibility - exact port of extractVisibility
    pub fn extract_visibility(&self, node: &Node) -> Option<Visibility> {
        // Look for visibility modifiers in child nodes
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                match child.kind() {
                    "public" => return Some(Visibility::Public),
                    "private" => return Some(Visibility::Private),
                    "protected" => return Some(Visibility::Protected),
                    _ => continue,
                }
            }
        }

        // Check for language-specific patterns in text
        let text = self.get_node_text(node);
        if text.contains("public ") {
            Some(Visibility::Public)
        } else if text.contains("private ") {
            Some(Visibility::Private)
        } else if text.contains("protected ") {
            Some(Visibility::Protected)
        } else {
            None
        }
    }
}
