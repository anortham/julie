// QML Identifier Extraction
// Extracts identifier usages: function calls, member access, variable references

use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, extract_type_arguments};
use crate::qml::QmlExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages from QML code
pub(super) fn extract_identifiers(
    extractor: &mut QmlExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base.identifiers.clone()
}

/// Recursively walk the tree and extract identifiers
fn walk_tree_for_identifiers(
    extractor: &mut QmlExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from current node
    extract_identifier_from_node(extractor, node, symbol_map);

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(extractor, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    extractor: &mut QmlExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Nested QML component instantiations: Rectangle {}, Button {}, etc.
        // The root ui_object_definition is the class declaration (handled by symbol extraction).
        // Nested ones are type references — analogous to constructor calls.
        "ui_object_definition" => {
            // Only nested objects (non-root) — check if there's a parent ui_object_definition
            let is_nested = {
                let mut current = node;
                let mut found_parent = false;
                while let Some(parent) = current.parent() {
                    if parent.kind() == "ui_object_definition" {
                        found_parent = true;
                        break;
                    }
                    current = parent;
                }
                found_parent
            };

            if is_nested {
                if let Some(type_name_node) = node.child_by_field_name("type_name") {
                    let name = extractor.base.get_node_text(&type_name_node);
                    let containing_symbol_id =
                        find_containing_symbol_id(extractor, node, symbol_map);

                    extractor.base.create_identifier(
                        &type_name_node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                }
            }
        }

        // Function/method calls: foo(), object.method()
        "call_expression" => {
            if let Some(function_node) = node.child_by_field_name("function") {
                match function_node.kind() {
                    "identifier" => {
                        // Simple function call: foo()
                        let name = extractor.base.get_node_text(&function_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base.create_identifier(
                            &function_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    "member_expression" => {
                        // Member call: object.method()
                        if let Some(property_node) = function_node.child_by_field_name("property") {
                            let name = extractor.base.get_node_text(&property_node);
                            let containing_symbol_id =
                                find_containing_symbol_id(extractor, node, symbol_map);

                            extractor.base.create_identifier(
                                &property_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                    _ => {
                        // Other cases - skip for now
                    }
                }
            }
        }

        // Member access: object.property (not part of a call)
        "member_expression" => {
            // Only extract if NOT part of a call_expression
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    if let Some(function_node) = parent.child_by_field_name("function") {
                        if function_node.id() == node.id() {
                            return; // Skip - handled by call_expression
                        }
                    }
                }
            }

            // Extract the property being accessed
            if let Some(property_node) = node.child_by_field_name("property") {
                let name = extractor.base.get_node_text(&property_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base.create_identifier(
                    &property_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        // Variable references in expressions
        "identifier" => {
            // Only create variable reference if not already handled by call or member access
            if let Some(parent) = node.parent() {
                match parent.kind() {
                    "call_expression"
                    | "member_expression"
                    | "function_declaration"
                    | "ui_object_definition"
                    | "ui_property"
                    | "ui_signal" => {
                        return; // Skip - handled elsewhere or is a definition
                    }
                    _ => {
                        // This is a variable reference
                        let name = extractor.base.get_node_text(&node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base.create_identifier(
                            &node,
                            name,
                            IdentifierKind::VariableRef,
                            containing_symbol_id,
                        );
                    }
                }
            }
        }

        // Construction with generic type args: `new Map<string, User>()`
        // QML-JS (tree-sitter-qmljs) `new_expression` has `constructor` and
        // `type_arguments` as direct fields (not wrapped in `generic_type`).
        // Only fire when `type_arguments` is present; plain `new Foo()` is skipped.
        "new_expression" => {
            let Some(type_args) = node.child_by_field_name("type_arguments") else {
                return;
            };
            let Some(constructor) = node.child_by_field_name("constructor") else {
                return;
            };
            let name = extractor.base.get_node_text(&constructor);
            let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);
            let identifier = extractor.base.create_identifier(
                &constructor,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
            let arguments =
                extract_type_arguments(&extractor.base, type_args, decompose_qml_type_arg);
            extractor.base.record_type_arguments(&identifier, arguments);
        }

        // Type references in TypeScript-style annotations (QML-JS shares the TS grammar):
        //   function f(x: Array<User>): Map<K, V> {}
        // `type_identifier` is the name node of a `generic_type` or a plain type ref.
        "type_identifier" => {
            if is_qml_type_declaration_name(node) {
                return;
            }
            let name = extractor.base.get_node_text(&node);
            // QML builtin value types (`string`, `int`, `real`, `var`, ...) are not
            // resolvable type references — skip them so they don't pollute the
            // identifier table, matching the C#/Python/Razor `is_*_builtin_type`
            // convention. Builtins never carry type arguments, so skipping here does
            // not affect type-argument capture for user-defined generics.
            if is_qml_builtin_type(&name) {
                return;
            }
            let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);
            let identifier = extractor.base.create_identifier(
                &node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
            // Record ordered/nested type arguments for outermost generics.
            record_outermost_generic_type_arguments_qml(extractor, node, &identifier);
        }

        _ => {
            // Skip other node types
        }
    }
}

/// Find the containing symbol ID for a node
fn find_containing_symbol_id(
    _extractor: &QmlExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let mut current = node;

    while let Some(parent) = current.parent() {
        let parent_line = parent.start_position().row + 1;

        // Check if this parent matches any symbol by line number
        for symbol in symbol_map.values() {
            if symbol.start_line == parent_line as u32 {
                return Some(symbol.id.clone());
            }
        }

        current = parent;
    }

    None
}

// ============================================================================
// Type-argument capture helpers (Miller bridge Phase 2)
// ============================================================================

/// If `name_node` is the `name` field of an *outermost* `generic_type` use site
/// (e.g. `Array` in `Array<User>` or `Map` in `Map<K, V>`), records that
/// generic's ordered/nested applied type arguments against `identifier`.
///
/// QML-JS shares the TypeScript grammar, so `generic_type` has:
///   - `name` field: `type_identifier` (or `nested_type_identifier`)
///   - `type_arguments` field: `type_arguments` node (children are concrete type nodes)
///
/// Outermost check: skip if `generic_type`'s parent is `type_arguments`
/// (that means this generic is itself nested inside another).
fn record_outermost_generic_type_arguments_qml(
    extractor: &mut QmlExtractor,
    name_node: Node,
    identifier: &Identifier,
) {
    let Some(generic_type) = name_node.parent() else {
        return;
    };
    if generic_type.kind() != "generic_type" {
        return;
    }
    // A generic_type whose parent is type_arguments is nested — its args ride
    // along as children of the enclosing usage, not as a separate row.
    if generic_type
        .parent()
        .map(|p| p.kind() == "type_arguments")
        .unwrap_or(false)
    {
        return;
    }
    let Some(arg_list) = generic_type.child_by_field_name("type_arguments") else {
        return;
    };
    let arguments = extract_type_arguments(&extractor.base, arg_list, decompose_qml_type_arg);
    extractor.base.record_type_arguments(identifier, arguments);
}

/// `TypeArgDecomposer` for QML: maps a child of a `type_arguments` list to its
/// applied argument. Skips unnamed nodes (punctuation `<`, `,`, `>`). For a
/// nested `generic_type` returns the base name (from the `name` field) plus the
/// nested `type_arguments` to recurse into. Everything else is returned as a
/// leaf via the source text.
fn decompose_qml_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip punctuation: <, >, ,
    }
    match node.kind() {
        "generic_type" => {
            // Nested generic: `Array<User>` inside `Map<K, Array<User>>`
            let name = node
                .child_by_field_name("name")
                .map(|n| base.get_node_text(&n))
                .unwrap_or_else(|| base.get_node_text(&node));
            let nested = node.child_by_field_name("type_arguments");
            Some((name, nested))
        }
        _ => {
            // type_identifier, predefined_type, array_type, etc. — leaf node.
            Some((base.get_node_text(&node), None))
        }
    }
}

/// Check if a `type_identifier` node is a type parameter declaration name
/// rather than a type reference.
///
/// In QML-JS (TypeScript grammar), `type_identifier` appears as the `name` field
/// of `type_parameter` (`<T>` declarations), `type_alias_declaration`, and
/// `interface_declaration`. All of these are declarations, not references.
fn is_qml_type_declaration_name(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if let Some(name_node) = parent.child_by_field_name("name") {
        if name_node.id() == node.id() {
            return matches!(
                parent.kind(),
                "type_parameter" | "type_alias_declaration" | "interface_declaration"
            );
        }
    }
    false
}

/// Returns `true` for QML builtin value types, which are noise as type references
/// (no resolvable definition). Mirrors the C#/Python `is_*_builtin_type` filters.
/// Covers the documented QML basic/value types; user-defined component types and
/// JS/TS object types fall through and are recorded as type usages.
fn is_qml_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "int"
            | "double"
            | "real"
            | "string"
            | "url"
            | "color"
            | "date"
            | "time"
            | "var"
            | "variant"
            | "enumeration"
            | "list"
            | "point"
            | "rect"
            | "size"
            | "font"
            | "vector2d"
            | "vector3d"
            | "vector4d"
            | "quaternion"
            | "matrix4x4"
            // JS/TS predefined primitives that can appear in QML-JS annotations
            | "number"
            | "boolean"
            | "void"
            | "any"
            | "unknown"
            | "never"
            | "object"
            | "undefined"
            | "null"
            | "symbol"
            | "bigint"
    )
}
