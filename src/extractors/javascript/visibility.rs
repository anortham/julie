//! Visibility extraction for JavaScript
//!
//! Handles extraction of symbol visibility based on naming conventions,
//! since JavaScript doesn't have explicit visibility modifiers like TypeScript.

use crate::extractors::base::Visibility;
use tree_sitter::Node;

impl super::JavaScriptExtractor {
    /// Extract visibility - direct Implementation of extractVisibility
    pub(super) fn extract_visibility(&self, node: &Node) -> Visibility {
        // JavaScript doesn't have explicit visibility modifiers like TypeScript
        // But we can infer from naming conventions (reference logic)
        let name_node = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("property"));

        if let Some(name) = name_node {
            let name_text = self.base.get_node_text(&name);
            if name_text.starts_with('#') {
                return Visibility::Private;
            }
            if name_text.starts_with('_') {
                return Visibility::Protected; // Convention
            }
        }

        Visibility::Public
    }
}
