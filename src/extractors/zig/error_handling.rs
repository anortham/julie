use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use regex::Regex;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract symbols from ERROR nodes - handles incomplete/fragmented syntax
pub(super) fn extract_from_error_node(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    // Try to extract meaningful symbols from ERROR nodes
    let node_text = base.get_node_text(&node);

    // Look for partial generic type constructor pattern in fragmented ERROR nodes
    let partial_match = Regex::new(r"^const\s+(\w+)\s*\($")
        .unwrap()
        .captures(&node_text);

    if let Some(partial_match) = partial_match {
        let name = partial_match[1].to_string();

        let signature = format!("fn {}(comptime T: type) type", name);
        let metadata = Some({
            let mut meta = HashMap::new();
            meta.insert(
                "isGenericTypeConstructor".to_string(),
                serde_json::Value::Bool(true),
            );
            meta
        });

        return Some(base.create_symbol(
            &node,
            name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.cloned(),
                metadata,
                doc_comment: base.extract_documentation(&node),
            },
        ));
    }

    None
}
