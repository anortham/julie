//! Enum extraction for GDScript

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Extract enum definition
pub(super) fn extract_enum_definition(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
) -> Option<Symbol> {
    // For enum_definition nodes, find the identifier child directly
    let name = if let Some(name_node) = find_child_by_type(node, "identifier") {
        base.get_node_text(&name_node)
    } else {
        // Try to extract name from the text pattern: "enum Name { ... }"
        let text = base.get_node_text(&node);
        if let Some(captures) = regex::Regex::new(r"enum\s+(\w+)\s*\{")
            .unwrap()
            .captures(&text)
        {
            captures.get(1)?.as_str().to_string()
        } else {
            return None;
        }
    };

    let signature = base.get_node_text(&node);

    // Extract doc comment
    let doc_comment = base.find_doc_comment(&node);

    let enum_symbol = base.create_symbol(
        &node,
        name,
        SymbolKind::Enum,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.cloned(),
            metadata: None,
            doc_comment,
        },
    );

    // Note: Enum members would be extracted in the traversal as children
    Some(enum_symbol)
}

/// Extract enum member from an identifier within an enum
pub(super) fn extract_enum_member(
    base: &mut BaseExtractor,
    node: Node,
    _parent_id: Option<&String>,
    symbols: &[Symbol],
) -> Option<Symbol> {
    // Check if this identifier is inside an enum by checking the parent chain
    let enum_parent = find_enum_parent(node, symbols)?;

    let name = base.get_node_text(&node);

    // Skip if this is a type annotation or other non-member identifier
    if name.is_empty() || name.chars().next()?.is_lowercase() {
        return None;
    }

    // Extract doc comment
    let doc_comment = base.find_doc_comment(&node);

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::EnumMember,
        SymbolOptions {
            signature: Some(base.get_node_text(&node)),
            visibility: Some(Visibility::Public),
            parent_id: Some(enum_parent.id.clone()),
            metadata: None,
            doc_comment,
        },
    ))
}

/// Find the enum parent of a node by walking up the AST
fn find_enum_parent<'a>(node: Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
    // Walk up the AST to find if we're inside an enum definition
    let mut current = node.parent()?;

    while let Some(parent) = current.parent() {
        if current.kind() == "enum_definition" {
            // Find the corresponding enum symbol
            let enum_position = current.start_position();
            return symbols.iter().find(|s| {
                s.kind == SymbolKind::Enum
                    && s.start_line == (enum_position.row + 1) as u32
                    && s.start_column == enum_position.column as u32
            });
        }
        current = parent;
    }
    None
}

/// Helper to find a child node of a specific type
fn find_child_by_type<'a>(node: Node<'a>, child_type: &str) -> Option<Node<'a>> {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == child_type {
                return Some(child);
            }
        }
    }
    None
}
