//! Declaration extraction for includes, macros, functions, and variables
//!
//! This module handles extraction of C declarations: includes, macros, function definitions,
//! function declarations, and variable declarations. Struct/union/enum extraction is in
//! `structs.rs` and typedef handling is in `typedefs.rs`.

use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::c::CExtractor;
use serde_json::Value;
use std::collections::HashMap;

use super::helpers;
use super::signatures;
use super::typedefs;
use super::types;

/// Extract an include directive as a symbol
pub(super) fn extract_include(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let signature = extractor.base.get_node_text(&node);
    let include_path = helpers::extract_include_path(&signature)?;

    let metadata = create_metadata_map(HashMap::from([
        ("type".to_string(), "include".to_string()),
        ("path".to_string(), include_path.clone()),
        (
            "isSystemHeader".to_string(),
            helpers::is_system_header(&signature).to_string(),
        ),
    ]));

    let doc_comment = extractor.base.find_doc_comment(&node);

    Some(extractor.base.create_symbol(
        &node,
        include_path.clone(),
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(signature.clone()),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Extract a macro directive as a symbol
pub(super) fn extract_macro(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let signature = extractor.base.get_node_text(&node);
    let macro_name = helpers::extract_macro_name(&extractor.base, node)?;

    let metadata = create_metadata_map(HashMap::from([
        ("type".to_string(), "macro".to_string()),
        ("name".to_string(), macro_name.clone()),
        (
            "isFunctionLike".to_string(),
            (node.kind() == "preproc_function_def").to_string(),
        ),
        ("definition".to_string(), signature.clone()),
    ]));

    let doc_comment = extractor.base.find_doc_comment(&node);

    Some(extractor.base.create_symbol(
        &node,
        macro_name.clone(),
        SymbolKind::Constant,
        SymbolOptions {
            signature: Some(signature.clone()),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment,
        },
    ))
}

/// Helper for converting string metadata to serde_json::Value metadata
fn create_metadata_map(metadata: HashMap<String, String>) -> HashMap<String, Value> {
    metadata
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect()
}

/// Extract declarations (variables, functions, typedefs)
pub(super) fn extract_declaration(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    // Check if this is a typedef declaration
    if helpers::is_typedef_declaration(&extractor.base, node) {
        if let Some(typedef_symbol) = typedefs::extract_typedef_from_declaration(extractor, node, parent_id) {
            symbols.push(typedef_symbol);
            return symbols;
        }
    }

    // Check if this is a function declaration
    if let Some(_function_declarator) = helpers::find_function_declarator(node) {
        if let Some(function_symbol) = extract_function_declaration(extractor, node, parent_id) {
            symbols.push(function_symbol);
            return symbols;
        }
    }

    // Extract variable declarations
    let declarators = helpers::find_variable_declarators(node);
    for declarator in declarators {
        if let Some(variable_symbol) =
            extract_variable_declaration(extractor, node, declarator, parent_id)
        {
            symbols.push(variable_symbol);
        }
    }

    symbols
}

/// Extract a function definition
pub(super) fn extract_function_definition(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let function_name = helpers::extract_function_name(&extractor.base, node)?;
    let signature = signatures::build_function_signature(&extractor.base, node);
    let visibility = if helpers::is_static_function(&extractor.base, node) {
        "private"
    } else {
        "public"
    };

    let doc_comment = extractor.base.find_doc_comment(&node);

    Some(extractor.base.create_symbol(
        &node,
        function_name.clone(),
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(if visibility == "private" {
                Visibility::Private
            } else {
                Visibility::Public
            }),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("function".to_string())),
                ("name".to_string(), Value::String(function_name)),
                (
                    "returnType".to_string(),
                    Value::String(types::extract_return_type(&extractor.base, node)),
                ),
                (
                    "parameters".to_string(),
                    Value::String(
                        signatures::extract_function_parameters(&extractor.base, node).join(", "),
                    ),
                ),
                (
                    "isDefinition".to_string(),
                    Value::String("true".to_string()),
                ),
                (
                    "isStatic".to_string(),
                    Value::String(helpers::is_static_function(&extractor.base, node).to_string()),
                ),
            ])),
            doc_comment,
        },
    ))
}

