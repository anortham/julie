// CSS Extractor Animations - Extract @keyframes and animation-related symbols

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) struct AnimationExtractor;

impl AnimationExtractor {
    /// Extract keyframes rule - Implementation of extractKeyframesRule
    pub(super) fn extract_keyframes_rule(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let keyframes_name = Self::extract_keyframes_name(base, &node);
        let signature = base.get_node_text(&node);
        let symbol_name = format!("@keyframes {}", keyframes_name);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("keyframes".to_string()),
        );
        metadata.insert(
            "animationName".to_string(),
            serde_json::Value::String(keyframes_name),
        );

        // Extract CSS comment
        let doc_comment = base.find_doc_comment(&node);

        Some(base.create_symbol(
            &node,
            symbol_name,
            SymbolKind::Function, // Animations as functions as designed
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        ))
    }

    /// Extract animation name as separate symbol - for test compatibility
    pub(super) fn extract_animation_name(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let animation_name = Self::extract_keyframes_name(base, &node);
        let signature = format!("@keyframes {}", animation_name);

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("animation".to_string()),
        );

        // Extract CSS comment
        let doc_comment = base.find_doc_comment(&node);

        Some(base.create_symbol(
            &node,
            animation_name,
            SymbolKind::Function, // Animation names as functions
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|id| id.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        ))
    }

    /// Extract individual keyframes - port of extractKeyframes
    pub(super) fn extract_keyframes(
        base: &mut BaseExtractor,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "keyframe_block" {
                // Find keyframe selector (from, to, or percentage)
                let mut keyframe_selector = None;
                let mut child_cursor = child.walk();
                for grandchild in child.children(&mut child_cursor) {
                    match grandchild.kind() {
                        "from" | "to" | "percentage" => {
                            keyframe_selector = Some(grandchild);
                            break;
                        }
                        _ => {}
                    }
                }

                if let Some(selector) = keyframe_selector {
                    let selector_text = base.get_node_text(&selector);
                    let signature = base.get_node_text(&child);

                    // Create metadata
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("keyframe".to_string()),
                    );
                    metadata.insert(
                        "selector".to_string(),
                        serde_json::Value::String(selector_text.clone()),
                    );

                    // Extract CSS comment
                    let doc_comment = base.find_doc_comment(&child);

                    let symbol = base.create_symbol(
                        &child,
                        selector_text,
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|id| id.to_string()),
                            metadata: Some(metadata),
                            doc_comment,
                        },
                    );

                    symbols.push(symbol);
                }
            }
        }
    }

    /// Extract keyframes name - port of extractKeyframesName
    pub(super) fn extract_keyframes_name(base: &BaseExtractor, node: &Node) -> String {
        let text = base.get_node_text(node);
        if let Some(captures) = regex::Regex::new(r"@keyframes\s+([^\s{]+)")
            .unwrap()
            .captures(&text)
        {
            // Safe: capture group 1 exists if regex matched (pattern has one capture group)
            captures
                .get(1)
                .map_or("unknown", |m| m.as_str())
                .to_string()
        } else {
            "unknown".to_string()
        }
    }
}
