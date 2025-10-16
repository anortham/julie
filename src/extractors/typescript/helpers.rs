//! Helper functions for TypeScript extractor
//!
//! This module provides utility functions for tree traversal, node inspection,
//! and common extraction patterns used across other modules.

use crate::extractors::base::Symbol;
use tree_sitter::Node;

/// Find the containing symbol for a given node
///
/// Searches through the provided symbols to find one that contains the given node
/// by comparing line/column ranges. Only searches symbols from the same file.
pub(super) fn find_containing_symbol<'a>(
    node: Node,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let position = node.start_position();
    let pos_line = (position.row + 1) as u32;

    // Find symbols that contain this position
    for symbol in symbols {
        if symbol.start_line <= pos_line && symbol.end_line >= pos_line {
            return Some(symbol);
        }
    }

    None
}

/// Check if a node has a modifier child of the given kind
///
/// Useful for checking for 'async', 'static', 'abstract', etc.
pub(super) fn has_modifier(node: Node, modifier_kind: &str) -> bool {
    node.children(&mut node.walk())
        .any(|child| child.kind() == modifier_kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_modifier() {
        let code = "async function foo() {}";
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        fn find_function(node: Node) -> Option<Node> {
            if node.kind() == "function_declaration" {
                return Some(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_function(child) {
                    return Some(found);
                }
            }
            None
        }

        if let Some(func) = find_function(tree.root_node()) {
            assert!(has_modifier(func, "async"));
        }
    }
}
