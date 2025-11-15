//! PowerShell class, method, property, and enum extraction
//! Handles OOP constructs in PowerShell 5.0+

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

use super::documentation;
use super::helpers::{
    extract_enum_member_value, extract_inheritance, extract_property_type, extract_return_type,
    find_class_name_node, find_enum_member_name_node, find_enum_name_node, find_method_name_node,
    find_property_name_node, has_modifier,
};

/// Extract class symbols
pub(super) fn extract_class(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_class_name_node(node)?;
    let name = base.get_node_text(&name_node);

    let signature = extract_class_signature(base, node);

    // Extract doc comment (PowerShell comment-based help)
    let doc_comment = documentation::extract_powershell_doc_comment(base, &node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract method symbols from class methods
pub(super) fn extract_method(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_method_name_node(node)?;
    let name = base.get_node_text(&name_node);
    let _is_static = has_modifier(base, node, "static");
    let is_hidden = has_modifier(base, node, "hidden");

    let signature = extract_method_signature(base, node);
    let visibility = if is_hidden {
        Visibility::Private
    } else {
        Visibility::Public
    };

    // Extract doc comment (PowerShell comment-based help)
    let doc_comment = documentation::extract_powershell_doc_comment(base, &node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Method,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract property symbols from class properties
pub(super) fn extract_property(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_property_name_node(node)?;
    let mut name = base.get_node_text(&name_node);
    name = name.replace("$", ""); // Remove $ prefix

    let is_hidden = has_modifier(base, node, "hidden");
    let signature = extract_property_signature(base, node);
    let visibility = if is_hidden {
        Visibility::Private
    } else {
        Visibility::Public
    };

    // Extract doc comment (PowerShell comment-based help)
    let doc_comment = documentation::extract_powershell_doc_comment(base, &node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Property,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract enum symbols
pub(super) fn extract_enum(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_enum_name_node(node)?;
    let name = base.get_node_text(&name_node);

    let signature = format!("enum {}", name);

    // Extract doc comment (PowerShell comment-based help)
    let doc_comment = documentation::extract_powershell_doc_comment(base, &node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Enum,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract enum member symbols
pub(super) fn extract_enum_member(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = find_enum_member_name_node(node)?;
    let name = base.get_node_text(&name_node);
    let value = extract_enum_member_value(base, node);

    let signature = if let Some(val) = value {
        format!("{} = {}", name, val)
    } else {
        name.clone()
    };

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::EnumMember,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment: None,
        },
    ))
}

/// Extract class signature
fn extract_class_signature(base: &BaseExtractor, node: Node) -> String {
    let name = find_class_name_node(node)
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "unknown".to_string());

    // Check for inheritance
    if let Some(inheritance) = extract_inheritance(base, node) {
        format!("class {} : {}", name, inheritance)
    } else {
        format!("class {}", name)
    }
}

/// Extract method signature
fn extract_method_signature(base: &BaseExtractor, node: Node) -> String {
    let name = find_method_name_node(node)
        .map(|n| base.get_node_text(&n))
        .unwrap_or_else(|| "unknown".to_string());

    let return_type = extract_return_type(base, node);
    let is_static = has_modifier(base, node, "static");

    let prefix = if is_static { "static " } else { "" };
    let suffix = return_type.map_or(String::new(), |t| format!(" {}", t));

    format!("{}{} {}()", prefix, suffix, name)
}

/// Extract property signature
fn extract_property_signature(base: &BaseExtractor, node: Node) -> String {
    let name = find_property_name_node(node)
        .map(|n| base.get_node_text(&n).replace("$", ""))
        .unwrap_or_else(|| "unknown".to_string());

    let property_type = extract_property_type(base, node);
    let is_hidden = has_modifier(base, node, "hidden");

    let prefix = if is_hidden { "hidden " } else { "" };
    if let Some(ptype) = property_type {
        format!("{}{}${}", prefix, ptype, name)
    } else {
        format!("{}${}", prefix, name)
    }
}
