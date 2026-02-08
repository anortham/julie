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
        struct_name.clone(),
        SymbolKind::Struct,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("struct".to_string())),
                ("name".to_string(), Value::String(struct_name)),
                (
                    "fields".to_string(),
                    Value::String(format!(
                        "{} fields",
                        signatures::extract_struct_fields(&extractor.base, node).len()
                    )),
                ),
            ])),
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
        union_name.clone(),
        SymbolKind::Union,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("union".to_string())),
                ("name".to_string(), Value::String(union_name)),
                (
                    "fields".to_string(),
                    Value::String(format!(
                        "{} fields",
                        signatures::extract_struct_fields(&extractor.base, node).len()
                    )),
                ),
            ])),
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
        enum_name.clone(),
        SymbolKind::Enum,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("enum".to_string())),
                ("name".to_string(), Value::String(enum_name)),
                (
                    "values".to_string(),
                    Value::String(format!(
                        "{} values",
                        signatures::extract_enum_values(&extractor.base, node).len()
                    )),
                ),
            ])),
            doc_comment,
        },
    ))
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
                            name.clone(),
                            SymbolKind::Constant,
                            SymbolOptions {
                                signature: Some(signature),
                                visibility: Some(Visibility::Public),
                                parent_id: Some(parent_enum_id.to_string()),
                                metadata: Some(HashMap::from([
                                    ("type".to_string(), Value::String("enum_value".to_string())),
                                    ("name".to_string(), Value::String(name)),
                                    (
                                        "value".to_string(),
                                        Value::String(value.unwrap_or_default()),
                                    ),
                                    (
                                        "enumParent".to_string(),
                                        Value::String(parent_enum_id.to_string()),
                                    ),
                                ])),
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
