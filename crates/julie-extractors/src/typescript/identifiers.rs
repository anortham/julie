//! Identifier extraction (function calls, member access, etc.)
//!
//! This module handles extraction of identifier usages for LSP-quality find_references functionality,
//! including function calls, member access, and other identifier references.

use crate::base::{Identifier, IdentifierKind, Symbol};
use crate::typescript::TypeScriptExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages from the tree
pub(super) fn extract_identifiers(
    extractor: &mut TypeScriptExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base().identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    extractor: &mut TypeScriptExtractor,
    node: Node,
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
    extractor: &mut TypeScriptExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function/method calls: foo(), object.method()
        "call_expression" => {
            // The function being called is in the "function" field
            if let Some(function_node) = node.child_by_field_name("function") {
                match function_node.kind() {
                    "identifier" => {
                        // Simple function call: foo()
                        let name = extractor.base().get_node_text(&function_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            &function_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    "member_expression" => {
                        // Member call: object.method()
                        // Extract the rightmost identifier (the method name)
                        if let Some(property_node) = function_node.child_by_field_name("property") {
                            let name = extractor.base().get_node_text(&property_node);
                            let containing_symbol_id =
                                find_containing_symbol_id(extractor, node, symbol_map);

                            extractor.base_mut().create_identifier(
                                &property_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                    _ => {
                        // Other cases like computed member expressions
                        // Skip for now
                    }
                }
            }
        }

        // Member access: object.property
        "member_expression" => {
            // Only extract if it's NOT part of a call_expression
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    // Check if this member_expression is the function being called
                    if let Some(function_node) = parent.child_by_field_name("function") {
                        if function_node.id() == node.id() {
                            return; // Skip - handled by call_expression
                        }
                    }
                }
            }

            // Extract the rightmost identifier (the property name)
            if let Some(property_node) = node.child_by_field_name("property") {
                let name = extractor.base().get_node_text(&property_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &property_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        // Type references: const x: Foo, function f(a: Foo): Bar, field: Foo
        // TypeScript tree-sitter uses `type_identifier` for BOTH declaration names
        // (interface Foo, type Foo) AND reference positions (const x: Foo).
        // We only want references — declarations are filtered by parent context.
        "type_identifier" => {
            // Skip if this is a declaration name, not a type reference.
            // type_identifier is the `name` field of declarations and type parameters.
            if is_type_declaration_name(&node) {
                return;
            }

            let name = extractor.base().get_node_text(&node);

            // Skip common utility types and single-letter generic params
            if is_ts_noise_type(&name) {
                return;
            }

            let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

            extractor.base_mut().create_identifier(
                &node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
        }

        _ => {}
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &TypeScriptExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    // CRITICAL FIX: Only search symbols from THIS FILE
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == extractor.base().file_path)
        .map(|&s| s.clone())
        .collect();

    extractor
        .base()
        .find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}

/// Check if a `type_identifier` node is a declaration name rather than a type reference.
///
/// In TypeScript tree-sitter, `type_identifier` appears as the `name` field of:
/// - `interface_declaration` → `interface Foo {}` (declaration)
/// - `type_alias_declaration` → `type Foo = ...` (declaration)
/// - `class_declaration` / `abstract_class_declaration` → `class Foo {}` (declaration)
/// - `type_parameter` → `<T extends Base>` (the `T` is a declaration)
/// - `mapped_type_clause` → `[K in keyof T]` (the `K` is a declaration)
///
/// It also appears as the `name` field of reference contexts like `generic_type`
/// and `nested_type_identifier` — those are NOT declarations.
fn is_type_declaration_name(node: &Node) -> bool {
    if let Some(parent) = node.parent() {
        // Check if this node is the `name` field of a declaration or type param
        if let Some(name_node) = parent.child_by_field_name("name") {
            if name_node.id() == node.id() {
                return matches!(
                    parent.kind(),
                    "interface_declaration"
                        | "type_alias_declaration"
                        | "class_declaration"
                        | "abstract_class_declaration"
                        | "type_parameter"
                        | "mapped_type_clause"
                );
            }
        }
    }
    false
}

/// Returns true for TypeScript types that are too common to be meaningful
/// type references for centrality scoring.
///
/// Only filters types that are TypeScript compiler intrinsics (mapped/conditional
/// utility types) and single-letter generics. Does NOT filter JavaScript runtime
/// globals (Map, Set, Promise, Array, etc.) because user-defined types with those
/// names must be trackable — and builtin references to non-existent symbols cause
/// zero centrality impact anyway (Step 1b only boosts symbols in the symbols table).
fn is_ts_noise_type(name: &str) -> bool {
    // Single-letter names are almost always generic type parameters used in scope.
    // Even when they appear as references (e.g. `: T`), they carry no cross-file signal.
    if name.len() == 1 && name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
        return true;
    }

    // TypeScript compiler utility types — these are never user-defined
    matches!(
        name,
        "Record"
            | "Partial"
            | "Required"
            | "Readonly"
            | "Pick"
            | "Omit"
            | "Exclude"
            | "Extract"
            | "NonNullable"
            | "ReturnType"
            | "Parameters"
            | "InstanceType"
            | "ConstructorParameters"
            | "ThisType"
            | "Awaited"
    )
}
