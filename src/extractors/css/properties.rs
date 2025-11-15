// CSS Extractor Properties - Extract CSS custom properties and @supports rules

use super::helpers::PropertyHelper;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) struct PropertyExtractor;

impl PropertyExtractor {
    /// Extract custom property - Implementation of extractCustomProperty
    pub(super) fn extract_custom_property(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let property_name = base.get_node_text(&node);
        let value_node = PropertyHelper::find_property_value(&node);
        let value = if let Some(val_node) = value_node {
            base.get_node_text(&val_node)
        } else {
            String::new()
        };

        let signature = format!("{}: {}", property_name, value);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("custom-property".to_string()),
        );
        metadata.insert(
            "property".to_string(),
            serde_json::Value::String(property_name.clone()),
        );
        metadata.insert("value".to_string(), serde_json::Value::String(value));

        // Extract CSS comment
        let doc_comment = base.find_doc_comment(&node);

        Some(base.create_symbol(
            &node,
            property_name,
            SymbolKind::Property, // Custom properties as properties
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        ))
    }

    /// Extract supports rule - port of extractSupportsRule
    pub(super) fn extract_supports_rule(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let condition = Self::extract_supports_condition(base, &node);
        let signature = base.get_node_text(&node);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("supports-rule".to_string()),
        );
        metadata.insert(
            "condition".to_string(),
            serde_json::Value::String(condition.clone()),
        );

        // Extract CSS comment
        let doc_comment = base.find_doc_comment(&node);

        Some(base.create_symbol(
            &node,
            condition,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        ))
    }

    /// Extract supports condition - port of extractSupportsCondition
    pub(super) fn extract_supports_condition(base: &BaseExtractor, node: &Node) -> String {
        let text = base.get_node_text(node);
        if let Some(captures) = regex::Regex::new(r"@supports\s+([^{]+)")
            .unwrap()
            .captures(&text)
        {
            // Safe: capture group 1 exists if regex matched (pattern has one capture group)
            let condition = captures.get(1).map_or("", |m| m.as_str()).trim();
            format!("@supports {}", condition)
        } else {
            "@supports".to_string()
        }
    }
}
