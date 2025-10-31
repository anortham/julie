/// Ruby language extractor with support for:
/// - Modules, classes, singleton classes
/// - Methods, singleton methods, initialize/constructor
/// - Variables, constants, aliases
/// - Assignments, parallel assignments, rest assignments
/// - Special calls: require, attr_accessor, define_method, def_delegator
/// - Relationships: inheritance, module inclusion
/// - Identifier extraction for LSP-quality find_references
///
/// Implementation of comprehensive Ruby extractor
use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol, Visibility};
use tree_sitter::{Node, Tree};

// Private modules - encapsulate implementation details
mod assignments;
mod calls;
mod helpers;
mod identifiers;
mod relationships;
mod signatures;
mod symbols;

/// Ruby extractor that handles Ruby-specific constructs
pub struct RubyExtractor {
    base: BaseExtractor,
    current_visibility: Visibility,
}

impl RubyExtractor {
    pub fn new(file_path: String, content: String, workspace_root: &std::path::Path) -> Self {
        Self {
            base: BaseExtractor::new("ruby".to_string(), file_path, content, workspace_root),
            current_visibility: Visibility::Public,
        }
    }

    /// Extract all symbols from Ruby source code
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.current_visibility = Visibility::Public; // Reset for each file

        // Clear any previous symbols from symbol_map
        self.base.symbol_map.clear();

        self.traverse_tree(tree.root_node(), &mut symbols);

        // Include additional symbols from symbol_map (parallel assignments, etc.)
        // BUT: Only add symbols that weren't already added during traversal
        // (create_symbol automatically adds to symbol_map, causing duplication)
        let existing_ids: std::collections::HashSet<_> =
            symbols.iter().map(|s| s.id.clone()).collect();

        for (id, symbol) in self.base.symbol_map.iter() {
            if !existing_ids.contains(id) {
                symbols.push(symbol.clone());
            }
        }

        symbols
    }

    /// Extract relationships between symbols (inheritance, module inclusion, etc.)
    pub fn extract_relationships(&self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(&self.base, tree, symbols)
    }

    /// Extract identifier usages for LSP-quality references
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(&mut self.base, tree, symbols)
    }

    // ========================================================================
    // Symbol Extraction - Tree Traversal
    // ========================================================================

    fn traverse_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>) {
        self.traverse_tree_with_parent(node, symbols, None);
    }

    fn traverse_tree_with_parent(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let mut symbol_opt: Option<Symbol> = None;

        match node.kind() {
            "module" => {
                symbol_opt = Some(symbols::extract_module(
                    &mut self.base,
                    node,
                    parent_id.clone(),
                    self.current_visibility.clone(),
                ));
            }
            "class" => {
                symbol_opt = Some(symbols::extract_class(
                    &mut self.base,
                    node,
                    parent_id.clone(),
                    self.current_visibility.clone(),
                ));
            }
            "singleton_class" => {
                symbol_opt = Some(symbols::extract_singleton_class(
                    &mut self.base,
                    node,
                    parent_id.clone(),
                ));
            }
            "method" => {
                symbol_opt = Some(symbols::extract_method(
                    &mut self.base,
                    node,
                    parent_id.clone(),
                    self.current_visibility.clone(),
                ));
            }
            "singleton_method" => {
                symbol_opt = Some(symbols::extract_singleton_method(
                    &mut self.base,
                    node,
                    parent_id.clone(),
                    self.current_visibility.clone(),
                ));
            }
            "call" => {
                if let Some(symbol) = calls::extract_call(&mut self.base, node) {
                    symbol_opt = Some(symbol);
                }
            }
            "assignment" | "operator_assignment" => {
                // Handle assignments by extracting symbols
                if let Some(symbol) =
                    assignments::extract_assignment(&mut self.base, node, parent_id.clone())
                {
                    symbols.push(symbol);
                }
            }
            "class_variable" | "instance_variable" | "global_variable" => {
                // Only create symbol if not part of an assignment (which handles it)
                if !helpers::is_part_of_assignment(&node) {
                    symbol_opt = Some(symbols::extract_variable(&mut self.base, node));
                }
            }
            "constant" => {
                // Extract all constants for now to debug the parent_id issue
                symbol_opt = Some(symbols::extract_constant(
                    &mut self.base,
                    node,
                    parent_id.clone(),
                ));
            }
            "alias" => {
                symbol_opt = Some(symbols::extract_alias(&mut self.base, node));
            }
            "identifier" => {
                // Handle visibility modifiers
                let text = self.base.get_node_text(&node);
                if let Some(new_visibility) = helpers::parse_visibility(&text) {
                    self.current_visibility = new_visibility;
                }
            }
            _ => {}
        }

        // Add symbol to collection and update parent_id for children
        let current_parent_id = if let Some(symbol) = symbol_opt {
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);
            Some(symbol_id)
        } else {
            parent_id
        };

        // Recursively traverse children with updated parent context
        let old_visibility = self.current_visibility.clone();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // Check if child is a visibility modifier that affects subsequent siblings
            if child.kind() == "identifier" {
                let text = self.base.get_node_text(&child);
                if let Some(new_visibility) = helpers::parse_visibility(&text) {
                    self.current_visibility = new_visibility;
                }
            }
            self.traverse_tree_with_parent(child, symbols, current_parent_id.clone());
        }
        self.current_visibility = old_visibility; // Restore previous visibility
    }
}
