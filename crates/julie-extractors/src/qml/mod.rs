// QML (Qt Modeling Language) Extractor Implementation
// QML is JavaScript-based declarative UI language for Qt applications
// Tree-sitter-qmljs extends TypeScript grammar with QML-specific nodes

mod identifiers;
mod relationships;

use crate::base::{BaseExtractor, Identifier, PendingRelationship, Relationship, Symbol};
use tree_sitter::Tree;

pub struct QmlExtractor {
    base: BaseExtractor,
    symbols: Vec<Symbol>,
    /// Pending relationships that need cross-file resolution after workspace indexing
    pending_relationships: Vec<PendingRelationship>,
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
            pending_relationships: Vec::new(),
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
        use crate::base::{SymbolKind, SymbolOptions};

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
        &mut self,
        tree: &Tree,
        symbols: &[Symbol],
    ) -> Vec<Relationship> {
        let rels = relationships::extract_relationships(self, tree, symbols);
        // Extract pending relationships (cross-file calls) and add them to our internal list
        self.extract_pending_relationships(tree, symbols);
        rels
    }

    /// Extract pending relationships from the syntax tree
    /// This handles cross-file function calls that need resolution
    fn extract_pending_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) {
        let symbol_map: std::collections::HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.name.clone(), s)).collect();

        self.walk_for_pending_calls(tree.root_node(), &symbol_map);
    }

    /// Walk the tree looking for function calls that are not in the local symbol map
    fn walk_for_pending_calls(&mut self, node: tree_sitter::Node, symbol_map: &std::collections::HashMap<String, &Symbol>) {
        // Look for call expressions
        if node.kind() == "call_expression" {
            if let Some(function_node) = node.child_by_field_name("function") {
                // Extract function name - handle both direct calls and member access
                let function_name = if function_node.kind() == "member_expression" {
                    // For obj.method(), get just "method"
                    if let Some(property) = function_node.child_by_field_name("property") {
                        self.base.get_node_text(&property)
                    } else {
                        self.base.get_node_text(&function_node)
                    }
                } else {
                    self.base.get_node_text(&function_node)
                };

                // Check if this is a call to a function not in our symbol map
                match symbol_map.get(function_name.as_str()) {
                    None => {
                        // Unknown function - could be from another file
                        // Check if it's being called from within a function
                        if let Some(caller_symbol) = self.find_containing_function_in_symbols(node, symbol_map) {
                            let line_number = node.start_position().row as u32 + 1;
                            self.add_pending_relationship(PendingRelationship {
                                from_symbol_id: caller_symbol.id.clone(),
                                callee_name: function_name.clone(),
                                kind: crate::base::RelationshipKind::Calls,
                                file_path: self.base.file_path.clone(),
                                line_number,
                                confidence: 0.7,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        // Recursively process children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_for_pending_calls(child, symbol_map);
        }
    }

    /// Find the containing function for a node by walking up the tree
    fn find_containing_function_in_symbols<'a>(
        &self,
        node: tree_sitter::Node,
        symbol_map: &'a std::collections::HashMap<String, &'a Symbol>,
    ) -> Option<&'a Symbol> {
        let mut current = node.parent();

        while let Some(current_node) = current {
            // Check for function declarations
            if current_node.kind() == "function_declaration" {
                // Get the function name
                if let Some(name_node) = current_node.child_by_field_name("name") {
                    let func_name = self.base.get_node_text(&name_node);
                    if let Some(symbol) = symbol_map.get(&func_name) {
                        if matches!(symbol.kind, crate::base::SymbolKind::Function | crate::base::SymbolKind::Event) {
                            return Some(symbol);
                        }
                    }
                }
            }

            current = current_node.parent();
        }

        None
    }

    /// Get pending relationships that need cross-file resolution
    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.pending_relationships.clone()
    }

    /// Add a pending relationship (used during extraction)
    pub fn add_pending_relationship(&mut self, pending: PendingRelationship) {
        self.pending_relationships.push(pending);
    }

    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(self, tree, symbols)
    }
}
