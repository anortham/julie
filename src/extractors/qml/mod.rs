// QML (Qt Modeling Language) Extractor Implementation
// QML is JavaScript-based declarative UI language for Qt applications
// Tree-sitter-qmljs extends TypeScript grammar with QML-specific nodes

mod identifiers;
mod relationships;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use tree_sitter::Tree;

pub struct QmlExtractor {
    base: BaseExtractor,
    symbols: Vec<Symbol>,
}

impl QmlExtractor {
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

    /// Recursively traverse the QML AST and extract symbols
    fn traverse_node(&mut self, node: tree_sitter::Node, parent_id: Option<String>) {
        use crate::extractors::base::{SymbolKind, SymbolOptions};

        let mut current_symbol: Option<Symbol> = None;

        match node.kind() {
            // QML component definitions (Rectangle, Window, Button, etc.)
            "ui_object_definition" => {
                if let Some(type_name) = node.child_by_field_name("type_name") {
                    let name = self.base.get_node_text(&type_name);
                    let options = SymbolOptions {
                        parent_id: parent_id.clone(),
                        ..Default::default()
                    };
                    let symbol = self
                        .base
                        .create_symbol(&node, name, SymbolKind::Class, options);
                    self.symbols.push(symbol.clone());
                    current_symbol = Some(symbol);
                }
            }

            // QML properties (property int age: 42)
            "ui_property" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = self.base.get_node_text(&name_node);
                    let options = SymbolOptions {
                        parent_id: parent_id.clone(),
                        ..Default::default()
                    };
                    let symbol =
                        self.base
                            .create_symbol(&node, name, SymbolKind::Property, options);
                    self.symbols.push(symbol);
                }
            }

            // QML signals (signal clicked(x, y))
            "ui_signal" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = self.base.get_node_text(&name_node);
                    let options = SymbolOptions {
                        parent_id: parent_id.clone(),
                        ..Default::default()
                    };
                    let symbol = self
                        .base
                        .create_symbol(&node, name, SymbolKind::Event, options);
                    self.symbols.push(symbol);
                }
            }

            // JavaScript functions (inherited from TypeScript grammar)
            "function_declaration" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name = self.base.get_node_text(&name_node);
                    let options = SymbolOptions {
                        parent_id: parent_id.clone(),
                        ..Default::default()
                    };
                    let symbol =
                        self.base
                            .create_symbol(&node, name, SymbolKind::Function, options);
                    self.symbols.push(symbol);
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
