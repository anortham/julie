// CSS Extractor Animations - Extract @keyframes and animation-related symbols

use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
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
        let keyframes_name = Self::extract_keyframes_name(base, &node)?;
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
        let animation_name = Self::extract_keyframes_name(base, &node)?;
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

    /// Extract individual keyframes - intentionally a no-op.
    ///
    /// Individual keyframe blocks (0%, 50%, 100%, from, to) are not useful as
    /// symbols for code intelligence. They pollute search results and have no
    /// meaningful name. The @keyframes rule itself and the animation name are
    /// extracted by `extract_keyframes_rule` and `extract_animation_name`.
    pub(super) fn extract_keyframes(
        _base: &mut BaseExtractor,
        _node: Node,
        _symbols: &mut Vec<Symbol>,
        _parent_id: Option<&str>,
    ) {
        // Intentionally empty — keyframe percentages/keywords are noise, not symbols.
    }

    /// Extract keyframes name - port of extractKeyframesName
    pub(super) fn extract_keyframes_name(base: &BaseExtractor, node: &Node) -> Option<String> {
        let text = base.get_node_text(node);
        let captures = regex::Regex::new(r"@keyframes\s+([^\s{]+)")
            .unwrap()
            .captures(&text)?;
        captures.get(1).map(|m| m.as_str().to_string())
    }
}
