//! Type extraction for Scala (classes, traits, objects, enums)

use crate::base::{BaseExtractor, Symbol};
use tree_sitter::Node;

/// Extract a Scala class definition
pub(super) fn extract_class(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 3
    None
}

/// Extract a Scala trait definition
pub(super) fn extract_trait(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 3
    None
}

/// Extract a Scala object definition
pub(super) fn extract_object(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 3
    None
}

/// Extract a Scala enum definition (Scala 3)
pub(super) fn extract_enum(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 3
    None
}

/// Extract a Scala enum case (simple or full)
pub(super) fn extract_enum_case(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 3
    None
}
