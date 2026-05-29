use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, extract_type_arguments};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages (function calls, member access, etc.)
/// Following the Rust extractor reference implementation pattern
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    // Create symbol map for fast lookup
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    // Walk the tree and extract identifiers
    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);

    // Return the collected identifiers
    base.identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    // Extract identifier from this node if applicable
    extract_identifier_from_node(base, node, symbol_map);

    // Recursively walk children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_for_identifiers(base, child, symbol_map);
    }
}

/// Extract identifier from a single node based on its kind
fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function calls: calculate(), obj.method()
        // Also handles scoped Zig generic type applications: `var x: ArrayList(User)`.
        // In Zig, generics are comptime functions and share `call_expression` with
        // regular calls. Type-arg capture is gated by `is_zig_call_in_type_position`.
        "call_expression" => {
            // Try to get the function name from direct identifier child
            if let Some(name_node) = base.find_child_by_type(&node, "identifier") {
                let name = base.get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                let identifier = base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );

                // Scoped generic capture: only for calls in a type-annotation position.
                // Zig has no `arguments` wrapper — args are direct named children of
                // call_expression. Pass the call_expression itself as the arg_list;
                // the decomposer skips the function identifier via the `function` field check.
                if is_zig_call_in_type_position(node) {
                    let arguments = extract_type_arguments(base, node, decompose_zig_type_arg);
                    base.record_type_arguments(&identifier, arguments);
                }
            }
            // Check for field_expression (method calls like obj.method())
            else if let Some(field_expr) = base.find_child_by_type(&node, "field_expression") {
                // Extract the rightmost identifier (the method name)
                let mut cursor = field_expr.walk();
                let identifiers: Vec<Node> = field_expr
                    .children(&mut cursor)
                    .filter(|c| c.kind() == "identifier")
                    .collect();

                if let Some(last_identifier) = identifiers.last() {
                    let name = base.get_node_text(last_identifier);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                    base.create_identifier(
                        last_identifier,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                }
            }
        }

        // Member access: point.x, user.account.balance
        "field_expression" => {
            // Only extract if it's NOT part of a call_expression
            // (we handle those in the call_expression case above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    return; // Skip - handled by call_expression
                }
            }

            // Extract the rightmost identifier (the member name)
            let mut cursor = node.walk();
            let identifiers: Vec<Node> = node
                .children(&mut cursor)
                .filter(|c| c.kind() == "identifier")
                .collect();

            if let Some(member_node) = identifiers.last() {
                let name = base.get_node_text(member_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    member_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        // Type annotations in Zig: field types, parameter types, return types, var types.
        // Zig uses plain identifiers after `:` for types — no wrapper `type` node.
        // We detect type position by checking the parent node context.
        //
        // Patterns:
        //   container_field: `name: Type` → identifier after `:` is a type
        //   parameter: `param: *Type` → identifier inside pointer_type/optional_type
        //   variable_declaration: `var x: Type` → identifier after `:`
        //   error_union_type: `!Type` → identifier child is a type
        //   pointer_type: `*Type` → identifier child is a type
        //   optional_type: `?Type` → identifier child is a type
        "identifier" => {
            if let Some(parent) = node.parent() {
                let is_type_position = match parent.kind() {
                    // *Server, []*Server
                    "pointer_type" | "optional_type" => true,
                    // !DocumentStore (error union return type)
                    "error_union_type" => true,
                    // document_store: DocumentStore (field or param type)
                    // var store: DocumentStore (variable type)
                    "container_field" | "parameter" | "variable_declaration" => {
                        // Only the identifier AFTER `:` is the type, not before it (the name)
                        is_after_colon(parent, node)
                    }
                    _ => false,
                };

                if is_type_position {
                    let name = base.get_node_text(&node);
                    // Skip builtin types and keywords
                    if !is_zig_builtin_type(&name) {
                        let containing_symbol_id =
                            find_containing_symbol_id(base, node, symbol_map);
                        base.create_identifier(
                            &node,
                            name,
                            IdentifierKind::TypeUsage,
                            containing_symbol_id,
                        );
                    }
                }
            }
        }

        _ => {}
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    base.find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

/// Check if `child` appears after a `:` token in `parent`.
/// In Zig, `name: Type` means the identifier after `:` is a type, before is the name.
fn is_after_colon(parent: Node, child: Node) -> bool {
    let mut saw_colon = false;
    let mut cursor = parent.walk();
    for sibling in parent.children(&mut cursor) {
        if sibling.kind() == ":" {
            saw_colon = true;
        }
        if sibling.id() == child.id() {
            return saw_colon;
        }
    }
    false
}

// ============================================================================
// Type-argument capture helpers (Miller bridge Phase 2, scoped)
// ============================================================================

/// Returns `true` if this `call_expression` is in a type-annotation position.
///
/// Zig generics are comptime functions (`ArrayList(i32)`) and parse as
/// `call_expression`, indistinguishable from regular calls at the grammar level.
/// This heuristic gates type-arg capture to calls that serve as the type in:
///   - `variable_declaration`: `var x: ArrayList(T) = ...`
///   - `container_field`: `field: ArrayList(T),`
///   - `parameter`: `fn f(x: ArrayList(T)) ...`
///   - type-wrapper nodes (`pointer_type`, `optional_type`, `nullable_type`,
///     `error_union_type`) that are themselves in a type position — recurse.
fn is_zig_call_in_type_position(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "variable_declaration" | "container_field" | "parameter" => is_after_colon(parent, node),
        // Pointer/optional/nullable wrappers — recurse to check their parent context.
        "pointer_type" | "optional_type" | "nullable_type" | "error_union_type" | "slice_type"
        | "array_type" => is_zig_call_in_type_position(parent),
        _ => false,
    }
}

/// `TypeArgDecomposer` for Zig: maps a child of a `call_expression` arg-list to
/// its applied argument.
///
/// **Important Zig grammar detail**: unlike TypeScript or C#, Zig's
/// `call_expression` has NO `arguments` wrapper node — argument expressions are
/// direct children alongside `(`, `,`, `)`. We therefore pass the
/// `call_expression` itself as the `arg_list_node` to `extract_type_arguments`.
///
/// This means the decomposer receives the FUNCTION identifier as its first named
/// child. We detect and skip it via the `function` field of the parent.
///
/// For nested generics (`ArrayList(User)` inside `Map(Key, ArrayList(User))`),
/// the inner `call_expression` is itself passed as the `arg_list_node` for
/// recursion — the same function-skip logic applies.
fn decompose_zig_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip (, ), commas
    }

    // Skip the `function` field identifier of the parent call_expression.
    // This fires for the outermost function name (e.g. `ArrayList` in `ArrayList(User)`)
    // and for each nested generic's own function name.
    if let Some(parent) = node.parent() {
        if parent.kind() == "call_expression" {
            if parent
                .child_by_field_name("function")
                .map(|f| f.id() == node.id())
                .unwrap_or(false)
            {
                return None;
            }
        }
    }

    match node.kind() {
        "identifier" => Some((base.get_node_text(&node), None)),
        "call_expression" => {
            // Nested generic: e.g. `ArrayList(User)` inside `Map(Key, ArrayList(User))`.
            // Use the `function` field for the base name; pass the call_expression itself
            // as the nested arg_list (the same decomposer skips its own function identifier).
            let name = node
                .child_by_field_name("function")
                .map(|f| base.get_node_text(&f))
                .unwrap_or_else(|| base.get_node_text(&node));
            Some((name, Some(node)))
        }
        _ => {
            // comptime_int, builtin_type, etc. — source text as a leaf.
            let text = base.get_node_text(&node);
            if text.is_empty() {
                None
            } else {
                Some((text, None))
            }
        }
    }
}

/// Skip Zig builtin primitive types that would create noise.
fn is_zig_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "void"
            | "bool"
            | "noreturn"
            | "anyerror"
            | "anytype"
            | "undefined"
            | "null"
            | "anyopaque"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f16"
            | "f32"
            | "f64"
            | "f80"
            | "f128"
            | "comptime_int"
            | "comptime_float"
    )
}
