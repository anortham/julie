// CSS Extractor Media Queries - Extract @media rules and queries

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) struct MediaExtractor;

impl MediaExtractor {
    /// Extract media rule - Implementation of extractMediaRule
    pub(super) fn extract_media_rule(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let media_query = Self::extract_media_query(base, &node);
        let signature = base.get_node_text(&node);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("media-rule".to_string()),
        );
        metadata.insert(
            "query".to_string(),
            serde_json::Value::String(media_query.clone()),
        );

        // Extract CSS comment
        let doc_comment = base.find_doc_comment(&node);

        Some(base.create_symbol(
            &node,
            media_query,
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

    /// Extract media query - port of extractMediaQuery
    pub(super) fn extract_media_query(base: &BaseExtractor, node: &Node) -> String {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // Find @media keyword
        for (i, child) in children.iter().enumerate() {
            let text = base.get_node_text(child);
            if text == "@media" {
                let mut query_parts = Vec::new();

                // Get the query parts after @media
                for j in (i + 1)..children.len() {
                    let child = &children[j];
                    if child.kind() == "block" {
                        break; // Stop at the rule block
                    }
                    let part = base.get_node_text(child).trim().to_string();
                    if !part.is_empty() {
                        query_parts.push(part);
                    }
                }

                return format!("@media {}", query_parts.join(" ").trim());
            }
        }

        "@media".to_string()
    }
}
