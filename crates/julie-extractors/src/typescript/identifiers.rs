//! Identifier extraction (function calls, member access, etc.)
//!
//! This module handles extraction of identifier usages for LSP-quality find_references functionality,
//! including function calls, member access, and other identifier references.

use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, extract_type_arguments};
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
            // Phase 3: capture string-literal call-arguments (config-free; the
            // carrier classification + gate happen in the src/ pipeline).
            record_call_arg_literals(extractor, &node, symbol_map);
        }

        // Heritage clause: `class A extends Base<Foo, Bar>` or `class A extends Base`.
        // The `extends_clause` grammar uses an expression-context `value` field for the
        // base-class identifier (so it's `identifier`, not `type_identifier`) plus an
        // optional separate `type_arguments` field. Unlike type annotations, this does NOT
        // produce a `generic_type` node, so the `type_identifier` arm cannot hook here.
        "extends_clause" => {
            if let Some(value_node) = node.child_by_field_name("value") {
                if let Some((name_node, name)) = terminal_identifier(extractor, value_node) {
                    let containing_symbol_id =
                        find_containing_symbol_id(extractor, node, symbol_map);
                    let identifier = extractor.base_mut().create_identifier(
                        &name_node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                    // Capture type arguments if the base class is generic
                    if let Some(arg_list) = node.child_by_field_name("type_arguments") {
                        let arguments = extract_type_arguments(
                            extractor.base(),
                            arg_list,
                            decompose_ts_type_arg,
                        );
                        extractor.base_mut().record_type_arguments(&identifier, arguments);
                    }
                }
            }
        }

        "new_expression" => {
            if let Some((name_node, name)) = constructor_identifier(extractor, &node) {
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);
                let identifier = extractor.base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
                // Capture applied type arguments when the new_expression carries `<...>`
                // (e.g. `new Map<string, User>()`). The type_arguments node sits as a
                // direct named child of the new_expression alongside the constructor and
                // arguments list.
                let maybe_type_args = {
                    let mut cursor = node.walk();
                    node.named_children(&mut cursor)
                        .find(|c| c.kind() == "type_arguments")
                };
                if let Some(arg_list) = maybe_type_args {
                    let arguments = extract_type_arguments(
                        extractor.base(),
                        arg_list,
                        decompose_ts_type_arg,
                    );
                    extractor.base_mut().record_type_arguments(&identifier, arguments);
                }
            }
        }

        "jsx_opening_element" | "jsx_self_closing_element" => {
            if let Some((name_node, name)) = jsx_component_identifier(extractor, &node) {
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);
                extractor.base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
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
                if parent.kind() == "new_expression" {
                    if let Some(constructor_node) = parent.child_by_field_name("constructor") {
                        if constructor_node.id() == node.id() {
                            return;
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

            let identifier = extractor.base_mut().create_identifier(
                &node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
            // If this type_identifier is the name of an outermost generic_type (e.g. the
            // `Base` in `extends Base<Foo,Bar>` or the `Map` in `field: Map<K,V>`),
            // record the applied type arguments in order. Nested generics are skipped here
            // because they ride along as `children` of the enclosing usage.
            record_outermost_generic_type_arguments_ts(extractor, node, &identifier);
        }

        _ => {}
    }
}

fn constructor_identifier<'tree>(
    extractor: &TypeScriptExtractor,
    node: &Node<'tree>,
) -> Option<(Node<'tree>, String)> {
    let constructor = node
        .child_by_field_name("constructor")
        .or_else(|| node.child_by_field_name("callee"))
        .or_else(|| {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find(|child| !matches!(child.kind(), "arguments" | "type_arguments"))
        })?;
    terminal_identifier(extractor, constructor)
}

fn jsx_component_identifier<'tree>(
    extractor: &TypeScriptExtractor,
    node: &Node<'tree>,
) -> Option<(Node<'tree>, String)> {
    let name_node = node.child_by_field_name("name")?;
    let (identifier_node, name) = terminal_identifier(extractor, name_node)?;
    if is_component_name(&name) {
        Some((identifier_node, name))
    } else {
        None
    }
}

fn terminal_identifier<'tree>(
    extractor: &TypeScriptExtractor,
    node: Node<'tree>,
) -> Option<(Node<'tree>, String)> {
    match node.kind() {
        "identifier"
        | "property_identifier"
        | "type_identifier"
        | "private_property_identifier" => Some((node, extractor.base().get_node_text(&node))),
        "member_expression" => node
            .child_by_field_name("property")
            .and_then(|property| terminal_identifier(extractor, property)),
        "jsx_namespace_name" | "nested_identifier" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .filter(|child| matches!(child.kind(), "identifier" | "property_identifier"))
                .last()
                .and_then(|child| terminal_identifier(extractor, child))
        }
        _ => None,
    }
}

fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .map_or(false, |first| first.is_ascii_uppercase())
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &TypeScriptExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    extractor
        .base()
        .find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

// ============================================================================
// String-literal call-argument capture helpers (Miller bridge Phase 3)
// ============================================================================

/// Capture string-literal arguments of a call as `Literal` records.
///
/// Config-free: `carrier` is the verbatim callee text; the URL/SQL
/// classification and the carrier gate run later in the `src/` pipeline.
/// Records one literal per string-like argument, with `arg_position` counted
/// over the full (named) argument list so `foo(x, "sql")` reports position 1.
fn record_call_arg_literals(
    extractor: &mut TypeScriptExtractor,
    call_node: &Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(function_node) = call_node.child_by_field_name("function") else {
        return;
    };
    let Some(args_node) = call_node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = callee_text(extractor, function_node);
    let containing_symbol_id = find_containing_symbol_id(extractor, *call_node, symbol_map);

    let mut cursor = args_node.walk();
    for (pos, arg) in args_node.named_children(&mut cursor).enumerate() {
        if let Some(text) = extractor.base().decode_string_literal(&arg) {
            extractor.base_mut().record_literal(
                &arg,
                text,
                carrier.clone(),
                pos as u32,
                containing_symbol_id.clone(),
            );
        }
    }
}

/// Derive the verbatim callee text used as a literal's `carrier`.
///
/// Plain `identifier` → its text (`fetch`). `member_expression` → the
/// `object.property` join (`axios.get`) so dotted client APIs match config.
fn callee_text(extractor: &TypeScriptExtractor, function_node: Node) -> Option<String> {
    match function_node.kind() {
        "identifier" => Some(extractor.base().get_node_text(&function_node)),
        "member_expression" => {
            let object = function_node
                .child_by_field_name("object")
                .map(|n| extractor.base().get_node_text(&n));
            let property = function_node
                .child_by_field_name("property")
                .map(|n| extractor.base().get_node_text(&n));
            match (object, property) {
                (Some(o), Some(p)) => Some(format!("{o}.{p}")),
                (None, Some(p)) => Some(p),
                _ => None,
            }
        }
        _ => {
            let text = extractor.base().get_node_text(&function_node);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

// ============================================================================
// Type-argument capture helpers (Miller bridge Phase 2)
// ============================================================================

/// First `type_arguments` child of a `generic_type` (its `<...>`), if any.
fn type_arguments_child(generic_type: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = generic_type.walk();
    generic_type
        .children(&mut cursor)
        .find(|child| child.kind() == "type_arguments")
}

/// `TypeArgDecomposer` for TypeScript: maps a child of a `type_arguments` list to its
/// applied argument. Skips unnamed nodes (punctuation `<`, `,`, `>`); for a nested
/// `generic_type` returns the base name plus its inner `type_arguments` to recurse into;
/// for every other named type node returns its source text as a leaf.
fn decompose_ts_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None;
    }
    match node.kind() {
        "generic_type" => {
            // The `name` field of a `generic_type` is its head identifier/type_identifier.
            let name = node
                .child_by_field_name("name")
                .map(|n| base.get_node_text(&n))
                .unwrap_or_else(|| base.get_node_text(&node));
            let nested = type_arguments_child(node);
            Some((name, nested))
        }
        _ => Some((base.get_node_text(&node), None)),
    }
}

/// If `name_node` is the `name` field of an *outermost* `generic_type` use site
/// (e.g. the `Base` in `extends Base<Foo,Bar>` or the `Map` in `field: Map<K,V>`),
/// record that generic's ordered/nested applied type arguments against `identifier`.
///
/// "Outermost" means the `generic_type`'s parent is NOT `type_arguments`; nested
/// generics such as `Array<User>` inside `Map<string, Array<User>>` are skipped here
/// because they ride along as `children` of the enclosing usage and must not be
/// double-counted as separate top-level usages.
fn record_outermost_generic_type_arguments_ts(
    extractor: &mut TypeScriptExtractor,
    name_node: Node,
    identifier: &Identifier,
) {
    let Some(generic_type) = name_node.parent() else {
        return;
    };
    if generic_type.kind() != "generic_type" {
        return;
    }
    // A generic_type whose parent is type_arguments is itself nested inside another
    // generic — its args ride along under the outer usage.
    if generic_type
        .parent()
        .map(|p| p.kind() == "type_arguments")
        .unwrap_or(false)
    {
        return;
    }
    let Some(arg_list) = type_arguments_child(generic_type) else {
        return;
    };
    let arguments = extract_type_arguments(extractor.base(), arg_list, decompose_ts_type_arg);
    extractor.base_mut().record_type_arguments(identifier, arguments);
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
    if name.len() == 1
        && name
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_uppercase())
    {
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
