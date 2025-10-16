use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract struct type declarations
pub(super) fn extract_struct(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    is_public_fn: fn(&BaseExtractor, Node) -> bool,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let name = base.get_node_text(&name_node);
    let is_public = is_public_fn(base, node);

    let signature = format!("struct {}", name);
    let visibility = if is_public {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let doc_comment = base.extract_documentation(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract union type declarations
pub(super) fn extract_union(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    is_public_fn: fn(&BaseExtractor, Node) -> bool,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let name = base.get_node_text(&name_node);
    let is_public = is_public_fn(base, node);

    // Check if it's a union(enum) or regular union
    let node_text = base.get_node_text(&node);
    let union_type = if node_text.contains("union(enum)") {
        "union(enum)"
    } else {
        "union"
    };

    let signature = format!("{} {}", union_type, name);
    let visibility = if is_public {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let doc_comment = base.extract_documentation(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract enum type declarations
pub(super) fn extract_enum(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    is_public_fn: fn(&BaseExtractor, Node) -> bool,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let name = base.get_node_text(&name_node);
    let is_public = is_public_fn(base, node);

    let signature = format!("enum {}", name);
    let visibility = if is_public {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let doc_comment = base.extract_documentation(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Enum,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract enum variant/field declarations
pub(super) fn extract_enum_variant(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let variant_name = base.get_node_text(&name_node);

    Some(base.create_symbol(
        &node,
        variant_name.clone(),
        SymbolKind::EnumMember,
        SymbolOptions {
            signature: Some(variant_name),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment: None,
        },
    ))
}

/// Extract struct/container field declarations
pub(super) fn extract_struct_field(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let field_name = base.get_node_text(&name_node);

    // Look for type information in various forms
    let type_node = base
        .find_child_by_type(&node, "type_expression")
        .or_else(|| base.find_child_by_type(&node, "builtin_type"))
        .or_else(|| base.find_child_by_type(&node, "slice_type"))
        .or_else(|| {
            // Look for identifier after colon for type
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            let colon_index = children.iter().position(|child| child.kind() == ":")?;
            children.get(colon_index + 1).copied()
        });

    let field_type = if let Some(type_node) = type_node {
        base.get_node_text(&type_node)
    } else {
        "unknown".to_string()
    };

    let signature = format!("{}: {}", field_name, field_type);

    Some(base.create_symbol(
        &node,
        field_name,
        SymbolKind::Field,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public), // Zig struct fields are generally public
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment: None,
        },
    ))
}

/// Extract error type declarations (error_declaration)
pub(super) fn extract_error_type(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let name = base.get_node_text(&name_node);

    let signature = format!("error {}", name);
    let metadata = Some({
        let mut meta = HashMap::new();
        meta.insert("isErrorType".to_string(), serde_json::Value::Bool(true));
        meta
    });

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Class,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.cloned(),
            metadata,
            doc_comment: base.extract_documentation(&node),
        },
    ))
}

/// Extract type alias declarations
pub(super) fn extract_type_alias(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    is_public_fn: fn(&BaseExtractor, Node) -> bool,
) -> Option<Symbol> {
    let name_node = base.find_child_by_type(&node, "identifier")?;
    let name = base.get_node_text(&name_node);
    let is_public = is_public_fn(base, node);

    let signature = format!("type {}", name);
    let visibility = if is_public {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let metadata = Some({
        let mut meta = HashMap::new();
        meta.insert("isTypeAlias".to_string(), serde_json::Value::Bool(true));
        meta
    });

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Interface,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata,
            doc_comment: base.extract_documentation(&node),
        },
    ))
}
