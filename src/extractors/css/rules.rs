// CSS Extractor Rules - Extract CSS rules and their properties

use super::helpers::PropertyHelper;
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) struct RuleExtractor;

impl RuleExtractor {
    /// Extract CSS rule - Implementation of extractRule
    pub(super) fn extract_rule(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Find selectors and declaration block
        let mut selectors_node = None;
        let mut declaration_block = None;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "selectors" => selectors_node = Some(child),
                "block" => declaration_block = Some(child),
                _ => {}
            }
        }

        let selector_text = if let Some(selectors) = selectors_node {
            base.get_node_text(&selectors)
        } else {
            "unknown".to_string()
        };

        let signature = Self::build_rule_signature(base, &node, &selector_text);

        // Determine symbol kind based on selector type
        let symbol_kind = if selector_text.starts_with('.') {
            SymbolKind::Class // Class selectors
        } else if selector_text.starts_with('#') {
            SymbolKind::Variable // ID selectors (treated as variables)
        } else if selector_text == ":root" {
            SymbolKind::Class // :root pseudo-class treated as class
        } else {
            SymbolKind::Variable // Other selectors
        };

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("css-rule".to_string()),
        );
        metadata.insert(
            "selector".to_string(),
            serde_json::Value::String(selector_text.clone()),
        );

        let properties = PropertyHelper::extract_properties(base, declaration_block.as_ref());
        metadata.insert(
            "properties".to_string(),
            serde_json::Value::Array(
                properties
                    .into_iter()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );

        // Extract CSS comment
        let doc_comment = base.find_doc_comment(&node);

        Some(base.create_symbol(
            &node,
            selector_text,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        ))
    }

    /// Build rule signaimplementation's buildRuleSignature
    pub(super) fn build_rule_signature(
        base: &BaseExtractor,
        node: &Node,
        selector: &str,
    ) -> String {
        let declaration_block = PropertyHelper::find_declaration_block(node);

        if let Some(block) = declaration_block {
            let key_properties =
                PropertyHelper::extract_key_properties(base, &block, Some(selector));
            if !key_properties.is_empty() {
                return format!("{} {{ {} }}", selector, key_properties.join("; "));
            }
        }

        selector.to_string()
    }
}
