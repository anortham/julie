/// TOML extractor - Extract tables and key-value pairs as symbols
///
/// Extracts TOML tables and key-value pairs for semantic search and navigation.
/// - Regular tables: [table_name] -> SymbolKind::Module
/// - Nested tables: [parent.child] -> SymbolKind::Module
/// - Array tables: [[array_table]] -> SymbolKind::Module
/// - Key-value pairs: key = value -> SymbolKind::Property
use crate::base::{BaseExtractor, Identifier, Symbol, SymbolKind};
use std::path::Path;

pub struct TomlExtractor {
    pub(crate) base: BaseExtractor,
}

impl TomlExtractor {
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

    /// Walk the tree and extract table symbols
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
            "table" => self.extract_table(node, parent_id, false),
            "table_array_element" => self.extract_table(node, parent_id, true),
            "pair" => self.extract_pair(node, parent_id),
            _ => None,
        }
    }

    /// Extract a table (regular or array) as a symbol
    fn extract_table(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
        _is_array: bool,
    ) -> Option<Symbol> {
        use crate::base::SymbolOptions;

        // Find the table name (looking for identifier or dotted key)
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // Look for the table header (the part between [ ] or [[ ]])
        let table_name = self.extract_table_name(&children)?;

        let options = SymbolOptions {
            signature: None,
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment: None,
            ..Default::default()
        };

        let symbol = self.base.create_symbol(
            &node,
            table_name,
            SymbolKind::Module, // All tables are modules/containers
            options,
        );

        Some(symbol)
    }

    /// Extract a key-value pair as a Property symbol
    fn extract_pair(
        &mut self,
        node: tree_sitter::Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        use crate::base::SymbolOptions;

        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // Need at least key, =, value
        if children.len() < 3 {
            return None;
        }

        // Extract key name from first child (bare_key, quoted_key, or dotted_key)
        let key_node = children[0];
        let key_name = match key_node.kind() {
            "bare_key" => self.base.get_node_text(&key_node),
            "quoted_key" => {
                let text = self.base.get_node_text(&key_node);
                text.trim_matches('"').trim_matches('\'').to_string()
            }
            "dotted_key" => {
                // dotted_key has children: bare_key . bare_key . bare_key
                self.base.get_node_text(&key_node)
            }
            _ => return None,
        };

        if key_name.is_empty() {
            return None;
        }

        // Value is the last child (after key and =)
        let value_node = *children.last().unwrap();
        let value_text = self.base.get_node_text(&value_node);

        // Build signature as "key = value", truncating long values
        let max_sig_len = 80;
        let prefix = format!("{} = ", key_name);
        let signature = if prefix.len() + value_text.len() > max_sig_len {
            let available = max_sig_len.saturating_sub(prefix.len() + 3); // 3 for "..."
            // Find a safe char boundary for truncation
            let truncated: String = value_text.chars().take(available).collect();
            format!("{}{}...", prefix, truncated)
        } else {
            format!("{}{}", prefix, value_text)
        };

        // Extract string values into doc_comment for semantic search
        let doc_comment = if value_node.kind() == "string" {
            let trimmed = value_text.trim_matches('"').trim_matches('\'');
            if !trimmed.is_empty() {
                Some(if trimmed.len() <= 2000 {
                    trimmed.to_string()
                } else {
                    // Truncate at char boundary to avoid panic on multi-byte UTF-8
                    trimmed.chars().take(2000).collect()
                })
            } else {
                None
            }
        } else {
            None
        };

        let options = SymbolOptions {
            signature: Some(signature),
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            doc_comment,
            ..Default::default()
        };

        let symbol = self
            .base
            .create_symbol(&node, key_name, SymbolKind::Property, options);

        Some(symbol)
    }

    /// Extract the table name from children nodes
    fn extract_table_name(&self, children: &[tree_sitter::Node]) -> Option<String> {
        for child in children {
            match child.kind() {
                "bare_key" | "quoted_key" | "dotted_key" => {
                    let name = self.base.get_node_text(child);
                    // Remove quotes if present
                    let name = name.trim_matches('"').trim_matches('\'');
                    return Some(name.to_string());
                }
                _ => {
                    // Recursively check children
                    let mut cursor = child.walk();
                    let nested_children: Vec<_> = child.children(&mut cursor).collect();
                    if let Some(name) = self.extract_table_name(&nested_children) {
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    pub fn extract_identifiers(
        &mut self,
        _tree: &tree_sitter::Tree,
        _symbols: &[Symbol],
    ) -> Vec<Identifier> {
        // TOML is configuration data - no code identifiers
        Vec::new()
    }
}
