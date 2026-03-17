//! Helper functions for Elixir symbol extraction
//!
//! Provides utility functions for extracting call targets, function heads,
//! module names, do blocks, and guard clauses.

use crate::base::BaseExtractor;
use tree_sitter::Node;

/// Extract the target name from a call node (e.g., "defmodule", "def", "import")
pub(super) fn extract_call_target_name(base: &BaseExtractor, node: &Node) -> Option<String> {
    // The target field of a call node is the function being called
    let target = node.child_by_field_name("target")?;
    match target.kind() {
        "identifier" => Some(base.get_node_text(&target).to_string()),
        "dot" => {
            // qualified call: Module.function — extract just the function name
            let mut cursor = target.walk();
            target
                .children(&mut cursor)
                .filter(|n| n.kind() == "identifier")
                .last()
                .map(|n| base.get_node_text(&n).to_string())
        }
        _ => None,
    }
}
