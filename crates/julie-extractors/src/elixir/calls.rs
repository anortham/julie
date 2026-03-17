//! Call dispatch for Elixir definition macros
//!
//! In Elixir, all definitions (defmodule, def, defp, defmacro, etc.) are
//! generic `call` nodes. This module inspects the call target name and
//! dispatches to the appropriate extraction function.

use crate::base::Symbol;
use crate::elixir::ElixirExtractor;
use tree_sitter::Node;

/// Dispatch a call node to the appropriate extractor.
/// Returns (Symbol, children_handled) where children_handled indicates
/// whether the handler already visited child nodes (e.g., defmodule visits its do_block).
pub(super) fn dispatch_call(
    _extractor: &mut ElixirExtractor,
    _node: &Node,
    _symbols: &mut Vec<Symbol>,
    _parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    // TODO: Implement in Task 10
    // let target_name = helpers::extract_call_target_name(&extractor.base, node)?;
    // match target_name.as_str() {
    //     "defmodule" => ...
    //     "def" => ...
    //     "defp" => ...
    //     "defmacro" | "defmacrop" => ...
    //     "defprotocol" => ...
    //     "defimpl" => ...
    //     "defstruct" => ...
    //     "import" | "use" | "alias" | "require" => ...
    //     _ => None,
    // }
    None
}
