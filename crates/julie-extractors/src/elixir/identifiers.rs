/// Identifier extraction for Elixir — LSP-quality find_references support.
///
/// Walks the tree to find: function calls, module references (aliases),
/// and qualified calls (Module.function).
use crate::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages from parsed Elixir source
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Identifier> {
    let symbol_map: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.clone(), s)).collect();
    walk_tree_for_identifiers(base, tree.root_node(), &symbol_map);
    base.identifiers.clone()
}

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

fn extract_identifier_from_node(
    base: &mut BaseExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        "call" => {
            // Check if this is a definition macro — skip those
            if let Some(target) = node.child_by_field_name("target") {
                if target.kind() == "identifier" {
                    let name = base.get_node_text(&target);
                    if is_definition_keyword(&name) {
                        return;
                    }
                    // Regular function call
                    let containing = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(&target, name, IdentifierKind::Call, containing);
                }
            }
            // Phase 3b: capture string-literal call-arguments config-free; the
            // carrier classification + bloat gate run later in the src/ pipeline.
            record_elixir_call_arg_literals(base, node, symbol_map);
        }
        "dot" => {
            // Qualified call: Module.function
            // The dot node has a left (module) and right (function) child
            if let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) {
                // Module reference
                if left.kind() == "alias" {
                    let module_name = base.get_node_text(&left);
                    let containing = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(
                        &left,
                        module_name,
                        IdentifierKind::TypeUsage,
                        containing.clone(),
                    );

                    // Function reference
                    if right.kind() == "identifier" {
                        let fn_name = base.get_node_text(&right);
                        base.create_identifier(
                            &right,
                            fn_name,
                            IdentifierKind::MemberAccess,
                            containing,
                        );
                    }
                }
            }
        }
        "alias" => {
            // Standalone module reference (not part of a definition)
            if !is_in_definition_context(&node) {
                let name = base.get_node_text(&node);
                let containing = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(&node, name, IdentifierKind::TypeUsage, containing);
            }
        }
        _ => {}
    }
}

fn is_definition_keyword(name: &str) -> bool {
    matches!(
        name,
        "defmodule"
            | "def"
            | "defp"
            | "defmacro"
            | "defmacrop"
            | "defprotocol"
            | "defimpl"
            | "defstruct"
            | "defguard"
            | "defguardp"
            | "defdelegate"
            | "defexception"
            | "defoverridable"
            | "import"
            | "use"
            | "alias"
            | "require"
    )
}

fn is_in_definition_context(node: &Node) -> bool {
    let mut current = Some(*node);
    while let Some(n) = current {
        if n.kind() == "call" {
            if let Some(target) = n.child_by_field_name("target") {
                if target.kind() == "identifier" {
                    // Check if the alias is a direct argument of a definition call
                    let parent_is_args = node.parent().is_some_and(|p| {
                        p.kind() == "arguments" && p.parent().is_some_and(|pp| pp.id() == n.id())
                    });
                    if parent_is_args {
                        return true;
                    }
                }
            }
        }
        current = n.parent();
    }
    false
}

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

/// Capture string-literal arguments of an Elixir `call` as `Literal` records.
///
/// Config-free: `carrier` is the verbatim callee — the bare function name for an
/// `identifier` target (`query`), or the `Module.function` join for a `dot`
/// target (`HTTPoison.get`, `Repo.query`). `kind` stays `Other`; the `src/`
/// carrier gate sets the authoritative kind and drops non-carrier literals.
/// `arg_position` counts over the full argument list. Definition macros
/// (`def`/`defp`/…) are skipped — their "arguments" are heads/bodies, not call
/// args.
fn record_elixir_call_arg_literals(
    base: &mut BaseExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    let Some(target) = call_node.child_by_field_name("target") else {
        return;
    };
    if target.kind() == "identifier" && is_definition_keyword(&base.get_node_text(&target)) {
        return;
    }
    // The argument list is a `arguments` CHILD node of the call (not a field).
    let args_node = {
        let mut cursor = call_node.walk();
        call_node
            .named_children(&mut cursor)
            .find(|n| n.kind() == "arguments")
    };
    let Some(args_node) = args_node else {
        return;
    };
    let carrier = elixir_carrier(base, target);
    let containing_symbol_id = find_containing_symbol_id(base, call_node, symbol_map);

    let mut cursor = args_node.walk();
    for (pos, arg) in args_node.named_children(&mut cursor).enumerate() {
        // Keyword args hold the literal in a `value` field; positional string
        // args have no `value` field, so fall back to the arg itself.
        let value = arg.child_by_field_name("value").unwrap_or(arg);
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

/// Derive an Elixir call's carrier from its `target`.
///
/// `identifier` target → bare function name. `dot` target → `Module.function`
/// (`left.right`) so qualified client APIs match config exactly
/// (`HTTPoison.get`) and module receivers still match a bare method config
/// (`query`) via the gate's last-segment rule (`Repo.query` → `query`).
fn elixir_carrier(base: &BaseExtractor, target: Node) -> Option<String> {
    match target.kind() {
        "identifier" => Some(base.get_node_text(&target)),
        "dot" => {
            let left = target
                .child_by_field_name("left")
                .map(|n| base.get_node_text(&n));
            let right = target
                .child_by_field_name("right")
                .map(|n| base.get_node_text(&n));
            match (left, right) {
                (Some(l), Some(r)) => Some(format!("{l}.{r}")),
                (None, Some(r)) => Some(r),
                _ => None,
            }
        }
        _ => {
            let text = base.get_node_text(&target);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}
