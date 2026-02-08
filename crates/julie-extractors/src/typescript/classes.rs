//! Class extraction
//!
//! This module handles extraction of class declarations including inheritance,
//! modifiers, and abstract classes.

use super::helpers;
use crate::base::{Symbol, SymbolKind, SymbolOptions};
use crate::typescript::TypeScriptExtractor;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract a class declaration
pub(super) fn extract_class(
    extractor: &mut TypeScriptExtractor,
    node: Node,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name");
    let name = name_node.map(|n| extractor.base().get_node_text(&n))?;

    let visibility = extractor.base().extract_visibility(&node);
    let mut metadata = HashMap::new();

    // Check for inheritance (extends clause)
    let mut extends_name = if let Some(heritage) = node.child_by_field_name("superclass") {
        let superclass_name = extractor.base().get_node_text(&heritage);
        metadata.insert("extends".to_string(), serde_json::json!(superclass_name));
        Some(superclass_name)
    } else {
        None
    };

    // Check for class_heritage node for extends/implements clauses
    let mut implements_names: Vec<String> = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            let mut heritage_cursor = child.walk();
            for heritage_child in child.children(&mut heritage_cursor) {
                if heritage_child.kind() == "implements_clause" {
                    let mut impl_cursor = heritage_child.walk();
                    for impl_child in heritage_child.children(&mut impl_cursor) {
                        if impl_child.kind() != "implements" && impl_child.kind() != "," {
                            let impl_name = extractor.base().get_node_text(&impl_child);
                            if !impl_name.is_empty() {
                                implements_names.push(impl_name);
                            }
                        }
                    }
                }
                // Also pick up extends from heritage if not already found via superclass field
                if heritage_child.kind() == "extends_clause" && extends_name.is_none() {
                    let mut ext_cursor = heritage_child.walk();
                    for ext_child in heritage_child.children(&mut ext_cursor) {
                        if ext_child.kind() != "extends" && ext_child.kind() != "," {
                            let ext_name = extractor.base().get_node_text(&ext_child);
                            if !ext_name.is_empty() {
                                metadata.insert("extends".to_string(), serde_json::json!(&ext_name));
                                extends_name = Some(ext_name);
                            }
                        }
                    }
                }
            }
        }
    }

    if !implements_names.is_empty() {
        metadata.insert("implements".to_string(), serde_json::json!(implements_names));
    }

    // Check for abstract modifier
    let is_abstract = helpers::has_modifier(node, "abstract");
    metadata.insert("isAbstract".to_string(), serde_json::json!(is_abstract));

    // Build signature
    let mut signature = String::new();
    if is_abstract {
        signature.push_str("abstract ");
    }
    signature.push_str(&format!("class {}", name));
    if let Some(ref ext) = extends_name {
        signature.push_str(&format!(" extends {}", ext));
    }
    if !implements_names.is_empty() {
        signature.push_str(&format!(" implements {}", implements_names.join(", ")));
    }

    // Extract JSDoc comment
    let doc_comment = extractor.base().find_doc_comment(&node);

    Some(extractor.base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility,
            parent_id: None,
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}
