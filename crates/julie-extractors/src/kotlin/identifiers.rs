//! Identifier and reference extraction for Kotlin
//!
//! This module handles extraction of function calls, member access, and other
//! identifier usages for LSP-quality find_references support.

use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol, extract_type_arguments};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract all identifier usages from a Kotlin file
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();

    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);

    base.identifiers.clone()
}

/// Recursively walk tree extracting identifiers from each node
fn walk_tree_for_identifiers(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    extract_identifier_from_node(base, node, symbol_map);

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
        // Function/method calls: foo(), bar.baz(), mutableListOf<User>()
        "call_expression" => {
            // Collect children once so we can find both the callee and type_arguments
            // without keeping the cursor borrow alive across mutable base calls.
            let children: Vec<_> = {
                let mut cursor = node.walk();
                node.children(&mut cursor).collect()
            };

            let type_args_node = children
                .iter()
                .find(|c| c.kind() == "type_arguments")
                .copied();

            // Simple call: identifier or simple_identifier is the first callee child.
            if let Some(child) = children
                .iter()
                .find(|c| c.kind() == "identifier" || c.kind() == "simple_identifier")
            {
                let arguments = type_args_node
                    .map(|ta| extract_type_arguments(base, ta, decompose_kotlin_type_arg));
                let name = base.get_node_text(child);
                let containing = find_containing_symbol_id(base, node, symbol_map);
                let identifier =
                    base.create_identifier(child, name, IdentifierKind::Call, containing);
                if let Some(args) = arguments {
                    if !args.is_empty() {
                        base.record_type_arguments(&identifier, args);
                    }
                }
            } else if let Some(nav_expr) = children
                .iter()
                .find(|c| c.kind() == "navigation_expression")
            {
                // Member call: obj.foo<T>()
                let nav_name = extract_rightmost_identifier(base, nav_expr);
                let arguments = type_args_node
                    .map(|ta| extract_type_arguments(base, ta, decompose_kotlin_type_arg));
                let containing = find_containing_symbol_id(base, node, symbol_map);
                if let Some((name_node, name)) = nav_name {
                    let identifier =
                        base.create_identifier(&name_node, name, IdentifierKind::Call, containing);
                    if let Some(args) = arguments {
                        if !args.is_empty() {
                            base.record_type_arguments(&identifier, args);
                        }
                    }
                }
            }
            // Phase 3b: capture string-literal call-arguments (config-free;
            // carrier classification + gate run later in the src/ pipeline).
            record_kotlin_call_arg_literals(base, node, symbol_map);
        }

        // Type references in type positions: val x: Foo, fun f(a: Foo): Bar,
        // class Foo(service: Bar), typealias A = Foo
        // Kotlin uses `user_type` for type annotations. It contains an
        // `identifier` child for the type name. Unlike Scala/Java,
        // class/interface/object declaration names use `identifier`
        // directly (not inside `user_type`), so we don't need to filter
        // declaration names here.
        "user_type" => {
            // Extract the first identifier child — that's the type name.
            // Kotlin tree-sitter uses `identifier` (not `simple_identifier`)
            // inside `user_type` nodes.
            let name_node = node
                .children(&mut node.walk())
                .find(|n| n.kind() == "identifier" || n.kind() == "simple_identifier");

            if let Some(name_node) = name_node {
                let name = base.get_node_text(&name_node);

                if is_kotlin_noise_type(&name) {
                    return;
                }

                let containing = find_containing_symbol_id(base, node, symbol_map);
                let identifier =
                    base.create_identifier(&name_node, name, IdentifierKind::TypeUsage, containing);
                // If this user_type is the outermost generic use site (not nested
                // inside another type_arguments list), record its ordered type args.
                record_outermost_kotlin_type_arguments(base, node, &identifier);
            }
        }

        // Member access: object.property
        "navigation_expression" => {
            // Only extract if it's NOT part of a call_expression
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_expression" {
                    return;
                }
            }

            // Extract the rightmost identifier (the member name)
            if let Some((name_node, name)) = extract_rightmost_identifier(base, &node) {
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);

                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {
            // Skip other node types
        }
    }
}

