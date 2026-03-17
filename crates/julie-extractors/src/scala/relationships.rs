//! Relationship extraction for Scala (inheritance, calls)

use crate::base::{Relationship, Symbol};
use crate::scala::ScalaExtractor;
use tree_sitter::Node;

/// Extract inheritance and implementation relationships
pub(super) fn extract_inheritance_relationships(
    _extractor: &mut ScalaExtractor,
    _node: &Node,
    _symbols: &[Symbol],
    _relationships: &mut Vec<Relationship>,
) {
    // TODO: Implement in Task 6
}

/// Extract function/method call relationships
pub(super) fn extract_call_relationships(
    _extractor: &mut ScalaExtractor,
    _node: Node,
    _symbols: &[Symbol],
    _relationships: &mut Vec<Relationship>,
) {
    // TODO: Implement in Task 6
}
