// R Relationship Extraction
// Extracts relationships between R symbols: function calls, library usage, pipes

use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::extractors::r::RExtractor;
use tree_sitter::{Node, Tree};

/// Extract all relationships from R code
pub(super) fn extract_relationships(
    extractor: &RExtractor,
    tree: &Tree,
    symbols: &[Symbol],
) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    extract_call_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    extract_pipe_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    extract_member_access_relationships(extractor, tree.root_node(), symbols, &mut relationships);
    relationships
}

/// Extract function call relationships
fn extract_call_relationships(
    extractor: &RExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // R function calls are represented as "call" nodes
    if node.kind() == "call" {
        // The function being called is the first child
        if let Some(function_node) = node.child(0) {
            let function_name = match function_node.kind() {
                "identifier" => extractor.base.get_node_text(&function_node),
                "namespace_operator" => {
                    // Handle package::function syntax
                    if let Some(function_child) = function_node.child(2) {
                        extractor.base.get_node_text(&function_child)
                    } else {
                        extractor.base.get_node_text(&function_node)
                    }
                }
                "extract_operator" => {
                    // Handle $ operator: object$method()
                    if let Some(member) = function_node.child(2) {
                        extractor.base.get_node_text(&member)
                    } else {
                        extractor.base.get_node_text(&function_node)
                    }
                }
                _ => extractor.base.get_node_text(&function_node),
            };

            // Find the containing function (caller)
            if let Some(caller_symbol) = find_containing_function(node, symbols) {
                // Find the called function symbol (might be user-defined or built-in)
                if let Some(called_symbol) = symbols
                    .iter()
                    .find(|s| s.name == function_name && s.kind == SymbolKind::Function)
                {
                    let relationship = Relationship {
                        id: format!(
                            "{}_{}_{:?}_{}",
                            caller_symbol.id,
                            called_symbol.id,
                            RelationshipKind::Calls,
                            node.start_position().row
                        ),
                        from_symbol_id: caller_symbol.id.clone(),
                        to_symbol_id: called_symbol.id.clone(),
                        kind: RelationshipKind::Calls,
                        file_path: extractor.base.file_path.clone(),
                        line_number: (node.start_position().row + 1) as u32,
                        confidence: 1.0,
                        metadata: None,
                    };
                    relationships.push(relationship);
                } else {
                    // For built-in functions or library functions, create a relationship anyway
                    // This helps track usage even if the function isn't defined in this file
                    if let Some(caller_symbol) = find_containing_function(node, symbols) {
                        let relationship = Relationship {
                            id: format!(
                                "{}_{:?}_{}_{}",
                                caller_symbol.id,
                                RelationshipKind::Calls,
                                function_name,
                                node.start_position().row
                            ),
                            from_symbol_id: caller_symbol.id.clone(),
                            to_symbol_id: format!("builtin_{}", function_name),
                            kind: RelationshipKind::Calls,
                            file_path: extractor.base.file_path.clone(),
                            line_number: (node.start_position().row + 1) as u32,
                            confidence: 0.8,
                            metadata: None,
                        };
                        relationships.push(relationship);
                    }
                }
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_call_relationships(extractor, child, symbols, relationships);
    }
}

/// Extract pipe operator relationships (%>%, |>, etc.)
fn extract_pipe_relationships(
    extractor: &RExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // Pipe operators in R are binary operators
    if node.kind() == "binary_operator" {
        if let Some(operator) = node.child(1) {
            let op_text = extractor.base.get_node_text(&operator);

            // Check if this is a pipe operator
            if op_text == "%>%" || op_text == "|>" {
                // The right side of the pipe is typically a function call
                if let Some(right_child) = node.child(2) {
                    if right_child.kind() == "call" {
                        // Extract the function being called
                        if let Some(function_node) = right_child.child(0) {
                            let function_name = extractor.base.get_node_text(&function_node);

                            // Find containing function
                            if let Some(containing_symbol) = find_containing_function(node, symbols)
                            {
                                // Create a relationship for the piped call
                                let relationship = Relationship {
                                    id: format!(
                                        "{}_{:?}_{}_{}",
                                        containing_symbol.id,
                                        RelationshipKind::Calls,
                                        function_name,
                                        node.start_position().row
                                    ),
                                    from_symbol_id: containing_symbol.id.clone(),
                                    to_symbol_id: format!("piped_{}", function_name),
                                    kind: RelationshipKind::Calls,
                                    file_path: extractor.base.file_path.clone(),
                                    line_number: (node.start_position().row + 1) as u32,
                                    confidence: 0.9,
                                    metadata: None,
                                };
                                relationships.push(relationship);
                            }
                        }
                    }
                }
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_pipe_relationships(extractor, child, symbols, relationships);
    }
}

/// Extract member access relationships ($ operator)
fn extract_member_access_relationships(
    extractor: &RExtractor,
    node: Node,
    symbols: &[Symbol],
    relationships: &mut Vec<Relationship>,
) {
    // R uses extract_operator for $ and @
    if node.kind() == "extract_operator" {
        // The member being accessed is the third child (index 2)
        if let Some(member_node) = node.child(2) {
            let member_name = extractor.base.get_node_text(&member_node);

            // Find containing function
            if let Some(containing_symbol) = find_containing_function(node, symbols) {
                // Create a Uses relationship for member access
                let relationship = Relationship {
                    id: format!(
                        "{}_{:?}_{}_{}",
                        containing_symbol.id,
                        RelationshipKind::Uses,
                        member_name,
                        node.start_position().row
                    ),
                    from_symbol_id: containing_symbol.id.clone(),
                    to_symbol_id: format!("member_{}", member_name),
                    kind: RelationshipKind::Uses,
                    file_path: extractor.base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 0.8,
                    metadata: None,
                };
                relationships.push(relationship);
            }
        }
    }

    // Recursively process children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_member_access_relationships(extractor, child, symbols, relationships);
    }
}

/// Find the containing function for a node
fn find_containing_function<'a>(node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        // R function definitions are inside binary_operator nodes
        if parent.kind() == "binary_operator" {
            // Check if the right side is a function_definition
            if let Some(right_child) = parent.child(2) {
                if right_child.kind() == "function_definition" {
                    // Find the symbol that matches this function
                    let func_line = parent.start_position().row + 1;
                    if let Some(symbol) = symbols
                        .iter()
                        .find(|s| s.kind == SymbolKind::Function && s.start_line == func_line as u32)
                    {
                        return Some(symbol);
                    }
                }
            }
        }
        current = parent;
    }
    None
}
