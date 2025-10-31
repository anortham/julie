use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts Swift properties, variables, and subscripts
impl SwiftExtractor {
    /// Implementation of extractVariable method
    pub(super) fn extract_variable(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let binding_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "property_binding_pattern" || c.kind() == "pattern_binding");

        if let Some(binding_node) = binding_node {
            let name_node = binding_node
                .children(&mut binding_node.walk())
                .find(|c| c.kind() == "simple_identifier" || c.kind() == "pattern");
            let name = name_node
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| "unknownVariable".to_string());

            let modifiers = self.extract_modifiers(node);
            let var_type = self.extract_variable_type(node);
            let is_let = node.children(&mut node.walk()).any(|c| c.kind() == "let");
            let is_var = node.children(&mut node.walk()).any(|c| c.kind() == "var");

            // Use "let" if is_let, otherwise default to "var"
            let mut signature = if is_let {
                format!("let {}", name)
            } else {
                format!("var {}", name)
            };

            if !modifiers.is_empty() {
                signature = format!("{} {}", modifiers.join(" "), signature);
            }

            if let Some(ref var_type) = var_type {
                signature.push_str(&format!(": {}", var_type));
            }

            let metadata = HashMap::from([
                (
                    "type".to_string(),
                    serde_json::Value::String("variable".to_string()),
                ),
                (
                    "modifiers".to_string(),
                    serde_json::Value::String(modifiers.join(", ")),
                ),
                (
                    "variableType".to_string(),
                    serde_json::Value::String(var_type.unwrap_or_else(|| "Any".to_string())),
                ),
                (
                    "isLet".to_string(),
                    serde_json::Value::String(is_let.to_string()),
                ),
                (
                    "isVar".to_string(),
                    serde_json::Value::String(is_var.to_string()),
                ),
            ]);

            // Extract Swift documentation comment
            let doc_comment = self.base.find_doc_comment(&node);

            Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(self.determine_visibility(&modifiers)),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some(metadata),
                    doc_comment,
                },
            ))
        } else {
            None
        }
    }

    /// Implementation of extractProperty method
    pub(super) fn extract_property(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "pattern");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownProperty".to_string());

        let modifiers = self.extract_modifiers(node);
        let property_type = self.extract_property_type(node);

        // Extract the property keyword (var or let)
        let binding_pattern = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "value_binding_pattern");
        let keyword = if let Some(binding_pattern) = binding_pattern {
            binding_pattern
                .children(&mut binding_pattern.walk())
                .find(|c| c.kind() == "var" || c.kind() == "let")
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| "var".to_string())
        } else {
            "var".to_string()
        };

        // Build signature with non-visibility modifiers
        let non_visibility_modifiers: Vec<_> = modifiers
            .iter()
            .filter(|m| {
                !["public", "private", "internal", "fileprivate", "open"].contains(&m.as_str())
            })
            .cloned()
            .collect();

        let mut signature = if !non_visibility_modifiers.is_empty() {
            format!(
                "{} {} {}",
                non_visibility_modifiers.join(" "),
                keyword,
                name
            )
        } else {
            format!("{} {}", keyword, name)
        };

        if let Some(ref property_type) = property_type {
            signature.push_str(&format!(": {}", property_type));
        }

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("property".to_string()),
            ),
            (
                "modifiers".to_string(),
                serde_json::Value::String(modifiers.join(", ")),
            ),
            (
                "propertyType".to_string(),
                serde_json::Value::String(property_type.unwrap_or_else(|| "Any".to_string())),
            ),
            ("keyword".to_string(), serde_json::Value::String(keyword)),
        ]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Implementation of extractSubscript method
    pub(super) fn extract_subscript(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = "subscript".to_string();
        let parameters = self
            .extract_parameters(node)
            .unwrap_or_else(|| "()".to_string());
        let return_type = self.extract_return_type(node);
        let modifiers = self.extract_modifiers(node);

        let mut signature = "subscript".to_string();

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        signature.push_str(&parameters);

        if let Some(ref return_type) = return_type {
            signature.push_str(&format!(" -> {}", return_type));
        }

        // Check for accessor requirements
        if let Some(accessor_reqs) = node.children(&mut node.walk()).find(|c| {
            c.kind() == "getter_setter_block" || c.kind() == "protocol_property_requirements"
        }) {
            signature.push_str(&format!(" {}", self.base.get_node_text(&accessor_reqs)));
        }

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("subscript".to_string()),
            ),
            (
                "parameters".to_string(),
                serde_json::Value::String(parameters),
            ),
            (
                "returnType".to_string(),
                serde_json::Value::String(return_type.unwrap_or_else(|| "Any".to_string())),
            ),
            (
                "modifiers".to_string(),
                serde_json::Value::String(modifiers.join(", ")),
            ),
        ]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }
}
