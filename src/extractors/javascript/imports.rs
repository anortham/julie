//! Import statement extraction for JavaScript
//!
//! Handles extraction of ES6 import statements and CommonJS requires.

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::Node;

impl super::JavaScriptExtractor {
    /// Create import symbol - direct Implementation of createImportSymbol
    pub(super) fn create_import_symbol(
        &mut self,
        node: Node,
        specifier: &str,
        parent_id: Option<String>,
    ) -> Symbol {
        let source = node.child_by_field_name("source");
        let source_path = source
            .map(|s| {
                self.base
                    .get_node_text(&s)
                    .replace(&['\'', '"', '`'][..], "")
            })
            .unwrap_or_else(|| "unknown".to_string());

        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), json!(source_path));
        metadata.insert("specifier".to_string(), json!(specifier));
        metadata.insert(
            "isDefault".to_string(),
            json!(self.has_default_import(&node)),
        );
        metadata.insert(
            "isNamespace".to_string(),
            json!(self.has_namespace_import(&node)),
        );

        // Extract JSDoc comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            specifier.to_string(),
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(self.base.get_node_text(&node)),
                visibility: None,
                parent_id,
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Extract import specifiers - implementation's extractImportSpecifiers
    pub(super) fn extract_import_specifiers(&self, node: &Node) -> Vec<String> {
        let mut specifiers = Vec::new();

        // Look for import clause which contains the specifiers (reference logic)
        let import_clause = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "import_clause");
        if let Some(clause) = import_clause {
            for child in clause.children(&mut clause.walk()) {
                match child.kind() {
                    "import_specifier" => {
                        // For named imports like { debounce, throttle } (reference logic)
                        if let Some(name_node) = child.child_by_field_name("name") {
                            specifiers.push(self.base.get_node_text(&name_node));
                        }
                        if let Some(alias_node) = child.child_by_field_name("alias") {
                            specifiers.push(self.base.get_node_text(&alias_node));
                        }
                    }
                    "identifier" => {
                        // For default imports like React (reference logic)
                        specifiers.push(self.base.get_node_text(&child));
                    }
                    "namespace_import" => {
                        // For namespace imports like * as name (reference logic)
                        specifiers.push(self.base.get_node_text(&child));
                    }
                    "named_imports" => {
                        // Look inside named_imports for specifiers (reference logic)
                        for named_child in child.children(&mut child.walk()) {
                            if named_child.kind() == "import_specifier" {
                                if let Some(name_node) = named_child.child_by_field_name("name") {
                                    specifiers.push(self.base.get_node_text(&name_node));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        specifiers
    }
}
