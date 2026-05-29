/// Identifier extraction for LSP-quality find_references
/// Tracks function calls, member access, and other identifier usages
use super::PythonExtractor;
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, TypeArgument};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages (function calls, member access, etc.)
/// Following the Rust extractor reference implementation pattern
pub fn extract_identifiers(
    extractor: &mut PythonExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(extractor, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    extractor.base_mut().identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    extractor: &mut PythonExtractor,
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
    extractor: &mut PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function/method calls: foo(), bar.baz()
        // Python uses "call" node type
        "call" => {
            // The function being called is in the "function" field
            if let Some(function_node) = node.child_by_field_name("function") {
                match function_node.kind() {
                    "identifier" => {
                        // Simple function call: foo()
                        let name = extractor.base_mut().get_node_text(&function_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            &function_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    "attribute" => {
                        // Member call: object.method()
                        // Extract the rightmost identifier (the method name)
                        if let Some(attr_node) = function_node.child_by_field_name("attribute") {
                            let name = extractor.base_mut().get_node_text(&attr_node);
                            let containing_symbol_id =
                                find_containing_symbol_id(extractor, node, symbol_map);

                            extractor.base_mut().create_identifier(
                                &attr_node,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                        }
                    }
                    _ => {
                        // Other cases like subscript expressions
                        // Skip for now
                    }
                }
            }
        }

        // Member access: object.property
        // Python uses "attribute" node type
        "attribute" => {
            if is_python_type_usage_node(node) {
                if let Some(attr_node) = node.child_by_field_name("attribute") {
                    let name = extractor.base_mut().get_node_text(&attr_node);
                    if !is_python_builtin_type(&name) {
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        let identifier = extractor.base_mut().create_identifier(
                            &attr_node,
                            name,
                            IdentifierKind::TypeUsage,
                            containing_symbol_id,
                        );
                        // `node` is the whole `a.B` attribute expression; if it is
                        // the `value` field of a subscript (e.g. `typing.Optional[X]`)
                        // we record the ordered type arguments against the identifier.
                        record_outermost_python_type_arguments(extractor, node, &identifier);
                    }
                }
                return;
            }

            // Only extract if it's NOT part of a call
            // (we handle those in the call case above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call" {
                    // Check if this attribute is the function being called
                    if let Some(function_node) = parent.child_by_field_name("function") {
                        if function_node.id() == node.id() {
                            return; // Skip - handled by call
                        }
                    }
                }
            }

            // Extract the attribute name
            if let Some(attr_node) = node.child_by_field_name("attribute") {
                let name = extractor.base_mut().get_node_text(&attr_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &attr_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        "identifier" => {
            if is_python_type_usage_identifier(node) {
                let name = extractor.base_mut().get_node_text(&node);
                if !is_python_builtin_type(&name) {
                    let containing_symbol_id =
                        find_containing_symbol_id(extractor, node, symbol_map);

                    let identifier = extractor.base_mut().create_identifier(
                        &node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                    // If this identifier is the `value` of an outermost subscript
                    // (e.g. `Optional` in `Optional[User]`), record the ordered
                    // type arguments.  Nested generics are skipped here — their
                    // args ride along as `children` of the enclosing usage.
                    record_outermost_python_type_arguments(extractor, node, &identifier);
                }
            }
        }

        _ => {}
    }
}

fn is_python_type_usage_identifier(node: Node) -> bool {
    if let Some(parent) = node.parent() {
        if parent.kind() == "attribute" {
            return false;
        }
    }

    is_python_type_usage_node(node)
}

fn is_python_type_usage_node(node: Node) -> bool {
    if is_python_declaration_name(node) {
        return false;
    }

    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "type" | "generic_type" | "union_type" => return true,
            "call" | "argument_list" | "return_statement" | "block" | "module" => return false,
            _ => {}
        }

        current = parent;
    }

    false
}

fn is_python_declaration_name(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if let Some(name_node) = parent.child_by_field_name("name") {
        return name_node.id() == node.id()
            && matches!(
                parent.kind(),
                "class_definition" | "function_definition" | "type_alias_statement"
            );
    }

    false
}

fn is_python_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "bytes"
            | "complex"
            | "dict"
            | "float"
            | "frozenset"
            | "int"
            | "list"
            | "None"
            | "object"
            | "set"
            | "str"
            | "tuple"
            | "type"
    )
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &PythonExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    let base = extractor.base();
    base.find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

// ============================================================================
// Generic type-argument capture (Miller bridge Phase 2)
// ============================================================================
//
// Python uses subscript syntax for generics: `Optional[User]`, `Dict[str, V]`.
// When a TypeUsage identifier is the `value` field of a subscript and is NOT
// itself nested inside another subscript's arg list, we walk the subscript's
// args and attach them as ordered `TypeArgument`s.
//
// We do NOT use the shared `extract_type_arguments` helper because Python's
// subscript structure differs from C#'s `type_argument_list`: single-arg
// subscripts have no container node, and multi-arg subscripts wrap args in a
// `tuple` node. A custom recursive collector handles both cases correctly while
// still using `TypeArgument` and `record_type_arguments` from shared base.
//
// **Known gap:** Class-base subscripts (`class C(Generic[K, V])`) are not
// reachable here — the `argument_list` node is a stopping boundary in
// `is_python_type_usage_node`, so no TypeUsage identifier is created for
// `Generic`.  Capturing that path would require extending identifier
// extraction, which is out of scope for this leg.

/// If `value_node` is the `value` field of an outermost subscript, record the
/// subscript's ordered/nested type arguments against `identifier`.
///
/// `value_node` is either a bare `identifier` (e.g. `Optional`) or an
/// `attribute` expression (e.g. `typing.Optional`).  Nested generics are
/// skipped — their args ride along as children of the enclosing usage.
/// If `value_node` is the type-name identifier of an outermost generic, record
/// the generic's ordered/nested type arguments against `identifier`.
///
/// Python 3.9+ type annotations use TWO different AST node kinds depending on
/// context:
///
/// * **`generic_type`** — inside a `type` annotation node (the canonical path
///   for typed parameters, return types, and variable annotations).  Structure:
///   `generic_type { identifier("Optional"), type_parameter { type {...}* } }`
///
/// * **`subscript`** — raw subscript expressions outside a `type` node.
///   Structure: `subscript { value: identifier, subscript: arg* }` (repeated
///   `subscript` field, no tuple wrapper).
///
/// Only the outermost generic is captured here; nested generics ride along as
/// `children` of the enclosing usage.
fn record_outermost_python_type_arguments(
    extractor: &mut PythonExtractor,
    value_node: Node<'_>,
    identifier: &Identifier,
) {
    let Some(parent) = value_node.parent() else {
        return;
    };

    let arguments = match parent.kind() {
        "generic_type" => {
            // Annotation context: `Optional[User]` → generic_type { id, type_parameter }
            // Any `identifier` child of a `generic_type` IS the type name (not an arg),
            // so no further field-name check is needed here.
            if is_generic_type_nested_in_args(parent) {
                return;
            }
            collect_generic_type_args(extractor.base(), parent)
        }
        "subscript" => {
            // Raw subscript context (outside a `type` annotation node).
            // Verify value_node is the `value` field, not a subscript arg.
            let is_value = parent
                .child_by_field_name("value")
                .map(|v| v.id() == value_node.id())
                .unwrap_or(false);
            if !is_value {
                return;
            }
            if is_subscript_nested_in_args(parent) {
                return;
            }
            collect_subscript_type_args(extractor.base(), parent)
        }
        _ => return,
    };

    extractor.base_mut().record_type_arguments(identifier, arguments);
}

/// Returns `true` if `generic_type_node` is itself a type argument inside
/// another generic's `type_parameter`, rather than the outermost generic.
///
/// Structure of a nested case:
/// ```text
/// generic_type (outer Dict)
///   type_parameter
///     type
///       generic_type (nested List)  ← this node
/// ```
/// So a `generic_type` is nested when its parent chain is `type → type_parameter`.
fn is_generic_type_nested_in_args(generic_type_node: Node<'_>) -> bool {
    let Some(type_wrapper) = generic_type_node.parent() else {
        return false;
    };
    if type_wrapper.kind() != "type" {
        return false;
    }
    type_wrapper
        .parent()
        .map(|p| p.kind() == "type_parameter")
        .unwrap_or(false)
}

/// Returns `true` if `subscript_node` is a subscript-field arg of another
/// subscript (raw subscript context, not annotation context).
fn is_subscript_nested_in_args(subscript_node: Node<'_>) -> bool {
    subscript_node
        .parent()
        .map(|p| p.kind() == "subscript")
        .unwrap_or(false)
}

/// Collect ordered type arguments from a `generic_type` node.
///
/// ```text
/// generic_type
///   identifier("Dict")
///   type_parameter
///     type { identifier("str") }       ← arg 0
///     type { generic_type { List … } } ← arg 1 (nested)
/// ```
fn collect_generic_type_args(base: &BaseExtractor, generic_type_node: Node<'_>) -> Vec<TypeArgument> {
    // Find the type_parameter child (the `[...]` arg list).
    let mut outer_cursor = generic_type_node.walk();
    let type_param = match generic_type_node
        .named_children(&mut outer_cursor)
        .find(|n| n.kind() == "type_parameter")
    {
        Some(tp) => tp,
        None => return Vec::new(),
    };

    let mut result = Vec::new();
    let mut ordinal = 0u32;
    let mut cursor = type_param.walk();

    for type_wrapper in type_param.named_children(&mut cursor) {
        // Each named child of type_parameter is a `type` wrapper node.
        if type_wrapper.kind() != "type" {
            continue;
        }
        // The actual type expression is the single named child of the wrapper.
        let mut inner_cursor = type_wrapper.walk();
        let Some(inner) = type_wrapper.named_children(&mut inner_cursor).next() else {
            continue;
        };
        if let Some(arg) = python_type_expr_to_type_arg(base, inner, ordinal) {
            ordinal += 1;
            result.push(arg);
        }
    }
    result
}

/// Collect ordered type arguments from a raw `subscript` node.
///
/// ```text
/// subscript
///   value: identifier("Dict")
///   subscript: identifier("str")        ← repeated field, arg 0
///   subscript: subscript { List, int }  ← repeated field, arg 1
/// ```
fn collect_subscript_type_args(base: &BaseExtractor, subscript_node: Node<'_>) -> Vec<TypeArgument> {
    let mut result = Vec::new();
    let mut ordinal = 0u32;
    let mut cursor = subscript_node.walk();

    for child in subscript_node.children_by_field_name("subscript", &mut cursor) {
        if let Some(arg) = python_subscript_arg_to_type_arg(base, child, ordinal) {
            ordinal += 1;
            result.push(arg);
        }
    }
    result
}

/// Map a type expression to a `TypeArgument` — used for `generic_type` args.
fn python_type_expr_to_type_arg(
    base: &BaseExtractor,
    node: Node<'_>,
    ordinal: u32,
) -> Option<TypeArgument> {
    match node.kind() {
        "generic_type" => {
            // Nested generic: extract the name identifier and recurse.
            let mut cursor = node.walk();
            let name_node = node
                .named_children(&mut cursor)
                .find(|n| n.kind() == "identifier")?;
            let type_name = base.get_node_text(&name_node);
            let children = collect_generic_type_args(base, node);
            Some(TypeArgument {
                ordinal,
                type_name,
                children,
            })
        }
        _ => {
            // Leaf type: identifier, union_type, constrained_type, etc.
            let type_name = base.get_node_text(&node);
            Some(TypeArgument {
                ordinal,
                type_name,
                children: Vec::new(),
            })
        }
    }
}

/// Map a raw subscript arg node to a `TypeArgument`.
fn python_subscript_arg_to_type_arg(
    base: &BaseExtractor,
    node: Node<'_>,
    ordinal: u32,
) -> Option<TypeArgument> {
    if !node.is_named() {
        return None; // skip commas, brackets
    }
    match node.kind() {
        "subscript" => {
            let value = node.child_by_field_name("value")?;
            let type_name = base.get_node_text(&value);
            let children = collect_subscript_type_args(base, node);
            Some(TypeArgument {
                ordinal,
                type_name,
                children,
            })
        }
        _ => {
            let type_name = base.get_node_text(&node);
            Some(TypeArgument {
                ordinal,
                type_name,
                children: Vec::new(),
            })
        }
    }
}
