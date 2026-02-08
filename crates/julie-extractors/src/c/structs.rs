//! Struct, union, and enum extraction for C code
//!
//! Handles extraction of struct definitions, union definitions, enum definitions,
//! and individual enum value symbols.

use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::c::CExtractor;
use serde_json::Value;
use std::collections::HashMap;

use super::helpers;
use super::signatures;

/// Extract a struct definition
pub(super) fn extract_struct(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let struct_name = helpers::extract_struct_name(&extractor.base, node)?;
    let signature = signatures::build_struct_signature(&extractor.base, node);

    let doc_comment = extractor.base.find_doc_comment(&node);

    Some(extractor.base.create_symbol(
        &node,
        struct_name,
        SymbolKind::Struct,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract a union definition
pub(super) fn extract_union(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let union_name = helpers::extract_union_name(&extractor.base, node)?;
    let signature = signatures::build_union_signature(&extractor.base, node);

    let doc_comment = extractor.base.find_doc_comment(&node);

    Some(extractor.base.create_symbol(
        &node,
        union_name,
        SymbolKind::Union,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Extract an enum definition
pub(super) fn extract_enum(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let enum_name = helpers::extract_enum_name(&extractor.base, node)?;
    let signature = signatures::build_enum_signature(&extractor.base, node);

    let doc_comment = extractor.base.find_doc_comment(&node);

    Some(extractor.base.create_symbol(
        &node,
        enum_name,
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

/// Extract struct/union field symbols as SymbolKind::Field children
///
/// Delegates to `extract_struct_fields` for the field traversal, then wraps each
/// `StructField` into a full `Symbol`. This avoids duplicating the tree-sitter
/// walking logic between signature building and symbol extraction.
///
/// For each field we also need the tree-sitter node for position info and doc comments,
/// so we walk the body a second time to pair fields with their declarator nodes.
pub(super) fn extract_struct_field_symbols(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_struct_id: &str,
) -> Vec<Symbol> {
    let mut field_symbols = Vec::new();

    let Some(body) = node.child_by_field_name("body") else {
        return field_symbols;
    };

    // Walk field_declarations to get both StructField data and the tree-sitter nodes
    // needed for position info, signatures, and doc comments
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "field_declaration" {
            continue;
        }

        let field_type = child
            .child_by_field_name("type")
            .map(|t| extractor.base.get_node_text(&t))
            .unwrap_or_default();

        let mut decl_cursor = child.walk();
        for decl_child in child.children_by_field_name("declarator", &mut decl_cursor) {
            let Some(field_name) = helpers::find_field_identifier_name(&extractor.base, decl_child) else {
                continue;
            };

            let signature = format!("{} {}", field_type, extractor.base.get_node_text(&decl_child));
            let doc_comment = extractor.base.find_doc_comment(&child);

            let field_symbol = extractor.base.create_symbol(
                &decl_child,
                field_name,
                SymbolKind::Field,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: Some(parent_struct_id.to_string()),
                    metadata: Some(HashMap::from([(
                        "fieldType".to_string(),
                        Value::String(field_type.clone()),
                    )])),
                    doc_comment,
                },
            );

            field_symbols.push(field_symbol);
        }
    }

    field_symbols
}

/// Extract enum value symbols
pub(super) fn extract_enum_value_symbols(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_enum_id: &str,
) -> Vec<Symbol> {
    let mut enum_value_symbols = Vec::new();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "enumerator_list" {
            let mut enum_cursor = child.walk();
            for enum_child in child.children(&mut enum_cursor) {
                if enum_child.kind() == "enumerator" {
                    if let Some(name_node) = enum_child.child_by_field_name("name") {
                        let name = extractor.base.get_node_text(&name_node);
                        let value = enum_child
                            .child_by_field_name("value")
                            .map(|v| extractor.base.get_node_text(&v));

                        let mut signature = name.clone();
                        if let Some(ref val) = value {
                            signature = format!("{} = {}", signature, val);
                        }

                        let doc_comment = extractor.base.find_doc_comment(&enum_child);

                        let enum_value_symbol = extractor.base.create_symbol(
                            &enum_child,
                            name,
                            SymbolKind::Constant,
                            SymbolOptions {
                                signature: Some(signature),
                                visibility: Some(Visibility::Public),
                                parent_id: Some(parent_enum_id.to_string()),
                                metadata: if value.is_some() {
                                    Some(HashMap::from([(
                                        "value".to_string(),
                                        Value::String(value.unwrap_or_default()),
                                    )]))
                                } else {
                                    None
                                },
                                doc_comment,
                            },
                        );

                        enum_value_symbols.push(enum_value_symbol);
                    }
                }
            }
        }
    }

    enum_value_symbols
}
