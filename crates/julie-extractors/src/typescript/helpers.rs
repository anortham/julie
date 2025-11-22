//! Helper functions for TypeScript extractor
//!
//! This module provides utility functions for tree traversal, node inspection,
//! and common extraction patterns used across other modules.

use tree_sitter::Node;

/// Check if a node has a modifier child of the given kind
///
/// Useful for checking for 'async', 'static', 'abstract', etc.
pub(super) fn has_modifier(node: Node, modifier_kind: &str) -> bool {
    node.children(&mut node.walk())
        .any(|child| child.kind() == modifier_kind)
}
