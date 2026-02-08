/// YAML extractor - Extract mapping keys as symbols
///
/// Extracts YAML structure as symbols for semantic search and navigation.
/// - Mapping pairs: Individual key: value entries (the useful symbols)
/// - Anchors: Detected and included in signature (e.g., `defaults: &defaults`)
///
/// Intentionally skipped (noise):
/// - Documents: Generic container, every YAML file has one
/// - Flow mappings: Inline objects {...} — generic name, not useful
///
/// Common use cases:
/// - CI/CD configs (GitHub Actions, GitLab CI)
/// - Kubernetes manifests
/// - Docker Compose files
/// - Ansible playbooks
/// - Configuration files
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, SymbolKind};
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
            // Block mapping pairs are the useful symbols (key: value entries)
            "block_mapping_pair" => self.extract_mapping_pair(node, parent_id),

            // "document" and "flow_mapping" are noise — generic names with no
            // search value. Their children are still walked and extracted.
            _ => None,
        }
    }

    /// Extract a block mapping pair (key: value) as a symbol.
    /// If the value has a YAML anchor (`&name`), include it in the signature.
    fn extract_mapping_pair(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        use crate::base::SymbolOptions;

        // Extract the key name
        let key_name = self.extract_mapping_key(node)?;

        // Skip merge keys (<<: *alias) — they're YAML syntax, not meaningful symbols
        if key_name == "<<" {
            return None;
        }

        // Check for anchor on the value side
        let anchor = self.extract_anchor(node);
        let signature = anchor.map(|a| format!("{}: &{}", key_name, a));

        // Determine kind: container keys (with nested mappings) are Module, leaves are Variable
        let kind = if self.has_nested_mapping(node) {
            SymbolKind::Module
        } else {
            SymbolKind::Variable
        };

        let options = SymbolOptions {
            signature,
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None,
            ..Default::default()
        };

        let symbol = self.base.create_symbol(
            &node,
            key_name,
            kind,
            options,
        );

        Some(symbol)
    }

    /// Extract anchor name from a block_mapping_pair's value side.
    /// In `defaults: &defaults`, the AST has:
    ///   block_mapping_pair -> block_node -> anchor -> anchor_name
    fn extract_anchor(&self, node: tree_sitter::Node) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "block_node" {
                let mut block_cursor = child.walk();
                for block_child in child.children(&mut block_cursor) {
                    if block_child.kind() == "anchor" {
                        // Find the anchor_name child
                        let mut anchor_cursor = block_child.walk();
                        for anchor_child in block_child.children(&mut anchor_cursor) {
                            if anchor_child.kind() == "anchor_name" {
                                return Some(self.base.get_node_text(&anchor_child));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Check if a block_mapping_pair's value side contains a nested block_mapping.
    /// This distinguishes container keys (database:) from leaf keys (host: localhost).
    fn has_nested_mapping(&self, node: tree_sitter::Node) -> bool {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "block_node" {
                let mut block_cursor = child.walk();
                for block_child in child.children(&mut block_cursor) {
                    if block_child.kind() == "block_mapping" {
                        return true;
                    }
                }
            }
        }
        false
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
        tree: &tree_sitter::Tree,
        symbols: &[Symbol],
    ) -> Vec<Identifier> {
        self.walk_tree_for_aliases(tree.root_node(), symbols);
        self.base.identifiers.clone()
    }

    /// Walk the tree looking for alias nodes (*name) and create VariableRef identifiers
    fn walk_tree_for_aliases(&mut self, node: tree_sitter::Node, symbols: &[Symbol]) {
        if node.kind() == "alias" {
            self.extract_alias_identifier(node, symbols);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_aliases(child, symbols);
        }
    }

    /// Extract an alias (*name) as a VariableRef identifier, resolving to the anchor's symbol
    fn extract_alias_identifier(&mut self, node: tree_sitter::Node, symbols: &[Symbol]) {
        // Find the alias_name child to get the actual name
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "alias_name" {
                let alias_name = self.base.get_node_text(&child);

                // Find the containing symbol (which mapping pair contains this alias)
                let containing_symbol_id = self
                    .base
                    .find_containing_symbol(&node, symbols)
                    .map(|s| s.id.clone());

                // Resolve: find the symbol whose signature contains &{alias_name}
                let anchor_pattern = format!("&{}", alias_name);
                let target_symbol_id = symbols
                    .iter()
                    .find(|s| {
                        s.signature
                            .as_ref()
                            .is_some_and(|sig| sig.contains(&anchor_pattern))
                    })
                    .map(|s| s.id.clone());

                let mut identifier = self.base.create_identifier(
                    &child,
                    alias_name,
                    IdentifierKind::VariableRef,
                    containing_symbol_id,
                );

                // Set the resolved target if we found the anchor symbol
                if target_symbol_id.is_some() {
                    identifier.target_symbol_id = target_symbol_id.clone();
                    // Also update in the base's identifiers vec
                    if let Some(last) = self.base.identifiers.last_mut() {
                        last.target_symbol_id = target_symbol_id;
                    }
                }

                return;
            }
        }
    }
}
