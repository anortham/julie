//! JavaScript Extractor for Julie
//!
//! Direct port of Miller's JavaScript extractor logic ported to idiomatic Rust
//! Original: /Users/murphy/Source/miller/src/extractors/javascript-extractor.ts
//!
//! This follows the exact extraction strategy from Miller while using Rust patterns:
//! - Uses Miller's node type switch statement logic
//! - Preserves Miller's signature building algorithms
//! - Maintains Miller's same edge case handling
//! - Converts to Rust Option<T>, Result<T>, iterators, ownership system

mod assignments;
mod functions;
mod helpers;
mod identifiers;
mod imports;
mod signatures;
mod types;
mod variables;
mod visibility;

use crate::extractors::base::{BaseExtractor, Relationship, Symbol};
use tree_sitter::Tree;

pub struct JavaScriptExtractor {
    base: BaseExtractor,
}

impl JavaScriptExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_node_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    /// Main tree traversal - ports Miller's visitNode function exactly
    fn visit_node(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let mut symbol: Option<Symbol> = None;

        // Port Miller's switch statement exactly
        match node.kind() {
            "class_declaration" => {
                symbol = Some(self.extract_class(node, parent_id.clone()));
            }
            "function_declaration"
            | "function"
            | "arrow_function"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration" => {
                symbol = Some(self.extract_function(node, parent_id.clone()));
            }
            "method_definition" => {
                symbol = Some(self.extract_method(node, parent_id.clone()));
            }
            "variable_declarator" => {
                // Handle destructuring patterns that create multiple symbols (Miller's logic)
                let name_node = node.child_by_field_name("name");
                if let Some(name) = name_node {
                    if name.kind() == "object_pattern" || name.kind() == "array_pattern" {
                        let destructured_symbols =
                            self.extract_destructuring_variables(node, parent_id.clone());
                        symbols.extend(destructured_symbols);
                    } else {
                        symbol = Some(self.extract_variable(node, parent_id.clone()));
                    }
                } else {
                    symbol = Some(self.extract_variable(node, parent_id.clone()));
                }
            }
            "import_statement" | "import_declaration" => {
                // Handle multiple import specifiers (Miller's logic)
                let import_symbols = self.extract_import_specifiers(&node);
                for specifier in import_symbols {
                    let import_symbol =
                        self.create_import_symbol(node, &specifier, parent_id.clone());
                    symbols.push(import_symbol);
                }
            }
            "export_statement" | "export_declaration" => {
                symbol = Some(self.extract_export(node, parent_id.clone()));
            }
            "property_definition" | "public_field_definition" | "field_definition" | "pair" => {
                symbol = Some(self.extract_property(node, parent_id.clone()));
            }
            "assignment_expression" => {
                if let Some(assignment_symbol) = self.extract_assignment(node, parent_id.clone()) {
                    symbol = Some(assignment_symbol);
                }
            }
            _ => {}
        }

        let current_parent_id = if let Some(sym) = &symbol {
            symbols.push(sym.clone());
            Some(sym.id.clone())
        } else {
            parent_id
        };

        // Recursively visit children (Miller's pattern)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    /// Visit node for relationships - placeholder for relationship extraction
    #[allow(clippy::only_used_in_recursion)] // &self, symbols, relationships all used in recursive calls
    fn visit_node_for_relationships(
        &self,
        node: tree_sitter::Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        // TODO: Implement relationship extraction following Miller's extractRelationships method
        // This is a placeholder to make the interface complete

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }
}
