// PHP Extractor - Identifier extraction (function calls, member access, type usage)

use super::PhpExtractor;
use crate::base::{IdentifierKind, Symbol};
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

        // Type annotations: parameter types, return types, property types.
        // PHP tree-sitter uses `named_type` for class/interface type references
        // (e.g., Request, Response, App) and `primitive_type` for builtins
        // (e.g., int, string, void). We only create type_usage for named_type.
        //
        // named_type appears in:
        //   - Parameter types:  function handle(Request $req)
        //   - Return types:     function handle(): Response
        //   - Property types:   public Request $request
        //   - Union types:      string|Request  (named_type inside union_type)
        //   - Optional types:   ?Request        (named_type inside optional_type)
        "named_type" => {
            let name = extractor.get_base().get_node_text(&node);

            // Skip single-letter type params (rare in PHP, but possible)
            if name.len() <= 1 {
                return;
            }

            let containing_symbol_id = find_containing_symbol_id(extractor, node, symbol_map);

            extractor.get_base_mut().create_identifier(
                &node,
                name,
                IdentifierKind::TypeUsage,
                containing_symbol_id,
            );
        }

        // instanceof expressions: $obj instanceof Router
        // PHP tree-sitter represents this as binary_expression with an
        // "instanceof" anonymous child. The type name after instanceof is
        // a `name` node.
        "binary_expression" => {
            let mut cursor = node.walk();
            let mut found_instanceof = false;
            for child in node.children(&mut cursor) {
                if found_instanceof && child.is_named() {
                    let name = extractor.get_base().get_node_text(&child);

                    // Skip single-letter names
                    if name.len() <= 1 {
                        return;
                    }

                    let containing_symbol_id =
                        find_containing_symbol_id(extractor, node, symbol_map);

                    extractor.get_base_mut().create_identifier(
                        &child,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                    return;
                }
                if child.kind() == "instanceof" {
                    found_instanceof = true;
                }
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
