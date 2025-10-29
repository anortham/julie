use super::helpers::infer_symbol_kind_from_assignment;
/// Assignment handling for Ruby symbols
/// Includes support for regular assignments, parallel assignments, and rest assignments
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Extract a symbol from an assignment node
pub(super) fn extract_assignment(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    // Handle various assignment patterns including parallel assignment
    let left_side = node
        .child_by_field_name("left")
        .or_else(|| node.children(&mut node.walk()).next())?;

    // Handle parallel assignments (a, b, c = 1, 2, 3)
    if left_side.kind() == "left_assignment_list" {
        return handle_parallel_assignment(base, node, left_side, parent_id);
    }

    // Handle regular assignments
    let right_side = node
        .child_by_field_name("right")
        .or_else(|| node.children(&mut node.walk()).last());
    let name = base.get_node_text(&left_side);
    let signature = if let Some(right) = right_side {
        format!("{} = {}", name, base.get_node_text(&right))
    } else {
        name.clone()
    };

    let kind = infer_symbol_kind_from_assignment(&left_side, |n| base.get_node_text(n));

    Some(base.create_symbol(
        &node,
        name,
        kind,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id,
            metadata: None,
            doc_comment: None,
        },
    ))
}

/// Handle parallel assignment patterns (a, b, c = 1, 2, 3)
fn handle_parallel_assignment(
    base: &mut BaseExtractor,
    node: Node,
    left_side: Node,
    parent_id: Option<String>,
) -> Option<Symbol> {
    let full_assignment = base.get_node_text(&node);

    // Extract identifiers from left_assignment_list
    let mut cursor = left_side.walk();
    let identifiers: Vec<_> = left_side
        .children(&mut cursor)
        .filter(|child| child.kind() == "identifier")
        .collect();

    // Extract rest assignments (splat expressions like *rest)
    let mut cursor = left_side.walk();
    let rest_assignments: Vec<_> = left_side
        .children(&mut cursor)
        .filter(|child| child.kind() == "rest_assignment")
        .collect();

    // Create symbols for identifiers
    let mut created_symbols = Vec::new();

    for identifier in &identifiers {
        let name = base.get_node_text(identifier);
        let symbol = base.create_symbol(
            &node,
            name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(full_assignment.clone()),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.clone(),
                metadata: None,
                doc_comment: None,
            },
        );
        created_symbols.push(symbol);
    }

    // Handle rest assignments
    for rest_node in &rest_assignments {
        if let Some(rest_identifier) = rest_node
            .children(&mut rest_node.walk())
            .find(|c| c.kind() == "identifier")
        {
            let rest_name = base.get_node_text(&rest_identifier);
            let rest_symbol = base.create_symbol(
                &node,
                rest_name,
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(full_assignment.clone()),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.clone(),
                    metadata: None,
                    doc_comment: None,
                },
            );
            created_symbols.push(rest_symbol);
        }
    }

    // Store additional symbols in the base extractor's symbol_map
    // Since this method only returns one symbol, we add the rest to the symbol_map
    for symbol in created_symbols.iter().skip(1) {
        base.symbol_map.insert(symbol.id.clone(), symbol.clone());
    }

    // Return the first symbol (if any were created)
    created_symbols.into_iter().next()
}

