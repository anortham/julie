// PHP Extractor - Namespace and import declarations, variable assignments

use super::{PhpExtractor, find_child};
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract PHP namespace declarations
pub(super) fn extract_namespace(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Symbol {
    let base = extractor.get_base();
    let name = find_child(extractor, &node, "namespace_name")
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "UnknownNamespace".to_string());

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("namespace".to_string()),
    );

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    extractor.get_base_mut().create_symbol(
        &node,
        name.clone(),
        SymbolKind::Namespace,
        SymbolOptions {
            signature: Some(format!("namespace {}", name)),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    )
}

/// Extract PHP use/import declarations
pub(super) fn extract_use(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Symbol {
    let (name, alias) = match node.kind() {
        "namespace_use_declaration" => {
            // Handle new namespace_use_declaration format
            if let Some(use_clause) = find_child(extractor, &node, "namespace_use_clause") {
                if let Some(qualified_name) = find_child(extractor, &use_clause, "qualified_name") {
                    let name = extractor.get_base().get_node_text(&qualified_name);
                    let alias = find_child(extractor, &node, "namespace_aliasing_clause")
                        .map(|alias_node| extractor.get_base().get_node_text(&alias_node));
                    (name, alias)
                } else {
                    ("UnknownImport".to_string(), None)
                }
            } else {
                ("UnknownImport".to_string(), None)
            }
        }
        _ => {
            // Handle legacy use_declaration format
            let name = find_child(extractor, &node, "namespace_name")
                .or_else(|| find_child(extractor, &node, "qualified_name"))
                .map(|n| extractor.get_base().get_node_text(&n))
                .unwrap_or_else(|| "UnknownImport".to_string());
            let alias = find_child(extractor, &node, "namespace_aliasing_clause")
                .map(|alias_node| extractor.get_base().get_node_text(&alias_node));
            (name, alias)
        }
    };

    let mut signature = format!("use {}", name);
    if let Some(alias_text) = &alias {
        signature.push_str(&format!(" {}", alias_text));
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("use".to_string()),
    );
    if let Some(alias_text) = alias {
        metadata.insert("alias".to_string(), serde_json::Value::String(alias_text));
    }

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    extractor.get_base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    )
}

/// Extract variable assignments
pub(super) fn extract_variable_assignment(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Find variable name (left side of assignment)
    let variable_name_node = find_child(extractor, &node, "variable_name")?;
    let name_node = find_child(extractor, &variable_name_node, "name")?;
    let var_name = extractor.get_base().get_node_text(&name_node);

    // Find assignment value (right side of assignment)
    let mut value_text = String::new();
    let mut cursor = node.walk();
    let mut found_assignment = false;

    for child in node.children(&mut cursor) {
        if found_assignment {
            value_text = extractor.get_base().get_node_text(&child);
            break;
        }
        if child.kind() == "=" {
            found_assignment = true;
        }
    }

    let signature = format!(
        "{} = {}",
        extractor.get_base().get_node_text(&variable_name_node),
        value_text
    );

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("variable_assignment".to_string()),
    );
    metadata.insert("value".to_string(), serde_json::Value::String(value_text));

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    Some(extractor.get_base_mut().create_symbol(
        &node,
        var_name,
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}
