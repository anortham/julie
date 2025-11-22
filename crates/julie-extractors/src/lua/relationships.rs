use crate::base::{BaseExtractor, PendingRelationship, RelationshipKind, Symbol, SymbolKind};
use crate::lua::{helpers, LuaExtractor};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Extract relationships such as function call edges from the Lua AST.
pub(super) fn extract_relationships(
    extractor: &mut LuaExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) {
    let symbol_map: HashMap<&str, &Symbol> = symbols
        .iter()
        .filter(|symbol| matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method))
        .map(|symbol| (symbol.name.as_str(), symbol))
        .collect();

    traverse_tree_for_relationships(extractor, tree.root_node(), &symbol_map, symbols);
}

fn traverse_tree_for_relationships<'a>(
    extractor: &mut LuaExtractor,
    node: Node<'a>,
    symbol_map: &HashMap<&'a str, &'a Symbol>,
    symbols: &[Symbol],
) {
    if node.kind() == "function_call" {
        // Handle simple function calls: foo()
        if let Some(identifier) = helpers::find_child_by_type(node, "identifier") {
            let callee_name = extractor.base().get_node_text(&identifier);
            process_function_call(extractor, node, &callee_name, symbol_map);
        }
        // Handle method calls: obj:method() or obj.method()
        else if let Some(method_expr) = helpers::find_child_by_type(node, "method_index_expression")
            .or_else(|| helpers::find_child_by_type(node, "dot_index_expression"))
        {
            let full_expr = extractor.base().get_node_text(&method_expr);
            // Extract the method name (everything after : or .)
            let method_name = if let Some(colon_pos) = full_expr.rfind(':') {
                &full_expr[colon_pos + 1..]
            } else if let Some(dot_pos) = full_expr.rfind('.') {
                &full_expr[dot_pos + 1..]
            } else {
                &full_expr
            };
            process_function_call(extractor, node, method_name, symbol_map);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_tree_for_relationships(extractor, child, symbol_map, symbols);
    }
}

fn process_function_call(
    extractor: &mut LuaExtractor,
    node: Node,
    callee_name: &str,
    symbol_map: &HashMap<&str, &Symbol>,
) {
    if let Some(caller_symbol) = find_enclosing_function(node, extractor.base(), symbol_map) {
        match symbol_map.get(callee_name) {
            Some(callee_symbol) => {
                // Target is a local function - create resolved Relationship
                if caller_symbol.id != callee_symbol.id {
                    let relationship = extractor.base().create_relationship(
                        caller_symbol.id.clone(),
                        callee_symbol.id.clone(),
                        RelationshipKind::Calls,
                        &node,
                        Some(0.9),
                        None,
                    );
                    extractor.relationships.push(relationship);
                }
            }
            None => {
                // Target not found in local symbols - likely a cross-file call
                // Create PendingRelationship for cross-file resolution
                let file_path = extractor.base().file_path.clone();
                let pending = PendingRelationship {
                    from_symbol_id: caller_symbol.id.clone(),
                    callee_name: callee_name.to_string(),
                    kind: RelationshipKind::Calls,
                    file_path,
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 0.7,
                };
                extractor.add_pending_relationship(pending);
            }
        }
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
