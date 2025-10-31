// PHP Extractor - Type extraction (classes, interfaces, traits, enums)

use super::{determine_visibility, extract_modifiers, find_child, find_child_text, PhpExtractor};
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract PHP class declarations following reference logic
pub(super) fn extract_class(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name =
        find_child_text(extractor, &node, "name").unwrap_or_else(|| "UnknownClass".to_string());

    let modifiers = extract_modifiers(extractor, &node);
    let extends_node = find_child(extractor, &node, "base_clause");
    let implements_node = find_child(extractor, &node, "class_interface_clause");
    let attribute_list = find_child(extractor, &node, "attribute_list");

    let mut signature = String::new();

    // Add attributes if present
    if let Some(attr_node) = attribute_list {
        signature.push_str(&extractor.get_base().get_node_text(&attr_node));
        signature.push('\n');
    }

    signature.push_str(&format!("class {}", name));

    if !modifiers.is_empty() {
        signature = format!("{} {}", modifiers.join(" "), signature);
    }

    if let Some(extends_node) = extends_node {
        let base_class = extractor
            .get_base()
            .get_node_text(&extends_node)
            .replace("extends", "")
            .trim()
            .to_string();
        signature.push_str(&format!(" extends {}", base_class));
    }

    if let Some(implements_node) = implements_node {
        let interfaces = extractor
            .get_base()
            .get_node_text(&implements_node)
            .replace("implements", "")
            .trim()
            .to_string();
        signature.push_str(&format!(" implements {}", interfaces));
    }

    // Add trait usages
    if let Some(declaration_list) = find_child(extractor, &node, "declaration_list") {
        let mut cursor = declaration_list.walk();
        for child in declaration_list.children(&mut cursor) {
            if child.kind() == "use_declaration" {
                let trait_usage = extractor.get_base().get_node_text(&child);
                signature.push_str(&format!(" {}", trait_usage));
            }
        }
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("class".to_string()),
    );
    metadata.insert(
        "modifiers".to_string(),
        serde_json::Value::Array(
            modifiers
                .iter()
                .map(|m| serde_json::Value::String(m.clone()))
                .collect(),
        ),
    );

    if let Some(extends_node) = extends_node {
        metadata.insert(
            "extends".to_string(),
            serde_json::Value::String(extractor.get_base().get_node_text(&extends_node)),
        );
    }

    if let Some(implements_node) = implements_node {
        metadata.insert(
            "implements".to_string(),
            serde_json::Value::String(extractor.get_base().get_node_text(&implements_node)),
        );
    }

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    extractor.get_base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(determine_visibility(&modifiers)),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    )
}

/// Extract PHP interface declarations
pub(super) fn extract_interface(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name =
        find_child_text(extractor, &node, "name").unwrap_or_else(|| "UnknownInterface".to_string());

    let extends_node = find_child(extractor, &node, "base_clause");
    let mut signature = format!("interface {}", name);

    if let Some(extends_node) = extends_node {
        let base_interfaces = extractor
            .get_base()
            .get_node_text(&extends_node)
            .replace("extends", "")
            .trim()
            .to_string();
        signature.push_str(&format!(" extends {}", base_interfaces));
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("interface".to_string()),
    );
    if let Some(extends_node) = extends_node {
        metadata.insert(
            "extends".to_string(),
            serde_json::Value::String(extractor.get_base().get_node_text(&extends_node)),
        );
    }

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    extractor.get_base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Interface,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    )
}

/// Extract PHP trait declarations
pub(super) fn extract_trait(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name =
        find_child_text(extractor, &node, "name").unwrap_or_else(|| "UnknownTrait".to_string());

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("trait".to_string()),
    );

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    extractor.get_base_mut().create_symbol(
        &node,
        name.clone(),
        SymbolKind::Trait,
        SymbolOptions {
            signature: Some(format!("trait {}", name)),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    )
}

/// Extract PHP enum declarations
pub(super) fn extract_enum(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Symbol {
    let name =
        find_child_text(extractor, &node, "name").unwrap_or_else(|| "UnknownEnum".to_string());

    // Check for backing type (e.g., enum Status: string)
    let backing_type = find_backing_type(extractor, &node);

    // Check for implements clause (e.g., implements JsonSerializable)
    let implements_node = find_child(extractor, &node, "class_interface_clause");

    let mut signature = format!("enum {}", name);
    if let Some(backing_type) = &backing_type {
        signature.push_str(&format!(": {}", backing_type));
    }
    if let Some(implements_node) = implements_node {
        let implements_clause = extractor
            .get_base()
            .get_node_text(&implements_node)
            .replace("implements", "")
            .trim()
            .to_string();
        signature.push_str(&format!(" implements {}", implements_clause));
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("enum".to_string()),
    );
    if let Some(backing_type) = backing_type {
        metadata.insert(
            "backingType".to_string(),
            serde_json::Value::String(backing_type),
        );
    }

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    extractor.get_base_mut().create_symbol(
        &node,
        name,
        SymbolKind::Enum,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    )
}

/// Extract PHP enum cases
pub(super) fn extract_enum_case(
    extractor: &mut PhpExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_child(extractor, &node, "name")?;
    let case_name = extractor.get_base().get_node_text(&name_node);

    // Check for value assignment (e.g., case PENDING = 'pending')
    let mut value = None;
    let mut cursor = node.walk();
    let mut found_assignment = false;

    for child in node.children(&mut cursor) {
        if found_assignment {
            match child.kind() {
                "string" | "integer" => {
                    value = Some(extractor.get_base().get_node_text(&child));
                    break;
                }
                _ => {}
            }
        }
        if child.kind() == "=" {
            found_assignment = true;
        }
    }

    let mut signature = format!("case {}", case_name);
    if let Some(val) = &value {
        signature.push_str(&format!(" = {}", val));
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "type".to_string(),
        serde_json::Value::String("enum_case".to_string()),
    );
    if let Some(val) = value {
        metadata.insert("value".to_string(), serde_json::Value::String(val));
    }

    // Extract PHPDoc comment
    let doc_comment = extractor.get_base().find_doc_comment(&node);

    Some(extractor.get_base_mut().create_symbol(
        &node,
        case_name,
        SymbolKind::EnumMember,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Find backing type after colon in enum declaration
fn find_backing_type(extractor: &PhpExtractor, node: &Node) -> Option<String> {
    let mut cursor = node.walk();
    let mut found_colon = false;

    for child in node.children(&mut cursor) {
        if found_colon && child.kind() == "primitive_type" {
            return Some(extractor.get_base().get_node_text(&child));
        }
        if child.kind() == ":" {
            found_colon = true;
        }
    }
    None
}
