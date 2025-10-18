// PHP Extractor - Identifier extraction (function calls, member access)

use super::PhpExtractor;
use crate::extractors::base::{IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract identifier from a single node based on its kind
pub(super) fn extract_identifier_from_node(
    extractor: &mut PhpExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) {
    match node.kind() {
        // Direct function calls: print_r(), array_map()
        "function_call_expression" => {
            // The function field contains the function being called
            if let Some(function_node) = node.child_by_field_name("function") {
                let name = extractor.get_base().get_node_text(&function_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.get_base_mut().create_identifier(
                    &function_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
        }

        // Method calls: $this->add(), $obj->method()
        "member_call_expression" => {
            // Extract the method name from the name field
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = extractor.get_base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.get_base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::Call,
                    containing_symbol_id,
                );
            }
        }

        // Member access: $obj->property
        "member_access_expression" => {
            // Skip if parent is a call expression (handled above)
            if let Some(parent) = node.parent() {
                if parent.kind() == "function_call_expression"
                    || parent.kind() == "member_call_expression"
                {
                    return; // Skip - handled by call expressions
                }
            }

            // Extract the member name (rightmost identifier)
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = extractor.get_base().get_node_text(&name_node);
                let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

                extractor.get_base_mut().create_identifier(
                    &name_node,
                    name,
                    IdentifierKind::MemberAccess,
                    containing_symbol_id,
                );
            }
        }

        _ => {
            // Skip other node types for now
        }
    }
}

/// Find the ID of the symbol that contains this node
/// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
fn find_containing_symbol_id(
    extractor: &PhpExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
) -> Option<String> {
    // CRITICAL FIX: Only search symbols from THIS FILE, not all files
    // Bug was: searching all symbols in DB caused wrong file symbols to match
    let file_symbols: Vec<Symbol> = symbol_map
        .values()
        .filter(|s| s.file_path == extractor.get_base().file_path)
        .map(|&s| s.clone())
        .collect();

    extractor
        .get_base()
        .find_containing_symbol(&node, &file_symbols)
        .map(|s| s.id.clone())
}
