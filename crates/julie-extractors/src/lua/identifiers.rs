use super::helpers;
/// Identifier extraction for LSP-quality find_references
///
/// Extracts all identifier usages:
/// - Function calls: `foo()`, `require("module")`
/// - Method calls with colon syntax: `obj:method()`
/// - Member access: `obj.field`, `obj.field.nested`
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use crate::lua::LuaExtractor;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages (function calls, member access, etc.)
/// Following the Rust extractor reference implementation pattern
pub(super) fn extract_identifiers(
    extractor: &mut LuaExtractor,
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
    extractor: &mut LuaExtractor,
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
    extractor: &mut LuaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Function calls: foo(), require("module")
        "function_call" => {
            // Try to get the function name from the identifier child
            if let Some(name_node) = helpers::find_child_by_type(&node, "identifier") {
                let name = extractor.base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
            // If no direct identifier, check for dot_index_expression (like math.sqrt())
            else if let Some(dot_index) =
                helpers::find_child_by_type(&node, "dot_index_expression")
            {
                // Extract the rightmost identifier (the method name)
                if let Some(_method_node) = helpers::find_child_by_type(&dot_index, "identifier") {
                    // Get all identifiers and use the last one (rightmost)
                    let mut cursor = dot_index.walk();
                    let identifiers: Vec<Node> = dot_index
                        .children(&mut cursor)
                        .filter(|c| c.kind() == "identifier")
                        .collect();

                    if let Some(last_identifier) = identifiers.last() {
                        let name = extractor.base().get_node_text(last_identifier);
                        let containing_symbol_id =
                            find_containing_symbol_id(extractor, node, symbol_map);

                        extractor.base_mut().create_identifier(
                            last_identifier,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                }
            }
            // Phase 3b: capture string-literal call-arguments config-free; the
            // carrier classification + bloat gate run later in the src/ pipeline.
            record_lua_call_arg_literals(extractor, node, symbol_map);
        }

        // Method calls with colon syntax: obj:method()
        "method_index_expression" => {
            // Extract the method name (rightmost identifier)
            let mut cursor = node.walk();
            let identifiers: Vec<Node> = node
                .children(&mut cursor)
                .filter(|c| c.kind() == "identifier")
                .collect();

            if let Some(method_node) = identifiers.last() {
                let name = extractor.base().get_node_text(method_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    method_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
        }

        // Member access with dot: obj.field, obj.field.nested
        "dot_index_expression" => {
            // Only extract if it's NOT part of a function_call or method_index_expression
            // (we handle those in the cases above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "function_call" || parent.kind() == "method_index_expression" {
                    return; // Skip - handled by function/method call
                }
            }

            // Extract the rightmost identifier (the member name)
            let mut cursor = node.walk();
            let identifiers: Vec<Node> = node
                .children(&mut cursor)
                .filter(|c| c.kind() == "identifier")
                .collect();

            if let Some(member_node) = identifiers.last() {
                let name = extractor.base().get_node_text(member_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.base_mut().create_identifier(
                    member_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {
            // Skip other node types for now
            // Future: type usage, import statements, etc.
        }
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &LuaExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    extractor
        .base()
        .find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

// ============================================================================
// String-literal call-argument capture (Miller bridge Phase 3b)
// ============================================================================

/// Capture string-literal arguments of a Lua `function_call` as `Literal`
/// records.
///
/// Config-free: `carrier` is the verbatim callee — a bare `identifier`
/// (`load`), or the `table.field`/`table.method` join for a
/// `dot_index_expression` (`http.request`) / `method_index_expression`
/// (`conn:execute` → `conn.execute`). `kind` stays `Other`; the `src/` carrier
/// gate sets the authoritative kind and drops non-carrier literals.
/// `arg_position` counts over the full argument list.
fn record_lua_call_arg_literals(
    extractor: &mut LuaExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(args_node) = call_node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = lua_carrier(extractor.base(), call_node);
    let containing_symbol_id = find_containing_symbol_id(extractor, call_node, symbol_map);

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

/// Derive a Lua `function_call`'s carrier from its `name` field.
///
/// `identifier` → bare name. `dot_index_expression` (`http.request`) →
/// `table.field`. `method_index_expression` (`conn:execute`) → `table.method`
/// (joined with `.` so the gate's last-segment rule matches a bare `execute`
/// config and a dotted `http.request` config matches exactly).
fn lua_carrier(base: &BaseExtractor, call_node: Node) -> Option<String> {
    let name = call_node.child_by_field_name("name")?;
    match name.kind() {
        "identifier" => Some(base.get_node_text(&name)),
        "dot_index_expression" => join_receiver_member(
            name.child_by_field_name("table")
                .map(|n| base.get_node_text(&n)),
            name.child_by_field_name("field")
                .map(|n| base.get_node_text(&n)),
        ),
        "method_index_expression" => join_receiver_member(
            name.child_by_field_name("table")
                .map(|n| base.get_node_text(&n)),
            name.child_by_field_name("method")
                .map(|n| base.get_node_text(&n)),
        ),
        _ => {
            let text = base.get_node_text(&name);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

/// Join a `receiver` and `member` into a `receiver.member` carrier, tolerating a
/// missing receiver.
fn join_receiver_member(receiver: Option<String>, member: Option<String>) -> Option<String> {
    match (receiver, member) {
        (Some(r), Some(m)) => Some(format!("{r}.{m}")),
        (None, Some(m)) => Some(m),
        _ => None,
    }
}
