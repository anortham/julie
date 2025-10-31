use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts protocol-specific members and requirements
impl SwiftExtractor {
    /// Implementation of extractProtocolFunction method
    pub(super) fn extract_protocol_function(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "simple_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownFunction".to_string());

        let parameters = self.extract_parameters(node);
        let return_type = self.extract_return_type(node);

        let params_str = parameters.unwrap_or_else(|| "()".to_string());
        let return_str = return_type.unwrap_or_else(|| "Void".to_string());

        let mut signature = format!("func {}", name);
        signature.push_str(&params_str);

        if !return_str.is_empty() && return_str != "Void" {
            signature.push_str(&format!(" -> {}", return_str));
        }

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("protocol-requirement".to_string()),
            ),
            (
                "parameters".to_string(),
                serde_json::Value::String(params_str),
            ),
            (
                "returnType".to_string(),
                serde_json::Value::String(return_str),
            ),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Implementation of extractProtocolProperty method
    pub(super) fn extract_protocol_property(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Symbol {
        let pattern_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "pattern");
        let name = if let Some(pattern_node) = pattern_node {
            pattern_node
                .children(&mut pattern_node.walk())
                .find(|c| c.kind() == "simple_identifier")
                .map(|n| self.base.get_node_text(&n))
                .unwrap_or_else(|| "unknownProperty".to_string())
        } else {
            "unknownProperty".to_string()
        };

        // Check for static modifier
        let modifiers_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "modifiers");
        let is_static = modifiers_node
            .map(|modifiers_node| {
                modifiers_node
                    .children(&mut modifiers_node.walk())
                    .any(|c| {
                        c.kind() == "property_modifier" && self.base.get_node_text(&c) == "static"
                    })
            })
            .unwrap_or(false);

        let property_type = self.extract_property_type(node);

        // Extract getter/setter requirements
        let protocol_requirements = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "protocol_property_requirements");
        let accessors = protocol_requirements
            .map(|req| format!(" {}", self.base.get_node_text(&req)))
            .unwrap_or_else(String::new);

        let mut signature = if is_static {
            format!("static var {}", name)
        } else {
            format!("var {}", name)
        };

        if let Some(ref property_type) = property_type {
            signature.push_str(&format!(": {}", property_type));
        }

        if !accessors.is_empty() {
            signature.push_str(&accessors);
        }

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("protocol-requirement".to_string()),
            ),
            (
                "propertyType".to_string(),
                serde_json::Value::String(property_type.unwrap_or_else(|| "Any".to_string())),
            ),
            (
                "accessors".to_string(),
                serde_json::Value::String(accessors),
            ),
            (
                "isStatic".to_string(),
                serde_json::Value::String(is_static.to_string()),
            ),
        ]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Implementation of extractAssociatedType method
    pub(super) fn extract_associated_type(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier" || c.kind() == "simple_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownType".to_string());

        let mut signature = format!("associatedtype {}", name);

        // Check for type constraints
        if let Some(inheritance) = self.extract_inheritance(node) {
            signature.push_str(&format!(": {}", inheritance));
        }

        let metadata = HashMap::from([(
            "type".to_string(),
            serde_json::Value::String("associatedtype".to_string()),
        )]);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Type,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }
}