/// Record outermost generic type arguments for a `user_type` node.
///
/// Fires when the `user_type` is an outermost generic use site (e.g. `List` in
/// `List<User>`), but not when it is nested inside another generic's
/// `type_arguments` (where it rides along as a `child`).
fn record_outermost_kotlin_type_arguments(
    base: &mut BaseExtractor,
    user_type_node: Node,
    identifier: &Identifier,
) {
    // Skip if this user_type is nested inside another type_arguments.
    if is_kotlin_user_type_nested(user_type_node) {
        return;
    }
    // Find the type_arguments child (e.g. `<User>` in `List<User>`).
    let children: Vec<_> = {
        let mut cursor = user_type_node.walk();
        user_type_node.children(&mut cursor).collect()
    };
    let Some(arg_list) = children.into_iter().find(|c| c.kind() == "type_arguments") else {
        return;
    };
    let arguments = extract_type_arguments(base, arg_list, decompose_kotlin_type_arg);
    base.record_type_arguments(identifier, arguments);
}

/// Returns true if `user_type` is nested inside a `type_projection` (i.e., it
/// is a type argument of some outer generic, not an outermost use site).
///
/// In Kotlin the nesting path is:
/// `outer_user_type > type_arguments > type_projection > [nullable_type >]* user_type`
fn is_kotlin_user_type_nested(user_type: Node) -> bool {
    let mut current = user_type;
    loop {
        let Some(parent) = current.parent() else {
            return false;
        };
        match parent.kind() {
            "type_projection" => return true,
            // Transparent type wrappers — keep climbing.
            "nullable_type" | "parenthesized_type" | "non_nullable_type" => {
                current = parent;
            }
            _ => return false,
        }
    }
}

/// Decompose a single child of a Kotlin `type_arguments` node.
///
/// Kotlin always wraps each argument in `type_projection`:
/// `type_arguments { type_projection { [variance_modifier,] type } ... }`
///
/// Returns `(type_name, Option<nested_type_arguments_node>)`.
fn decompose_kotlin_type_arg<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    if !node.is_named() {
        return None; // skip commas and angle brackets
    }
    if node.kind() != "type_projection" {
        return None;
    }
    // Find the actual type node inside type_projection (skip variance_modifier).
    let type_node = {
        let children: Vec<Node<'a>> = {
            let mut cursor = node.walk();
            node.children(&mut cursor).collect()
        };
        children
            .into_iter()
            .find(|c| c.is_named() && c.kind() != "variance_modifier")
    };
    let Some(type_node) = type_node else {
        return Some(("*".to_string(), None)); // star projection
    };
    extract_kotlin_type_node_info(base, type_node)
}

/// Recursively extract `(type_name, nested_type_arguments)` from a type node.
fn extract_kotlin_type_node_info<'a>(
    base: &BaseExtractor,
    node: Node<'a>,
) -> Option<(String, Option<Node<'a>>)> {
    match node.kind() {
        "user_type" => {
            let children: Vec<Node<'a>> = {
                let mut cursor = node.walk();
                node.children(&mut cursor).collect()
            };
            let name = children
                .iter()
                .find(|c| c.kind() == "identifier" || c.kind() == "simple_identifier")
                .map(|n| base.get_node_text(n))
                .unwrap_or_else(|| base.get_node_text(&node));
            let nested = children.into_iter().find(|c| c.kind() == "type_arguments");
            Some((name, nested))
        }
        "nullable_type" => {
            // `Foo?` — unwrap the inner type and append "?".
            let mut cursor = node.walk();
            let inner = node.named_children(&mut cursor).next();
            if let Some(inner) = inner {
                extract_kotlin_type_node_info(base, inner)
                    .map(|(name, nested)| (format!("{}?", name), nested))
            } else {
                Some((base.get_node_text(&node), None))
            }
        }
        _ => Some((base.get_node_text(&node), None)),
    }
}

/// Find the ID of the symbol that contains this node
fn find_containing_symbol_id(
    base: &BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    base.find_containing_symbol_from_map(&node, symbol_map)
        .map(|s| s.id.clone())
}

// ============================================================================
// String-literal call-argument capture (Miller bridge Phase 3b)
// ============================================================================

