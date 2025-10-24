//! Type extraction for C++ symbols
//! Handles extraction of classes, structs, unions, enums, and their members

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

use super::helpers;

/// Extract class declaration
pub(super) fn extract_class(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "type_identifier" || c.kind() == "template_type")?;

    let (name, is_specialization) = if name_node.kind() == "template_type" {
        // For template specializations like Vector<bool>, extract just the base name
        // The template_type node contains type_identifier + template_argument_list
        let type_id = name_node
            .children(&mut name_node.walk())
            .find(|c| c.kind() == "type_identifier")
            .map(|n| base.get_node_text(&n))
            .unwrap_or_else(|| base.get_node_text(&name_node));
        (type_id, true)
    } else {
        (base.get_node_text(&name_node), false)
    };

    let mut signature = if is_specialization {
        // For template specializations, include the full template type in signature
        let full_name = base.get_node_text(&name_node);
        format!("class {}", full_name)
    } else {
        format!("class {}", name)
    };

    // Handle template parameters
    if let Some(template_params) = helpers::extract_template_parameters(base, node.parent()) {
        signature = format!("{}\n{}", template_params, signature);
    }

    // Handle inheritance
    if let Some(base_clause) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "base_class_clause")
    {
        let bases = helpers::extract_base_classes(base, base_clause);
        if !bases.is_empty() {
            signature.push_str(&format!(" : {}", bases.join(", ")));
        }
    }

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract struct declaration
pub(super) fn extract_struct(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "type_identifier")?;

    let name = base.get_node_text(&name_node);
    let mut signature = format!("struct {}", name);

    // Handle template parameters
    if let Some(template_params) = helpers::extract_template_parameters(base, node.parent()) {
        signature = format!("{}\n{}", template_params, signature);
    }

    // Handle inheritance (structs can inherit too)
    if let Some(base_clause) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "base_class_clause")
    {
        let bases = helpers::extract_base_classes(base, base_clause);
        if !bases.is_empty() {
            signature.push_str(&format!(" : {}", bases.join(", ")));
        }
    }

    // Check for alignas qualifier
    let mut children_cursor = node.walk();
    let alignas_node = node
        .children(&mut children_cursor)
        .find(|c| c.kind() == "alignas_qualifier");
    if let Some(alignas) = alignas_node {
        let alignas_text = base.get_node_text(&alignas);
        signature = format!("{} {}", alignas_text, signature);
    }

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Struct,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract union declaration
pub(super) fn extract_union(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "type_identifier");

    let name = if let Some(name_node) = name_node {
        base.get_node_text(&name_node)
    } else {
        // Handle anonymous unions
        format!("<anonymous_union_{}>", node.start_position().row)
    };

    let signature = if name_node.is_some() {
        format!("union {}", name)
    } else {
        "union".to_string()
    };

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Union,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract enum declaration
pub(super) fn extract_enum(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "type_identifier")?;

    let name = base.get_node_text(&name_node);

    // Check if it's a scoped enum (enum class)
    let is_scoped = node.children(&mut node.walk()).any(|c| c.kind() == "class");

    let mut signature = if is_scoped {
        format!("enum class {}", name)
    } else {
        format!("enum {}", name)
    };

    // Check for underlying type
    let children: Vec<Node> = node.children(&mut node.walk()).collect();
    if let Some(colon_pos) = children.iter().position(|c| c.kind() == ":") {
        if colon_pos + 1 < children.len() {
            let type_node = &children[colon_pos + 1];
            if type_node.kind() == "primitive_type" || type_node.kind() == "type_identifier" {
                signature.push_str(&format!(" : {}", base.get_node_text(type_node)));
            }
        }
    }

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Enum,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract enum member
pub(super) fn extract_enum_member(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    let name_node = node
        .children(&mut cursor)
        .find(|c| c.kind() == "identifier")?;

    let name = base.get_node_text(&name_node);
    let mut signature = name.clone();

    // Check for initializer
    let children: Vec<Node> = node.children(&mut node.walk()).collect();
    if let Some(equals_pos) = children.iter().position(|c| c.kind() == "=") {
        if equals_pos + 1 < children.len() {
            let value_nodes = &children[equals_pos + 1..];
            let value: String = value_nodes
                .iter()
                .map(|n| base.get_node_text(n))
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string();
            if !value.is_empty() {
                signature.push_str(&format!(" = {}", value));
            }
        }
    }

    // Determine if this is from an anonymous enum
    let enum_parent = helpers::find_parent_enum(node);
    let is_anonymous_enum = enum_parent
        .and_then(|parent| {
            parent
                .children(&mut parent.walk())
                .find(|c| c.kind() == "type_identifier")
        })
        .is_none();

    let symbol_kind = if is_anonymous_enum {
        SymbolKind::Constant
    } else {
        SymbolKind::EnumMember
    };

    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: None,
            doc_comment,
        },
    ))
}
