/// JSON extractor - Extract keys and objects as symbols
///
/// Extracts JSON key-value pairs as symbols for semantic search and navigation.
/// - Top-level keys and nested object keys are extracted
/// - Objects and arrays are treated as SymbolKind::Module (containers)
/// - Primitive values are treated as SymbolKind::Variable
use crate::extractors::base::{BaseExtractor, Identifier, Symbol, SymbolKind};
use std::path::Path;

pub struct JsonExtractor {
    pub(crate) base: BaseExtractor,
}

impl JsonExtractor {
    pub fn new(
        language: String,
        file_path: String,
        source_code: String,
        workspace_root: &Path,
    ) -> Self {
        let base = BaseExtractor::new(language, file_path, source_code, workspace_root);
        Self { base }
    }

    pub fn extract_symbols(&mut self, tree: &tree_sitter::Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.walk_tree_for_symbols(tree.root_node(), &mut symbols, None);
        symbols
    }

    /// Walk the tree and extract key-value pair symbols
    fn walk_tree_for_symbols(
        &mut self,
        node: tree_sitter::Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<String>,
    ) {
        let symbol = self.extract_symbol_from_node(node, parent_id.as_deref());
        let mut current_parent_id = parent_id;

        if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            current_parent_id = Some(sym.id.clone());
        }

        // Recursively process child nodes
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_symbols(child, symbols, current_parent_id.clone());
        }
    }

    /// Extract symbol from a node based on its type
    fn extract_symbol_from_node(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        match node.kind() {
            "pair" => self.extract_pair(node, parent_id),
            _ => None,
        }
    }

    /// Extract a key-value pair as a symbol
    fn extract_pair(&mut self, node: tree_sitter::Node, parent_id: Option<&str>) -> Option<Symbol> {
        use crate::extractors::base::SymbolOptions;

        // Get children: typically [string (key), ":", value]
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        if children.len() < 3 {
            return None; // Need at least key, colon, value
        }

        // Extract key name (first child, strip quotes)
        let key_node = children[0];
        let key_text = self.base.get_node_text(&key_node);
        let key_name = key_text.trim_matches('"').to_string();

        // Value is typically the last child (after key and colon)
        let value_node = *children.last().unwrap();

        // Determine the value type to choose appropriate SymbolKind
        let symbol_kind = match value_node.kind() {
            "object" | "array" => SymbolKind::Module, // Treat containers as modules
            _ => SymbolKind::Variable,                // Treat primitives as variables
        };

        let options = SymbolOptions {
            signature: None,
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None,
            ..Default::default()
        };

        let symbol = self
            .base
            .create_symbol(&node, key_name, symbol_kind, options);

        Some(symbol)
    }

    pub fn extract_identifiers(
        &mut self,
        _tree: &tree_sitter::Tree,
        _symbols: &[Symbol],
    ) -> Vec<Identifier> {
        // JSON is configuration data - no code identifiers
        Vec::new()
    }
}
