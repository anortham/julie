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
                    let parent_is_args = node
                        .parent()
                        .is_some_and(|p| p.kind() == "arguments" && p.parent().is_some_and(|pp| pp.id() == n.id()));
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
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == base.file_path)
        .map(|&s| s.clone())
        .collect();

    base.find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}
