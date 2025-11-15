// R Language Extractor Implementation
// R is a statistical computing and graphics language
// Tree-sitter-r parser provides AST nodes for R syntax

mod identifiers;
mod relationships;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use tree_sitter::Tree;

pub struct RExtractor {
    base: BaseExtractor,
    symbols: Vec<Symbol>,
}

impl RExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
            symbols: Vec::new(),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let root_node = tree.root_node();
        self.symbols.clear();

        // Start recursive traversal from root
        self.traverse_node(root_node, None);

        self.symbols.clone()
    }

    /// Recursively traverse the R AST and extract symbols
    fn traverse_node(&mut self, node: tree_sitter::Node, parent_id: Option<String>) {
        use crate::extractors::base::{SymbolKind, SymbolOptions};

        let mut current_symbol: Option<Symbol> = None;

        match node.kind() {
            // R assignments: x <- 42, y = 100, getUserData <- function(...), 200 -> z
            "binary_operator" => {
                // Get operator to determine assignment direction
                if let Some(operator) = node.child(1) {
                    let op_text = self.base.get_node_text(&operator);

                    // Left-to-right assignment: x <- value, x = value, x <<- value
                    if op_text == "<-" || op_text == "=" || op_text == "<<-" {
                        if let Some(left_child) = node.child(0) {
                            if left_child.kind() == "identifier" {
                                let name = self.base.get_node_text(&left_child);

                                // Check if right side is a function definition
                                let symbol_kind = if let Some(right_child) = node.child(2) {
                                    if right_child.kind() == "function_definition" {
                                        SymbolKind::Function
                                    } else {
                                        SymbolKind::Variable
                                    }
                                } else {
                                    SymbolKind::Variable
                                };

                                let options = SymbolOptions {
                                    parent_id: parent_id.clone(),
                                    ..Default::default()
                                };
                                let symbol =
                                    self.base.create_symbol(&node, name, symbol_kind, options);
                                self.symbols.push(symbol.clone());
                                current_symbol = Some(symbol);
                            }
                        }
                    }
                    // Right-to-left assignment: value -> x, value ->> x
                    else if op_text == "->" || op_text == "->>" {
                        if let Some(right_child) = node.child(2) {
                            if right_child.kind() == "identifier" {
                                let name = self.base.get_node_text(&right_child);

                                let options = SymbolOptions {
                                    parent_id: parent_id.clone(),
                                    ..Default::default()
                                };
                                let symbol = self.base.create_symbol(
                                    &node,
                                    name,
                                    SymbolKind::Variable,
                                    options,
                                );
                                self.symbols.push(symbol.clone());
                                current_symbol = Some(symbol);
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // Recursively traverse children
        let next_parent_id = current_symbol.as_ref().map(|s| s.id.clone()).or(parent_id);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_node(child, next_parent_id.clone());
        }
    }

    pub fn extract_relationships(
        &self,
        tree: &Tree,
        symbols: &[Symbol],
    ) -> Vec<Relationship> {
        relationships::extract_relationships(self, tree, symbols)
    }

    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(self, tree, symbols)
    }
}
