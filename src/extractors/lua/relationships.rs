use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::extractors::lua::helpers;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract relationships such as function call edges from the Lua AST.
pub(super) fn extract_relationships(
    relationships: &mut Vec<Relationship>,
    base: &BaseExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) {
    let symbol_map: HashMap<&str, &Symbol> = symbols
        .iter()
        .filter(|symbol| matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method))
        .map(|symbol| (symbol.name.as_str(), symbol))
        .collect();

    traverse_tree_for_relationships(relationships, base, tree.root_node(), &symbol_map);
}

fn traverse_tree_for_relationships<'a>(
    relationships: &mut Vec<Relationship>,
    base: &BaseExtractor,
    node: Node<'a>,
    symbol_map: &HashMap<&'a str, &'a Symbol>,
) {
    if node.kind() == "function_call" {
        if let Some(identifier) = helpers::find_child_by_type(node, "identifier") {
            let callee_name = base.get_node_text(&identifier);

            if let Some(callee_symbol) = symbol_map.get(callee_name.as_str()) {
                if let Some(caller_symbol) = find_enclosing_function(node, base, symbol_map) {
                    if caller_symbol.id != callee_symbol.id {
                        relationships.push(base.create_relationship(
                            caller_symbol.id.clone(),
                            callee_symbol.id.clone(),
                            RelationshipKind::Calls,
                            &node,
                            Some(0.9),
                            None,
                        ));
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_tree_for_relationships(relationships, base, child, symbol_map);
    }
}

fn find_enclosing_function<'a>(
    mut node: Node<'a>,
    base: &BaseExtractor,
    symbol_map: &HashMap<&'a str, &'a Symbol>,
) -> Option<&'a Symbol> {
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "function_declaration"
            | "function_definition_statement"
            | "local_function_declaration"
            | "local_function_definition_statement" => {
                if let Some(identifier) = helpers::find_child_by_type(parent, "identifier") {
                    let caller_name = base.get_node_text(&identifier);
                    if let Some(symbol) = symbol_map.get(caller_name.as_str()) {
                        return Some(*symbol);
                    }
                }
            }
            _ => {}
        }
        node = parent;
    }
    None
}
