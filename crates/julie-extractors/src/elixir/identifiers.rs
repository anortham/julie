//! Identifier and reference extraction for Elixir

use crate::base::{BaseExtractor, Identifier, Symbol};

/// Extract all identifier usages from an Elixir file
pub(super) fn extract_identifiers(
    base: &mut BaseExtractor,
    _tree: &tree_sitter::Tree,
    _symbols: &[Symbol],
) -> Vec<Identifier> {
    // TODO: Implement in Task 13
    base.identifiers.clone()
}
