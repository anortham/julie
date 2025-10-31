// CSS Extractor At-Rules - Extract @media, @import, @keyframes, etc.

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) struct AtRuleExtractor;

impl AtRuleExtractor {
    /// Extract at-rule - Implementation of extractAtRule
    pub(super) fn extract_at_rule(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let rule_name = Self::extract_at_rule_name(base, &node);
        let signature = base.get_node_text(&node);

        // Determine symbol kind based on at-rule type - match reference logic
        let symbol_kind = if rule_name == "@keyframes" {
            SymbolKind::Function // Animations as functions
        } else if rule_name == "@import" {
            SymbolKind::Import
        } else {
            SymbolKind::Variable
        };

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("at-rule".to_string()),
        );
        metadata.insert(
            "ruleName".to_string(),
            serde_json::Value::String(rule_name.clone()),
        );
        let at_rule_type = rule_name.strip_prefix('@').unwrap_or(&rule_name);
        metadata.insert(
            "atRuleType".to_string(),
            serde_json::Value::String(at_rule_type.to_string()),
        );

        // Extract CSS comment
        let doc_comment = base.find_doc_comment(&node);

        Some(base.create_symbol(
            &node,
            rule_name,
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

    /// Extract at-rule name - port of extractAtRuleName
    pub(super) fn extract_at_rule_name(base: &BaseExtractor, node: &Node) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "at_keyword" {
                return base.get_node_text(&child);
            }
            let text = base.get_node_text(&child);
            if text.starts_with('@') {
                return text
                    .split_whitespace()
                    .next()
                    .unwrap_or("@unknown")
                    .to_string();
            }
        }
        "@unknown".to_string()
    }
}
