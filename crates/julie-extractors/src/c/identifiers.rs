//! Identifier extraction for function calls, member access, and type references
//!
//! This module handles extraction of identifier usages within C code, such as function calls,
//! member/field access operations, and type_identifier references (TypeUsage).

use crate::base::{Identifier, IdentifierKind, Symbol};
use crate::c::CExtractor;
use std::collections::HashMap;

/// Extract all identifiers from the syntax tree
pub(super) fn extract_identifiers(
    extractor: &mut CExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base.identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node(extractor, node, symbol_map);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(extractor, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function calls: add(), printf()
        "call_expression" => {
            if let Some(func_node) = node.child_by_field_name("function") {
                let name = extractor.base.get_node_text(&func_node);

                // Find containing symbol (which function contains this call)
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                // Create identifier for this function call
                extractor.base.create_identifier(
                    &func_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
            // Phase 3: capture string-literal call-arguments (config-free; the
            // carrier classification + gate happen in the src/ pipeline).
            record_c_call_arg_literals(extractor, node, symbol_map);
        }

        // Type references: typedef names, struct tags, enum tags in type positions.
        // C's tree-sitter grammar uses `type_identifier` for user-defined types
        // appearing in declarations, parameters, field types, casts, sizeof, etc.
        "type_identifier" => {
            if let Some(parent) = node.parent() {
                let is_definition_site = match parent.kind() {
                    // `struct Foo { ... }` — "Foo" is the tag being defined
                    // But `struct Foo*` in a parameter is a USAGE (struct_specifier
                    // without a body/field_declaration_list child).
                    "struct_specifier" | "union_specifier" => {
                        // It's a definition if the struct/union has a body
                        parent.child_by_field_name("body").is_some()
                    }
                    // `enum Color { ... }` — "Color" is the tag being defined
                    "enum_specifier" => parent.child_by_field_name("body").is_some(),
                    // `typedef int MyInt;` — "MyInt" is the alias being defined.
                    // In C's tree-sitter grammar, the typedef alias is the
                    // `declarator` field of `type_definition`.
                    "type_definition" => {
                        // The type_identifier is the declarator (the new name)
                        node.parent()
                            .and_then(|p| p.child_by_field_name("declarator"))
                            .is_some_and(|d| d.id() == node.id())
                    }
                    _ => false,
                };

                if !is_definition_site {
                    let name = extractor.base.get_node_text(&node);
                    let containing_symbol_id =
                        find_containing_symbol_id(extractor, node, symbol_map);
                    extractor.base.create_identifier(
                        &node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                }
            }
        }

        // Member/field access: p->x, obj.field
        "field_expression" => {
            // Skip if parent is a call_expression (will be handled as function call)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    return;
                }
            }

            // Extract field name from field_expression
            if let Some(field_node) = node.child_by_field_name("field") {
                let name = extractor.base.get_node_text(&field_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base.create_identifier(
                    &field_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {}
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &CExtractor,
    node: tree_sitter::Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    extractor
        .base
        .find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

// ============================================================================
// String-literal call-argument capture (Miller bridge Phase 3)
// ============================================================================

/// Capture string-literal arguments of a C `call_expression` as `Literal`
/// records. Config-free: `carrier` is the called function name (or `recv.field`
/// for a function-pointer member call); the URL/SQL classification and the
/// carrier gate run later in the `src/` pipeline. C has no named-argument
/// wrappers, so each `argument_list` named child is decoded directly.
/// `arg_position` is counted over the full argument list, so e.g. the URL in
/// `curl_easy_setopt(h, CURLOPT_URL, "https://...")` reports position 2.
fn record_c_call_arg_literals(
    extractor: &mut CExtractor,
    node: tree_sitter::Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(func_node) = node.child_by_field_name("function") else {
        return;
    };
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = c_carrier(extractor, func_node);
    let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

    let mut cursor = args.walk();
    for (pos, arg) in args.named_children(&mut cursor).enumerate() {
        if let Some(text) = extractor.base.decode_string_literal(&arg) {
            extractor.base.record_literal(
                &arg,
                text,
                carrier.clone(),
                pos as u32,
                containing_symbol_id.clone(),
            );
        }
    }
}

/// Derive a C call's carrier. Plain `identifier` → its text (`sqlite3_exec`);
/// `field_expression` (`p->fn`, `obj.fn` via function pointer) → the
/// `object.field` join so the gate's last-segment rule can match a bare config.
fn c_carrier(extractor: &CExtractor, func_node: tree_sitter::Node) -> Option<String> {
    match func_node.kind() {
        "identifier" => Some(extractor.base.get_node_text(&func_node)),
        "field_expression" => {
            let object = func_node
                .child_by_field_name("argument")
                .map(|n| extractor.base.get_node_text(&n));
            let field = func_node
                .child_by_field_name("field")
                .map(|n| extractor.base.get_node_text(&n));
            match (object, field) {
                (Some(o), Some(f)) => Some(format!("{o}.{f}")),
                (None, Some(f)) => Some(f),
                _ => None,
            }
        }
        _ => {
            let text = extractor.base.get_node_text(&func_node);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}
