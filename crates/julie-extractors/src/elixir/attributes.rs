//! Module attribute extraction for Elixir
//!
//! Handles @doc, @moduledoc, @spec, @type, @callback, @behaviour attributes.
//! These are `unary_operator` nodes with `@` as the operator.

use crate::base::{BaseExtractor, Symbol};
use tree_sitter::Node;

/// Extract a module attribute from a unary_operator node
pub(super) fn extract_module_attribute(
    _base: &mut BaseExtractor,
    _node: &Node,
    _parent_id: Option<&str>,
) -> Option<Symbol> {
    // TODO: Implement in Task 12
    None
}
