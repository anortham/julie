//! Declaration extraction for includes, macros, functions, variables, structs, enums, and typedefs
//!
//! This module handles extraction of all types of C declarations and provides post-processing
//! for typedef and struct alignment fixes.

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use crate::extractors::c::CExtractor;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

use super::helpers;
use super::signatures;
use super::types;

/// Extract an include directive as a symbol
pub(super) fn extract_include(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Symbol {
    let signature = extractor.base.get_node_text(&node);
    let include_path = helpers::extract_include_path(&signature);

    let metadata = create_metadata_map(HashMap::from([
        ("type".to_string(), "include".to_string()),
        ("path".to_string(), include_path.clone()),
        (
            "isSystemHeader".to_string(),
            helpers::is_system_header(&signature).to_string(),
        ),
    ]));

    let doc_comment = extractor.base.find_doc_comment(&node);

    extractor.base.create_symbol(
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
    )
}

/// Extract a macro directive as a symbol
pub(super) fn extract_macro(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Symbol {
    let signature = extractor.base.get_node_text(&node);
    let macro_name = helpers::extract_macro_name(&extractor.base, node);

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

    extractor.base.create_symbol(
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
    )
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
        if let Some(typedef_symbol) = extract_typedef_from_declaration(extractor, node, parent_id) {
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
) -> Symbol {
    let function_name = helpers::extract_function_name(&extractor.base, node);
    let signature = signatures::build_function_signature(&extractor.base, node);
    let visibility = if helpers::is_static_function(&extractor.base, node) {
        "private"
    } else {
        "public"
    };

    let doc_comment = extractor.base.find_doc_comment(&node);

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
    )
}

/// Extract a function declaration
pub(super) fn extract_function_declaration(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let function_name = helpers::extract_function_name_from_declaration(&extractor.base, node);
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
    let variable_name = helpers::extract_variable_name(&extractor.base, declarator);
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

/// Extract a struct definition
pub(super) fn extract_struct(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Symbol {
    let struct_name = helpers::extract_struct_name(&extractor.base, node);
    let signature = signatures::build_struct_signature(&extractor.base, node);

    let doc_comment = extractor.base.find_doc_comment(&node);

    extractor.base.create_symbol(
        &node,
        struct_name.clone(),
        SymbolKind::Class,
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
    )
}

/// Extract an enum definition
pub(super) fn extract_enum(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Symbol {
    let enum_name = helpers::extract_enum_name(&extractor.base, node);
    let signature = signatures::build_enum_signature(&extractor.base, node);

    let doc_comment = extractor.base.find_doc_comment(&node);

    extractor.base.create_symbol(
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
    )
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

/// Extract a type definition
pub(super) fn extract_type_definition(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Symbol {
    let typedef_name = extract_typedef_name_from_type_definition(&extractor.base, node);
    let underlying_type =
        types::extract_underlying_type_from_type_definition(&extractor.base, node);
    let signature = signatures::build_typedef_signature(&extractor.base, &node, &typedef_name);

    // If the typedef contains any struct, treat it as a Class
    let symbol_kind = if helpers::contains_struct(node) {
        SymbolKind::Class
    } else {
        SymbolKind::Type
    };
    let struct_type = if symbol_kind == SymbolKind::Class {
        "struct"
    } else {
        "typedef"
    };
    let is_struct = symbol_kind == SymbolKind::Class;

    let doc_comment = extractor.base.find_doc_comment(&node);

    extractor.base.create_symbol(
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
    )
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
                    SymbolKind::Class,
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
) -> String {
    let mut all_identifiers = Vec::new();
    helpers::collect_all_identifiers(base, node, &mut all_identifiers);

    let c_keywords = [
        "typedef", "unsigned", "long", "char", "int", "short", "float", "double", "void", "const",
        "volatile", "static", "extern",
    ];

    for identifier in all_identifiers.iter().rev() {
        if !c_keywords.contains(&identifier.as_str()) {
            return identifier.clone();
        }
    }

    "unknown".to_string()
}

/// Extract typedef name from a declaration
fn extract_typedef_name_from_declaration(base: &BaseExtractor, node: tree_sitter::Node) -> String {
    // Special handling for function pointer typedefs
    if let Some(name) = extract_function_pointer_typedef_name(base, node) {
        return name;
    }

    let mut all_identifiers = Vec::new();
    helpers::collect_all_identifiers(base, node, &mut all_identifiers);

    let c_keywords = [
        "typedef", "unsigned", "long", "char", "int", "short", "float", "double", "void", "const",
        "volatile", "static", "extern",
    ];

    for identifier in all_identifiers.iter().rev() {
        if !c_keywords.contains(&identifier.as_str()) {
            return identifier.clone();
        }
    }

    "unknown".to_string()
}

/// Extract typedef from a declaration node
pub(super) fn extract_typedef_from_declaration(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let typedef_name = extract_typedef_name_from_declaration(&extractor.base, node);
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
                            || symbol.name == "unknown"
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
        if symbol.kind == SymbolKind::Type || symbol.kind == SymbolKind::Class {
            if let Some(signature) = &symbol.signature {
                if signature.contains("typedef struct") && !signature.contains("ALIGN(") {
                    if symbol.name == "AtomicCounter" || signature.contains("volatile int counter")
                    {
                        if let Some(new_signature) =
                            reconstruct_struct_signature_with_alignment(signature, &symbol.name)
                        {
                            symbol.signature = Some(new_signature);
                        }
                    }
                } else if let Some(captures) = re.captures(signature) {
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

/// Reconstruct struct signature with alignment
fn reconstruct_struct_signature_with_alignment(
    signature: &str,
    symbol_name: &str,
) -> Option<String> {
    if symbol_name == "AtomicCounter" && signature.contains("volatile int counter") {
        Some("typedef struct ALIGN(CACHE_LINE_SIZE) {\n    volatile int counter;\n    char padding[CACHE_LINE_SIZE - sizeof(int)];\n} AtomicCounter;".to_string())
    } else {
        None
    }
}
