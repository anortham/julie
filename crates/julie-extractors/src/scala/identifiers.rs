//! Identifier and reference extraction for Scala

use crate::base::{BaseExtractor, Identifier, Symbol};

/// Extract all identifier usages from a Scala file
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    _tree: &tree_sitter::Tree,
    _symbols: &[Symbol],
) -> Vec<Identifier> {
    // TODO: Implement in Task 6
    base.identifiers.clone()
}