/// Capture string-literal arguments of a Kotlin `call_expression` as `Literal`
/// records.
///
/// Config-free: `carrier` is the verbatim callee text; the URL/SQL
/// classification and the carrier gate run later in the `src/` pipeline.
/// Kotlin call args live in a `value_arguments` child holding `value_argument`
/// nodes; a named argument (`url = "..."`) carries an extra `identifier` name,
/// so the value is the argument's last named child. `arg_position` is counted
/// over the full argument list. Kotlin string templates (`"$x"` / `"${x}"`)
/// decode to `{}` holes via the shared `interpolation`-aware decoder.
fn record_kotlin_call_arg_literals(
    base: &mut BaseExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let children: Vec<Node> = {
        let mut cursor = call_node.walk();
        call_node.children(&mut cursor).collect()
    };
    let Some(value_args) = children
        .iter()
        .find(|c| c.kind() == "value_arguments")
        .copied()
    else {
        return;
    };
    let callee = children
        .iter()
        .find(|c| {
            matches!(
                c.kind(),
                "identifier" | "simple_identifier" | "navigation_expression"
            )
        })
        .copied();
    let carrier = callee.and_then(|c| kotlin_carrier(base, c));
    let containing_symbol_id = find_containing_symbol_id(base, call_node, symbol_map);

    let args: Vec<Node> = {
        let mut cursor = value_args.walk();
        value_args.named_children(&mut cursor).collect()
    };
    for (pos, arg) in args.into_iter().enumerate() {
        if let Some(value) = kotlin_argument_value(arg) {
            if let Some(text) = base.decode_string_literal(&value) {
                base.record_literal(
                    &value,
                    text,
                    carrier.clone(),
                    pos as u32,
                    containing_symbol_id.clone(),
                );
            }
        }
    }
}

/// The value expression of a Kotlin `value_argument`. A named argument
/// (`name = expr`) has the name as a leading `identifier`, so the value is the
/// last named child; a positional argument's single named child is the value.
fn kotlin_argument_value(arg: Node) -> Option<Node> {
    if arg.kind() != "value_argument" {
        return Some(arg);
    }
    let mut cursor = arg.walk();
    arg.named_children(&mut cursor).last()
}

/// Derive a Kotlin call's carrier from its callee.
///
/// Plain `identifier`/`simple_identifier` → its text (`fetch`). A
/// `navigation_expression` (`db.execute`, `client.get`) → the `receiver.member`
/// join so dotted client APIs match config (`client.get`) while bare DB verbs
/// (`execute`/`query`) match any receiver via the gate's last-segment rule.
fn kotlin_carrier(base: &BaseExtractor, callee: Node) -> Option<String> {
    match callee.kind() {
        "identifier" | "simple_identifier" => Some(base.get_node_text(&callee)),
        "navigation_expression" => {
            let named: Vec<Node> = {
                let mut cursor = callee.walk();
                callee.named_children(&mut cursor).collect()
            };
            let receiver = named.first().map(|n| base.get_node_text(n));
            let member = named.last().map(|n| base.get_node_text(n));
            match (receiver, member) {
                (Some(r), Some(m)) if named.len() >= 2 => Some(format!("{r}.{m}")),
                (_, Some(m)) => Some(m),
                _ => None,
            }
        }
        _ => {
            let text = base.get_node_text(&callee);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

/// Returns true for Kotlin types that are too common to be meaningful
/// type references for centrality scoring.
///
/// Includes:
/// - Single-letter type params (T, K, V, E, R) — generic type parameters
/// - Kotlin/JVM primitive and base types — ubiquitous in every file
fn is_kotlin_noise_type(name: &str) -> bool {
    // Single-letter uppercase names are almost always generic type parameters.
    if name.len() == 1
        && name
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_uppercase())
    {
        return true;
    }

    matches!(
        name,
        // Kotlin primitive types
        "Int"
            | "Long"
            | "Short"
            | "Byte"
            | "Float"
            | "Double"
            | "Char"
            | "Boolean"
            | "Unit"
            // Kotlin top types
            | "Any"
            | "Nothing"
            // JVM interop
            | "String"
            | "Object"
    )
}

/// Helper to extract the rightmost identifier in a navigation_expression
fn extract_rightmost_identifier<'a>(
    base: &BaseExtractor,
    node: &Node<'a>,
) -> Option<(Node<'a>, String)> {
    // Kotlin navigation_expression structure
    // For chained access like user.account.balance:
    // - We need to find the rightmost identifier

    // First, try to find identifier children (rightmost in chain)
    let identifiers: Vec<Node> = node
        .children(&mut node.walk())
        .filter(|n| n.kind() == "identifier" || n.kind() == "simple_identifier")
        .collect();

    if let Some(last_identifier) = identifiers.last() {
        let name = base.get_node_text(last_identifier);
        return Some((*last_identifier, name));
    }

    None
}
