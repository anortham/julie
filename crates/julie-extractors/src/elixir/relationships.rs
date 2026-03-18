/// Relationship extraction for Elixir symbols.
///
/// Handles: use (Uses), @behaviour (Implements), defimpl (Implements), function calls (Calls).
use super::helpers;
use crate::base::{PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract all relationships from a parsed tree
pub(super) fn extract_relationships(
    extractor: &mut super::ElixirExtractor,
    tree: &tree_sitter::Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    let symbol_map: HashMap<String, &Symbol> =
        symbols.iter().map(|s| (s.name.clone(), s)).collect();

    walk_for_relationships(
        extractor,
        tree.root_node(),
        symbols,
        &symbol_map,
        &mut relationships,
    );
    relationships
}

fn walk_for_relationships(
    extractor: &mut super::ElixirExtractor,
    node: Node,
    symbols: &[Symbol],
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    match node.kind() {
        "call" => {
            if let Some(target_name) = helpers::extract_call_target_name(&extractor.base, &node) {
                match target_name.as_str() {
                    "use" => {
                        extract_use_relationship(extractor, &node, symbols, relationships);
                    }
                    "defimpl" => {
                        extract_impl_relationship(extractor, &node, symbols, relationships);
                    }
                    // Skip definition macros for call relationships
                    "defmodule" | "def" | "defp" | "defmacro" | "defmacrop" | "defprotocol"
                    | "defstruct" | "import" | "alias" | "require" => {}
                    _ => {
                        // Regular function call → Calls relationship
                        extract_call_relationship(
                            extractor,
                            &node,
                            &target_name,
                            symbol_map,
                            relationships,
                        );
                    }
                }
            }
        }
        "unary_operator" => {
            // Check for @behaviour
            extract_behaviour_relationship(extractor, &node, symbols, relationships);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_relationships(extractor, child, symbols, symbol_map, relationships);
    }
}

fn extract_use_relationship(
    extractor: &mut super::ElixirExtractor,
    node: &Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let Some(target) = helpers::extract_import_target(&extractor.base, node) else {
        return;
    };

    // Find the containing module symbol
    let containing_module = find_containing_module(extractor, node, symbols);
    let Some(from_symbol) = containing_module else {
        return;
    };

    // Try to find the used module in symbols — only match definition symbols,
    // not Import/Export symbols (which are created for the `use` statement itself)
    if let Some(to_symbol) = symbols
        .iter()
        .find(|s| s.name == target && !matches!(s.kind, SymbolKind::Import | SymbolKind::Export))
    {
        relationships.push(Relationship {
            id: format!(
                "{}_{}_Uses_{}",
                from_symbol.id,
                to_symbol.id,
                node.start_position().row
            ),
            from_symbol_id: from_symbol.id.clone(),
            to_symbol_id: to_symbol.id.clone(),
            kind: RelationshipKind::Uses,
            file_path: extractor.base.file_path.clone(),
            line_number: (node.start_position().row + 1) as u32,
            confidence: 1.0,
            metadata: None,
        });
    } else {
        // Unresolved — pending relationship for cross-file resolution
        extractor.pending_relationships.push(PendingRelationship {
            from_symbol_id: from_symbol.id.clone(),
            callee_name: target,
            kind: RelationshipKind::Uses,
            file_path: extractor.base.file_path.clone(),
            line_number: (node.start_position().row + 1) as u32,
            confidence: 0.8,
        });
    }
}

fn extract_impl_relationship(
    extractor: &super::ElixirExtractor,
    node: &Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let Some(protocol_name) = helpers::extract_impl_protocol_name(&extractor.base, node) else {
        return;
    };
    let for_type = helpers::extract_keyword_value(&extractor.base, node, "for");

    let impl_name = match &for_type {
        Some(ft) => format!("{}.{}", protocol_name, ft),
        None => protocol_name.clone(),
    };

    let from_symbol = symbols.iter().find(|s| s.name == impl_name);
    let to_symbol = symbols.iter().find(|s| s.name == protocol_name);

    if let (Some(from), Some(to)) = (from_symbol, to_symbol) {
        relationships.push(Relationship {
            id: format!(
                "{}_{}_Implements_{}",
                from.id,
                to.id,
                node.start_position().row
            ),
            from_symbol_id: from.id.clone(),
            to_symbol_id: to.id.clone(),
            kind: RelationshipKind::Implements,
            file_path: extractor.base.file_path.clone(),
            line_number: (node.start_position().row + 1) as u32,
            confidence: 1.0,
            metadata: None,
        });
    }
}

fn extract_behaviour_relationship(
    extractor: &mut super::ElixirExtractor,
    node: &Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    let Some(operator) = node.child_by_field_name("operator") else {
        return;
    };
    if extractor.base.get_node_text(&operator) != "@" {
        return;
    }

    let Some(operand) = node.child_by_field_name("operand") else {
        return;
    };
    if operand.kind() != "call" {
        return;
    }
    let Some(target) = operand.child_by_field_name("target") else {
        return;
    };
    let attr_name = extractor.base.get_node_text(&target);
    if attr_name != "behaviour" && attr_name != "behavior" {
        return;
    }

    let Some(args) = operand.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.kind() == "alias" {
            let behaviour_name = extractor.base.get_node_text(&child);
            let containing_module = find_containing_module(extractor, node, symbols);
            if let Some(from) = containing_module {
                if let Some(to) = symbols.iter().find(|s| s.name == behaviour_name) {
                    relationships.push(Relationship {
                        id: format!(
                            "{}_{}_Implements_{}",
                            from.id,
                            to.id,
                            node.start_position().row
                        ),
                        from_symbol_id: from.id.clone(),
                        to_symbol_id: to.id.clone(),
                        kind: RelationshipKind::Implements,
                        file_path: extractor.base.file_path.clone(),
                        line_number: (node.start_position().row + 1) as u32,
                        confidence: 1.0,
                        metadata: None,
                    });
                }
            }
        }
    }
}

fn extract_call_relationship(
    extractor: &mut super::ElixirExtractor,
    node: &Node,
    fn_name: &str,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    // Find the containing function
    let containing_fn = find_containing_function(extractor, node, symbol_map);
    let Some(caller) = containing_fn else {
        return;
    };

    let line_number = (node.start_position().row + 1) as u32;

    if let Some(callee) = symbol_map.get(fn_name) {
        relationships.push(Relationship {
            id: format!(
                "{}_{}_Calls_{}",
                caller.id,
                callee.id,
                node.start_position().row
            ),
            from_symbol_id: caller.id.clone(),
            to_symbol_id: callee.id.clone(),
            kind: RelationshipKind::Calls,
            file_path: extractor.base.file_path.clone(),
            line_number,
            confidence: 0.9,
            metadata: None,
        });
    } else {
        // Unresolved — pending relationship for cross-file resolution
        extractor.pending_relationships.push(PendingRelationship {
            from_symbol_id: caller.id.clone(),
            callee_name: fn_name.to_string(),
            kind: RelationshipKind::Calls,
            file_path: extractor.base.file_path.clone(),
            line_number,
            confidence: 0.7,
        });
    }
}

fn find_containing_module<'a>(
    extractor: &super::ElixirExtractor,
    node: &Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let mut current = Some(*node);
    while let Some(n) = current {
        if n.kind() == "call" {
            if let Some(target_name) = helpers::extract_call_target_name(&extractor.base, &n) {
                if target_name == "defmodule" {
                    if let Some(mod_name) = helpers::extract_module_name(&extractor.base, &n) {
                        return symbols.iter().find(|s| s.name == mod_name);
                    }
                }
            }
        }
        current = n.parent();
    }
    None
}

fn find_containing_function<'a>(
    extractor: &super::ElixirExtractor,
    node: &Node,
    symbol_map: &'a HashMap<String, &Symbol>,
) -> Option<Symbol> {
    let mut current = Some(*node);
    while let Some(n) = current {
        if n.kind() == "call" {
            if let Some(target_name) = helpers::extract_call_target_name(&extractor.base, &n) {
                if matches!(target_name.as_str(), "def" | "defp") {
                    if let Some((fn_name, _)) = helpers::extract_function_head(&extractor.base, &n)
                    {
                        if let Some(sym) = symbol_map.get(&fn_name) {
                            return Some((*sym).clone());
                        }
                    }
                }
            }
        }
        current = n.parent();
    }
    None
}
