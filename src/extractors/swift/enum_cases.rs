use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts Swift enum cases and members
impl SwiftExtractor {
    /// Implementation of extractEnumCases method
    pub(super) fn extract_enum_cases(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "enum_case_element" {
                let name_node = child
                    .children(&mut child.walk())
                    .find(|c| c.kind() == "pattern" || c.kind() == "type_identifier");
                if let Some(name_node) = name_node {
                    let name = self.base.get_node_text(&name_node);
                    let associated_values = child
                        .children(&mut child.walk())
                        .find(|c| c.kind() == "enum_case_parameters");

                    let mut signature = name.clone();
                    if let Some(associated_values) = associated_values {
                        signature.push_str(&self.base.get_node_text(&associated_values));
                    }

                    let metadata = HashMap::from([(
                        "type".to_string(),
                        serde_json::Value::String("enum-case".to_string()),
                    )]);

                    let symbol = self.base.create_symbol(
                        &child,
                        name,
                        SymbolKind::EnumMember,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: Some(metadata),
                            doc_comment: None,
                        },
                    );
                    symbols.push(symbol);
                }
            }
        }
    }

    /// Implementation of extractEnumCase method
    pub(super) fn extract_enum_case(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "simple_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownCase".to_string());

        let mut signature = name.clone();

        // Check for associated values
        if let Some(associated_values) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "enum_type_parameters")
        {
            signature.push_str(&self.base.get_node_text(&associated_values));
        }

        // Check for raw values
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        if let Some(equal_index) = children.iter().position(|c| c.kind() == "=") {
            if let Some(raw_value_node) = children.get(equal_index + 1) {
                signature.push_str(&format!(" = {}", self.base.get_node_text(raw_value_node)));
            }
        }

        let metadata = HashMap::from([(
            "type".to_string(),
            serde_json::Value::String("enum-case".to_string()),
        )]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::EnumMember,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }
}
