//! Declaration extraction for Scala (functions, imports, packages, type aliases, given, extension)

use crate::base::{BaseExtractor, Symbol};
use tree_sitter::Node;

/// Extract a Scala function/method definition
pub(super) fn extract_function(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 4
    None
}

/// Extract a Scala import declaration
pub(super) fn extract_import(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 4
    None
}

/// Extract a Scala package clause
pub(super) fn extract_package(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 4
    None
}

/// Extract a Scala type alias
pub(super) fn extract_type_alias(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 5
    None
}

/// Extract a Scala 3 given definition
pub(super) fn extract_given(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 5
    None
}

/// Extract a Scala 3 extension definition
pub(super) fn extract_extension(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 5
    None
}
