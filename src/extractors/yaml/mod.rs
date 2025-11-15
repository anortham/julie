/// YAML extractor - Extract documents, mappings, and keys as symbols
///
/// Extracts YAML structure as symbols for semantic search and navigation.
/// - Documents: Top-level YAML documents
/// - Block mappings: Key-value sections (objects)
/// - Flow mappings: Inline objects {...}
/// - Mapping pairs: Individual key: value entries
/// - Sequences: Arrays/lists
///
/// Common use cases:
/// - CI/CD configs (GitHub Actions, GitLab CI)
/// - Kubernetes manifests
/// - Docker Compose files
/// - Ansible playbooks
/// - Configuration files
use crate::extractors::base::{BaseExtractor, Identifier, Symbol, SymbolKind};
use std::path::Path;

pub struct YamlExtractor {
    pub(crate) base: BaseExtractor,
}

impl YamlExtractor {
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

    /// Walk the tree and extract YAML symbols
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
            // Documents (top-level)
            "document" => self.extract_document(node, parent_id),

            // Block mappings (objects with key-value pairs)
            "block_mapping_pair" => self.extract_mapping_pair(node, parent_id),

            // Flow mappings (inline objects)
            "flow_mapping" => self.extract_flow_mapping(node, parent_id),

            _ => None,
        }
    }

    /// Extract a YAML document as a symbol
    fn extract_document(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        use crate::extractors::base::SymbolOptions;

        let options = SymbolOptions {
            signature: None,
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: Some("YAML document".to_string()),
            ..Default::default()
        };

        let symbol =
            self.base
                .create_symbol(&node, "document".to_string(), SymbolKind::Module, options);

        Some(symbol)
    }

    /// Extract a block mapping pair (key: value) as a symbol
    fn extract_mapping_pair(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        use crate::extractors::base::SymbolOptions;

        // Extract the key name
        let key_name = self.extract_mapping_key(node)?;

        let options = SymbolOptions {
            signature: None,
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None,
            ..Default::default()
        };

        let symbol = self.base.create_symbol(
            &node,
            key_name,
            SymbolKind::Variable, // YAML keys are like variables
            options,
        );

        Some(symbol)
    }

    /// Extract a flow mapping (inline object) as a symbol
    fn extract_flow_mapping(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        use crate::extractors::base::SymbolOptions;

        let options = SymbolOptions {
            signature: None,
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: Some("Inline mapping".to_string()),
            ..Default::default()
        };

        let symbol = self.base.create_symbol(
            &node,
            "flow_mapping".to_string(),
            SymbolKind::Module,
            options,
        );

        Some(symbol)
    }

    /// Extract the key from a block_mapping_pair
    fn extract_mapping_key(&self, node: tree_sitter::Node) -> Option<String> {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "flow_node" | "block_node" => {
                    // Look for the actual key value
                    let mut key_cursor = child.walk();
                    for key_child in child.children(&mut key_cursor) {
                        match key_child.kind() {
                            "plain_scalar" | "single_quote_scalar" | "double_quote_scalar" => {
                                let key_text = self.base.get_node_text(&key_child);
                                // Remove quotes if present
                                let key_text = key_text.trim_matches('"').trim_matches('\'');
                                return Some(key_text.to_string());
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        None
    }

    pub fn extract_identifiers(
        &mut self,
        _tree: &tree_sitter::Tree,
        _symbols: &[Symbol],
    ) -> Vec<Identifier> {
        // YAML is configuration data - no code identifiers
        Vec::new()
    }
}