/// Extract a function declaration
pub(super) fn extract_function_declaration(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let function_name = helpers::extract_function_name_from_declaration(&extractor.base, node)?;
    let signature = signatures::build_function_declaration_signature(&extractor.base, node);
    let visibility = if helpers::is_static_function(&extractor.base, node) {
        "private"
    } else {
        "public"
    };

    let doc_comment = extractor.base.find_doc_comment(&node);

    Some(
        extractor.base.create_symbol(
            &node,
            function_name.clone(),
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(if visibility == "private" {
                    Visibility::Private
                } else {
                    Visibility::Public
                }),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(HashMap::from([
                    ("type".to_string(), Value::String("function".to_string())),
                    ("name".to_string(), Value::String(function_name)),
                    (
                        "returnType".to_string(),
                        Value::String(types::extract_return_type(&extractor.base, node)),
                    ),
                    (
                        "parameters".to_string(),
                        Value::String(
                            signatures::extract_function_parameters_from_declaration(
                                &extractor.base,
                                node,
                            )
                            .join(", "),
                        ),
                    ),
                    (
                        "isDefinition".to_string(),
                        Value::String("false".to_string()),
                    ),
                    (
                        "isStatic".to_string(),
                        Value::String(
                            helpers::is_static_function(&extractor.base, node).to_string(),
                        ),
                    ),
                ])),
                doc_comment,
            },
        ),
    )
}

/// Extract a variable declaration
pub(super) fn extract_variable_declaration(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    declarator: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let variable_name = helpers::extract_variable_name(&extractor.base, declarator)?;
    let signature = signatures::build_variable_signature(&extractor.base, node, declarator);
    let visibility = if helpers::is_static_function(&extractor.base, node) {
        "private"
    } else {
        "public"
    };

    Some(extractor.base.create_symbol(
        &node,
        variable_name.clone(),
        SymbolKind::Variable,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(if visibility == "private" {
                Visibility::Private
            } else {
                Visibility::Public
            }),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("variable".to_string())),
                ("name".to_string(), Value::String(variable_name)),
                (
                    "dataType".to_string(),
                    Value::String(types::extract_variable_type(&extractor.base, node)),
                ),
                (
                    "isStatic".to_string(),
                    Value::String(helpers::is_static_function(&extractor.base, node).to_string()),
                ),
                (
                    "isExtern".to_string(),
                    Value::String(helpers::is_extern_variable(&extractor.base, node).to_string()),
                ),
                (
                    "isConst".to_string(),
                    Value::String(helpers::is_const_variable(&extractor.base, node).to_string()),
                ),
                (
                    "isVolatile".to_string(),
                    Value::String(helpers::is_volatile_variable(&extractor.base, node).to_string()),
                ),
                (
                    "isArray".to_string(),
                    Value::String(helpers::is_array_variable(declarator).to_string()),
                ),
                (
                    "initializer".to_string(),
                    Value::String(
                        types::extract_initializer(&extractor.base, declarator).unwrap_or_default(),
                    ),
                ),
            ])),
            doc_comment: extractor.base.find_doc_comment(&node),
        },
    ))
}

/// Extract a linkage specification (extern "C" block)
pub(super) fn extract_linkage_specification(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string_literal" {
            let linkage_text = extractor.base.get_node_text(&child);
            if linkage_text.contains("\"C\"") {
                let signature = format!("extern {}", linkage_text);
                let doc_comment = extractor.base.find_doc_comment(&node);
                return Some(extractor.base.create_symbol(
                    &node,
                    "extern_c_block".to_string(),
                    SymbolKind::Namespace,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: Some(HashMap::from([
                            (
                                "type".to_string(),
                                Value::String("linkage_specification".to_string()),
                            ),
                            ("linkage".to_string(), Value::String("C".to_string())),
                        ])),
                        doc_comment,
                    },
                ));
            }
        }
    }
    None
}
