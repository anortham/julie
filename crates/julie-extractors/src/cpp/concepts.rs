//! C++20 concept extraction.

use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

use super::helpers;

pub(super) fn extract_concept(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name").or_else(|| {
        node.children(&mut node.walk())
            .find(|child| child.kind() == "identifier")
    })?;
    let name = base.get_node_text(&name_node);

    let mut signature = base.get_node_text(&node).trim().to_string();
    if let Some(template_params) = helpers::extract_template_parameters(base, node.parent()) {
        signature = format!("{}\n{}", template_params, signature);
    }

    let mut metadata = HashMap::new();
    metadata.insert("kind".to_string(), Value::String("concept".to_string()));

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Type,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: base.find_doc_comment(&node),
            annotations: Vec::new(),
        },
    ))
}
