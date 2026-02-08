//! Typedef extraction and post-processing for C code
//!
//! Handles extraction of type definitions, typedef name resolution,
//! function pointer typedef detection, and alignment attribute fixes.

use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::c::CExtractor;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

use super::helpers;
use super::signatures;
use super::types;

/// Extract a type definition
pub(super) fn extract_type_definition(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let typedef_name = extract_typedef_name_from_type_definition(&extractor.base, node)?;
    let underlying_type =
        types::extract_underlying_type_from_type_definition(&extractor.base, node);
    let signature = signatures::build_typedef_signature(&extractor.base, &node, &typedef_name);

    // Determine the correct kind based on the underlying type
    let symbol_kind = if helpers::contains_union(node) {
        SymbolKind::Union
    } else if helpers::contains_struct(node) {
        SymbolKind::Struct
    } else {
        SymbolKind::Type
    };
    let struct_type = match symbol_kind {
        SymbolKind::Struct => "struct",
        SymbolKind::Union => "union",
        _ => "typedef",
    };
    let is_struct = symbol_kind == SymbolKind::Struct || symbol_kind == SymbolKind::Union;

    let doc_comment = extractor.base.find_doc_comment(&node);

    Some(extractor.base.create_symbol(
        &node,
        typedef_name.clone(),
        symbol_kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String(struct_type.to_string())),
                ("name".to_string(), Value::String(typedef_name)),
                ("underlyingType".to_string(), Value::String(underlying_type)),
                ("isStruct".to_string(), Value::String(is_struct.to_string())),
            ])),
            doc_comment,
        },
    ))
}

/// Extract typedef from a declaration node
pub(super) fn extract_typedef_from_declaration(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let typedef_name = extract_typedef_name_from_declaration(&extractor.base, node)?;
    let signature = extractor.base.get_node_text(&node);
    let underlying_type = types::extract_underlying_type_from_declaration(&extractor.base, node);

    let doc_comment = extractor.base.find_doc_comment(&node);

    Some(extractor.base.create_symbol(
        &node,
        typedef_name.clone(),
        SymbolKind::Type,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(HashMap::from([
                ("type".to_string(), Value::String("typedef".to_string())),
                ("name".to_string(), Value::String(typedef_name)),
                ("underlyingType".to_string(), Value::String(underlying_type)),
            ])),
            doc_comment,
        },
    ))
}

/// Extract from expression statement (special case for typedef names)
pub(super) fn extract_from_expression_statement(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            let identifier_name = extractor.base.get_node_text(&child);

            // Check if this looks like a typedef name by looking at siblings
            if helpers::looks_like_typedef_name(&extractor.base, &node, &identifier_name) {
                let signature =
                    signatures::build_typedef_signature(&extractor.base, &node, &identifier_name);
                let doc_comment = extractor.base.find_doc_comment(&node);
                return Some(extractor.base.create_symbol(
                    &node,
                    identifier_name.clone(),
                    SymbolKind::Struct,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: Some(HashMap::from([
                            ("type".to_string(), Value::String("struct".to_string())),
                            ("name".to_string(), Value::String(identifier_name)),
                            (
                                "fromExpressionStatement".to_string(),
                                Value::String("true".to_string()),
                            ),
                        ])),
                        doc_comment,
                    },
                ));
            }
        }
    }
    None
}

/// Extract typedef name from type definition
fn extract_typedef_name_from_type_definition(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> Option<String> {
    let mut all_identifiers = Vec::new();
    helpers::collect_all_identifiers(base, node, &mut all_identifiers);

    let c_keywords = [
        "typedef", "unsigned", "long", "char", "int", "short", "float", "double", "void", "const",
        "volatile", "static", "extern",
    ];

    for identifier in all_identifiers.iter().rev() {
        if !c_keywords.contains(&identifier.as_str()) {
            return Some(identifier.clone());
        }
    }

    None
}

/// Extract typedef name from a declaration
fn extract_typedef_name_from_declaration(base: &BaseExtractor, node: tree_sitter::Node) -> Option<String> {
    // Special handling for function pointer typedefs
    if let Some(name) = extract_function_pointer_typedef_name(base, node) {
        return Some(name);
    }

    let mut all_identifiers = Vec::new();
    helpers::collect_all_identifiers(base, node, &mut all_identifiers);

    let c_keywords = [
        "typedef", "unsigned", "long", "char", "int", "short", "float", "double", "void", "const",
        "volatile", "static", "extern",
    ];

    for identifier in all_identifiers.iter().rev() {
        if !c_keywords.contains(&identifier.as_str()) {
            return Some(identifier.clone());
        }
    }

    None
}

/// Extract function pointer typedef name using regex
fn extract_function_pointer_typedef_name(
    base: &BaseExtractor,
    node: tree_sitter::Node,
) -> Option<String> {
    let signature = base.get_node_text(&node);
    let re = Regex::new(r"typedef\s+[^(]*\(\s*\*\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)").ok()?;

    if let Some(captures) = re.captures(&signature) {
        if let Some(name_match) = captures.get(1) {
            let name = name_match.as_str().to_string();
            if helpers::is_valid_typedef_name(&name) {
                return Some(name);
            }
        }
    }

    None
}

/// Fix function pointer typedef names in post-processing
pub(super) fn fix_function_pointer_typedef_names(symbols: &mut [Symbol]) {
    let re = Regex::new(r"typedef\s+[^(]*\(\s*\*\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)").unwrap();

    for symbol in symbols.iter_mut() {
        if symbol.kind == SymbolKind::Type {
            if let Some(signature) = &symbol.signature {
                if let Some(captures) = re.captures(signature) {
                    if let Some(name_match) = captures.get(1) {
                        let correct_name = name_match.as_str();

                        let should_fix = (symbol.name.len() <= 2
                            && symbol.name.chars().all(|c| c.is_ascii_lowercase()))
                            || symbol.name != correct_name;

                        if should_fix {
                            symbol.name = correct_name.to_string();
                            if let Some(metadata) = &mut symbol.metadata {
                                metadata.insert(
                                    "name".to_string(),
                                    Value::String(correct_name.to_string()),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Fix struct alignment attributes in post-processing
pub(super) fn fix_struct_alignment_attributes(symbols: &mut [Symbol]) {
    let re = Regex::new(r"typedef\s+struct\s+(ALIGN\([^)]+\))").unwrap();

    for symbol in symbols.iter_mut() {
        if matches!(
            symbol.kind,
            SymbolKind::Type | SymbolKind::Struct | SymbolKind::Union
        ) {
            if let Some(signature) = &symbol.signature {
                if let Some(captures) = re.captures(signature) {
                    if let Some(align_match) = captures.get(1) {
                        let align_attr = align_match.as_str();
                        if !signature.contains(&format!("struct {}", align_attr)) {
                            let fixed_signature =
                                signature.replace("struct", &format!("struct {}", align_attr));
                            symbol.signature = Some(fixed_signature);
                        }
                    }
                }
            }
        }
    }
}
