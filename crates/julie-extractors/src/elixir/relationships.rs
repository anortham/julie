//! Relationship extraction for Elixir (use, behaviour, defimpl, calls)

use crate::base::{Relationship, Symbol};
use crate::elixir::ElixirExtractor;
use tree_sitter::Tree;

/// Extract all relationships from an Elixir file
pub(super) fn extract_relationships(
    _extractor: &mut ElixirExtractor,
    _tree: &Tree,
    _symbols: &[Symbol],
    _relationships: &mut Vec<Relationship>,
) {
    // TODO: Implement in Task 13
}
