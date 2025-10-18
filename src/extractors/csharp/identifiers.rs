// C# Identifier Extraction

use crate::extractors::base::{BaseExtractor, Identifier, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract all identifier usages
pub fn extract_identifiers(
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
        "invocation_expression" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    let name = base.get_node_text(&child);
                    let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                    base.create_identifier(
                        &child,
                        name,
                        IdentifierKind::Call,
                        containing_symbol_id,
                    );
                    break;
                } else if child.kind() == "member_access_expression" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = base.get_node_text(&name_node);
                        let containing_symbol_id =
                            find_containing_symbol_id(base, node, symbol_map);
                        base.create_identifier(
                            &name_node,
                            name,
                            IdentifierKind::Call,
                            containing_symbol_id,
                        );
                    }
                    break;
                }
            }
        }
        "member_access_expression" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "invocation_expression" {
                    return;
                }
            }

            if let Some(name_node) = node.child_by_field_name("name") {
                let name = base.get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(base, node, symbol_map);
                base.create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }
        _ => {}
    }
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
