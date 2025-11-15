// PHP Extractor - Function/method extraction

use super::{PhpExtractor, determine_visibility, extract_modifiers, find_child, find_child_text};
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract PHP function/method declarations
pub(super) fn extract_function(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name =
        find_child_text(extractor, &node, "name").unwrap_or_else(|| "unknownFunction".to_string());

    let modifiers = extract_modifiers(extractor, &node);
    let parameters_node = find_child(extractor, &node, "formal_parameters");
    let attribute_list = find_child(extractor, &node, "attribute_list");

    // PHP return type comes after : as primitive_type, named_type, union_type, or optional_type
    let return_type_node = find_return_type(extractor, &node);

    // Check for reference modifier (&)
    let reference_modifier = find_child(extractor, &node, "reference_modifier");
    let ref_prefix = if reference_modifier.is_some() {
        "&"
    } else {
        ""
    };

    // Determine symbol kind
    let symbol_kind = match name.as_str() {
        "__construct" => SymbolKind::Constructor,
        "__destruct" => SymbolKind::Destructor,
        _ => SymbolKind::Function,
    };

    let mut signature = String::new();

    // Add attributes if present
    if let Some(attr_node) = attribute_list {
        signature.push_str(&extractor.get_base().get_node_text(&attr_node));
        signature.push('\n');
    }

    signature.push_str(&format!("function {}{}", ref_prefix, name));

    if !modifiers.is_empty() {
        signature = signature.replace(
            &format!("function {}{}", ref_prefix, name),
            &format!("{} function {}{}", modifiers.join(" "), ref_prefix, name),
        );
    }

    if let Some(params_node) = parameters_node {
        signature.push_str(&extractor.get_base().get_node_text(&params_node));
    } else {
        signature.push_str("()");
    }

    if let Some(return_node) = return_type_node {
        signature.push_str(&format!(
            ": {}",
            extractor.get_base().get_node_text(&return_node)
        ));
    }

    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), "function".to_string());
    metadata.insert("modifiers".to_string(), modifiers.join(","));

    if let Some(params_node) = parameters_node {
        metadata.insert(
            "parameters".to_string(),
            extractor.get_base().get_node_text(&params_node),
        );
    } else {
        metadata.insert("parameters".to_string(), "()".to_string());
    }

    if let Some(return_node) = return_type_node {
        metadata.insert(
            "returnType".to_string(),
            extractor.get_base().get_node_text(&return_node),
        );
    }

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    extractor.get_base_mut().create_symbol(
        &node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(determine_visibility(&modifiers)),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(
                metadata
                    .into_iter()
                    .map(|(k, v)| (k, serde_json::Value::String(v)))
                    .collect(),
            ),
            doc_comment,
        },
    )
}

/// Find return type node after colon
pub(super) fn find_return_type<'a>(_extractor: &PhpExtractor, node: &Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let mut found_colon = false;

    for child in node.children(&mut cursor) {
        if found_colon {
            match child.kind() {
                "primitive_type" | "named_type" | "union_type" | "optional_type" => {
                    return Some(child);
                }
                _ => {}
            }
        }
        if child.kind() == ":" {
            found_colon = true;
        }
    }
    None
}
