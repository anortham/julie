use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
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
        "call_expression" => {
            // Try to get the function name from direct identifier child
            if let Some(name_node) = base.find_child_by_type(&node, "identifier") {
                let name = base.get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
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
    // CRITICAL FIX: Only search symbols from THIS FILE, not all files
    // Bug was: searching all symbols in DB caused wrong file symbols to match
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == base.file_path)
        .map(|&s| s.clone())
        .collect();

    base.find_containing_symbol(&node, &file_symbols)
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

/// Skip Zig builtin primitive types that would create noise.
fn is_zig_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "void" | "bool" | "noreturn" | "anyerror" | "anytype" | "undefined" | "null" | "anyopaque"
            | "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
            | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
            | "f16" | "f32" | "f64" | "f80" | "f128"
            | "comptime_int" | "comptime_float"
    )
}
