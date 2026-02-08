use super::helpers::infer_symbol_kind_from_assignment;
/// Assignment handling for Ruby symbols
/// Includes support for regular assignments, parallel assignments, and rest assignments
use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
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
