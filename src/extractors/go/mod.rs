mod functions;
mod helpers;
mod identifiers;
mod relationships;
mod signatures;
mod specs;
mod types;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Go language extractor that handles Go-specific constructs including:
/// - Structs, interfaces, and type aliases
/// - Functions and methods with receivers
/// - Packages and imports
/// - Constants and variables
/// - Goroutines and channels
/// - Interface implementations and embedding
pub struct GoExtractor {
    base: BaseExtractor,
}

impl GoExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
        }
    }

    /// Extract symbols from Go source code - direct port from reference logic
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree(tree.root_node(), &mut symbols, None);

        // Prioritize functions over fields with the same name (reference logic)
        self.prioritize_functions_over_fields(symbols)
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        let symbol_map = self.build_symbol_map(symbols);

        // Extract relationships from the AST
        self.walk_tree_for_relationships(tree.root_node(), &symbol_map, &mut relationships);

        relationships
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();

        for symbol in symbols {
            if let Some(signature) = &symbol.signature {
                // Extract type information from signatures
                match symbol.kind {
                    SymbolKind::Function | SymbolKind::Method => {
                        if let Some(return_type) =
                            self.extract_return_type_from_signature(signature)
                        {
                            types.insert(symbol.id.clone(), return_type);
                        }
                    }
                    SymbolKind::Variable | SymbolKind::Constant => {
                        if let Some(var_type) = self.extract_variable_type_from_signature(signature)
                        {
                            types.insert(symbol.id.clone(), var_type);
                        }
                    }
                    _ => {}
                }
            }
        }

        types
    }

    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        // Create symbol map for fast lookup
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();

        // Walk the tree and extract identifiers
        self.walk_tree_for_identifiers(tree.root_node(), &symbol_map);

        // Return the collected identifiers
        self.base.identifiers.clone()
    }

    /// Prioritize functions over fields with the same name (reference implementation)
    fn prioritize_functions_over_fields(&self, symbols: Vec<Symbol>) -> Vec<Symbol> {
        let mut symbol_map: HashMap<String, Vec<Symbol>> = HashMap::new();

        // Group symbols by name
        for symbol in symbols {
            symbol_map
                .entry(symbol.name.clone())
                .or_default()
                .push(symbol);
        }

        let mut result = Vec::new();

        // For each name group, add functions first, then other types
        for (_name, symbol_group) in symbol_map {
            let functions: Vec<Symbol> = symbol_group
                .iter()
                .filter(|s| s.kind == SymbolKind::Function || s.kind == SymbolKind::Method)
                .cloned()
                .collect();
            let others: Vec<Symbol> = symbol_group
                .iter()
                .filter(|s| s.kind != SymbolKind::Function && s.kind != SymbolKind::Method)
                .cloned()
                .collect();

            result.extend(functions);
            result.extend(others);
        }

        result
    }

    /// Walk the tree and extract symbols (port from walkTree method)
    fn walk_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        // Handle declarations that can produce multiple symbols
        match node.kind() {
            "import_declaration" => {
                let import_symbols = self.extract_import_symbols(node, parent_id.as_deref());
                symbols.extend(import_symbols);
            }
            "var_declaration" => {
                let var_symbols = self.extract_var_symbols(node, parent_id.as_deref());
                symbols.extend(var_symbols);
            }
            "const_declaration" => {
                let const_symbols = self.extract_const_symbols(node, parent_id.as_deref());
                symbols.extend(const_symbols);
            }
            "field_declaration" => {
                // Fields can have multiple names on same line (X, Y float64)
                let field_symbols = self.extract_field(node, parent_id.as_deref());
                symbols.extend(field_symbols);
                return; // Don't walk children - fields are leaf nodes
            }
            _ => {
                if let Some(symbol) = self.extract_symbol(node, parent_id.as_deref()) {
                    let symbol_id = symbol.id.clone();
                    symbols.push(symbol);

                    // Recursively walk children with the new parent_id
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        self.walk_tree(child, symbols, Some(symbol_id.clone()));
                    }
                    return;
                }
            }
        }

        // If no symbol was created, continue walking children with same parent_id
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, symbols, parent_id.clone());
        }
    }

    /// Extract symbol from node (port from extractSymbol method)
    fn extract_symbol(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        match node.kind() {
            "package_clause" => self.extract_package(node, parent_id),
            "type_declaration" => self.extract_type_declaration(node, parent_id),
            "function_declaration" => Some(self.extract_function(node, parent_id)),
            "method_declaration" => Some(self.extract_method(node, parent_id)),
            // "field_declaration" handled in walk_tree (can produce multiple symbols)
            "ERROR" => self.extract_from_error_node(node, parent_id),
            _ => None,
        }
    }

    fn build_symbol_map<'a>(&self, symbols: &'a [Symbol]) -> HashMap<String, &'a Symbol> {
        let mut symbol_map = HashMap::new();
        for symbol in symbols {
            symbol_map.insert(symbol.name.clone(), symbol);
        }
        symbol_map
    }
}
