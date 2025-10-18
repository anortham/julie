use super::functions;
use super::tables;
use super::variables;
/// Core symbol extraction and tree traversal
///
/// Handles the main tree traversal logic and dispatches to appropriate
/// extraction functions based on node types.
use crate::extractors::base::{BaseExtractor, Symbol};
use tree_sitter::Node;

/// Recursively traverse the tree and extract symbols
pub(super) fn traverse_tree(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) {
    let mut symbol: Option<Symbol> = None;

    match node.kind() {
        "function_definition_statement" | "function_declaration" => {
            symbol = functions::extract_function_definition_statement(
                symbols,
                base,
                node,
                parent_id.as_deref(),
            );
        }
        "local_function_definition_statement" | "local_function_declaration" => {
            symbol = functions::extract_local_function_definition_statement(
                symbols,
                base,
                node,
                parent_id.as_deref(),
            );
        }
        "local_variable_declaration" | "variable_declaration" => {
            symbol = variables::extract_local_variable_declaration(
                symbols,
                base,
                node,
                parent_id.as_deref(),
            );
        }
        "assignment_statement" => {
            symbol =
                variables::extract_assignment_statement(symbols, base, node, parent_id.as_deref());
        }
        "variable_assignment" => {
            symbol =
                variables::extract_variable_assignment(symbols, base, node, parent_id.as_deref());
        }
        "table_constructor" | "table" => {
            // Table constructors can contain fields that should be extracted as child symbols
            tables::extract_table_fields(symbols, base, node, parent_id.as_deref());
            return; // Table constructor itself doesn't create a symbol, just its fields
        }
        _ => {}
    }

    // Traverse children with current symbol as parent (if extracted) or keep same parent
    let current_parent_id = symbol.as_ref().map(|s| s.id.clone()).or(parent_id);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_tree(symbols, base, child, current_parent_id.clone());
    }
}
