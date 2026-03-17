//! Property extraction for Scala (val/var definitions)

use crate::base::{BaseExtractor, Symbol};
use tree_sitter::Node;

/// Extract a Scala val definition (immutable)
pub(super) fn extract_val(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 5
    None
}

/// Extract a Scala var definition (mutable)
pub(super) fn extract_var(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 5
    None
}
