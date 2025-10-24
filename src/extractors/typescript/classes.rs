//! Class extraction
//!
//! This module handles extraction of class declarations including inheritance,
//! modifiers, and abstract classes.

use super::helpers;
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use crate::extractors::typescript::TypeScriptExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a class declaration
pub(super) fn extract_class(extractor: &mut TypeScriptExtractor, node: Node) -> Symbol {
    let name_node = node.child_by_field_name("name");
    let name = if let Some(name_node) = name_node {
        extractor.base().get_node_text(&name_node)
    } else {
        "Anonymous".to_string()
    };

    let visibility = extractor.base().extract_visibility(&node);
    let mut metadata = HashMap::new();

    // Check for inheritance (extends clause)
    if let Some(heritage) = node.child_by_field_name("superclass") {
        let superclass_name = extractor.base().get_node_text(&heritage);
        metadata.insert("extends".to_string(), serde_json::json!(superclass_name));
    }

    // Check for abstract modifier
    let is_abstract = helpers::has_modifier(node, "abstract");
    metadata.insert("isAbstract".to_string(), serde_json::json!(is_abstract));

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: None,
            visibility,
            parent_id: None,
            metadata: Some(metadata),
            doc_comment,
        },
    )
}