/// Extract assignment symbols into a collection (alternative entry point)
#[allow(dead_code)]
pub(super) fn extract_assignment_symbols(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
    symbols: &mut Vec<Symbol>,
) {
    let left_side = node
        .child_by_field_name("left")
        .or_else(|| node.children(&mut node.walk()).next());

    if let Some(left) = left_side {
        // Handle parallel assignments (a, b, c = 1, 2, 3)
        if left.kind() == "left_assignment_list" {
            let full_assignment = base.get_node_text(&node);

            // Extract all identifiers from left_assignment_list
            let mut cursor = left.walk();
            for child in left.children(&mut cursor) {
                if child.kind() == "identifier" {
                    let name = base.get_node_text(&child);
                    let symbol = base.create_symbol(
                        &node,
                        name,
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(full_assignment.clone()),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.clone(),
                            metadata: None,
                            doc_comment: None,
                        },
                    );
                    symbols.push(symbol);
                } else if child.kind() == "rest_assignment" {
                    // Handle rest assignments (splat expressions like *rest)
                    if let Some(rest_identifier) = child
                        .children(&mut child.walk())
                        .find(|c| c.kind() == "identifier")
                    {
                        let rest_name = base.get_node_text(&rest_identifier);
                        let symbol = base.create_symbol(
                            &node,
                            rest_name,
                            SymbolKind::Variable,
                            SymbolOptions {
                                signature: Some(full_assignment.clone()),
                                visibility: Some(Visibility::Public),
                                parent_id: parent_id.clone(),
                                metadata: None,
                                doc_comment: None,
                            },
                        );
                        symbols.push(symbol);
                    }
                } else if child.kind() == "splat_argument" || child.kind().contains("splat") {
                    // Try alternative splat node names
                    if let Some(rest_identifier) = child
                        .children(&mut child.walk())
                        .find(|c| c.kind() == "identifier")
                    {
                        let rest_name = base.get_node_text(&rest_identifier);
                        let symbol = base.create_symbol(
                            &node,
                            rest_name,
                            SymbolKind::Variable,
                            SymbolOptions {
                                signature: Some(full_assignment.clone()),
                                visibility: Some(Visibility::Public),
                                parent_id: parent_id.clone(),
                                metadata: None,
                                doc_comment: None,
                            },
                        );
                        symbols.push(symbol);
                    }
                }
            }
            return;
        } else {
            // Check if this might be a parallel assignment with different structure
            let assignment_text = base.get_node_text(&node);
            if assignment_text.contains(",")
                && (assignment_text.contains("*") || assignment_text.contains("="))
            {
                // Try to extract variables from a manual parse of the assignment
                extract_parallel_assignment_fallback(
                    base,
                    &node,
                    &assignment_text,
                    parent_id.clone(),
                    symbols,
                );
                return;
            }
        }

        // Handle regular assignments
        let right_side = node
            .child_by_field_name("right")
            .or_else(|| node.children(&mut node.walk()).last());
        let name = base.get_node_text(&left);
        let signature = if let Some(right) = right_side {
            format!("{} = {}", name, base.get_node_text(&right))
        } else {
            name.clone()
        };

        let kind = infer_symbol_kind_from_assignment(&left, |n| base.get_node_text(n));

        let symbol = base.create_symbol(
            &node,
            name,
            kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id,
                metadata: None,
                doc_comment: None,
            },
        );
        symbols.push(symbol);
    }
}

/// Fallback method to extract variables from parallel assignments when tree structure is unexpected
#[allow(dead_code)]
pub(super) fn extract_parallel_assignment_fallback(
    base: &mut BaseExtractor,
    node: &Node,
    assignment_text: &str,
    parent_id: Option<String>,
    symbols: &mut Vec<Symbol>,
) {
    // Split by '=' to get left and right sides
    if let Some(eq_pos) = assignment_text.find('=') {
        // Safe: '=' is ASCII, so this will be at a char boundary, but verify to be safe
        if !assignment_text.is_char_boundary(eq_pos) {
            return; // Skip malformed input
        }
        let left_side = assignment_text[..eq_pos].trim();

        // Extract variable names from the left side
        let variables: Vec<&str> = left_side.split(',').map(|s| s.trim()).collect();

        for var in variables {
            let clean_var = var.trim_start_matches('*'); // Remove splat operator
            if !clean_var.is_empty() && clean_var.chars().all(|c| c.is_alphanumeric() || c == '_') {
                let symbol = base.create_symbol(
                    node,
                    clean_var.to_string(),
                    SymbolKind::Variable,
                    SymbolOptions {
                        signature: Some(assignment_text.to_string()),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.clone(),
                        metadata: None,
                        doc_comment: None,
                    },
                );
                symbols.push(symbol);
            }
        }
    }
}
