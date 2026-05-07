//! Signature extraction and documentation for Bash
//!
//! Provides functions for building signatures of functions, variables, commands,
//! and other Bash constructs.

use tree_sitter::Node;

impl super::BashExtractor {
    /// Build signature for a function definition
    pub(super) fn extract_function_signature(&self, node: Node) -> Option<String> {
        let name_node = self.find_name_node(node);
        let name = name_node.map(|n| self.base.get_node_text(&n))?;
        Some(format!("function {}()", name))
    }

    /// Build signature for a variable assignment
    pub(super) fn extract_variable_signature(&self, node: Node) -> Option<String> {
        let name_node = self.find_variable_name_node(node);
        let name = name_node.map(|n| self.base.get_node_text(&n))?;

        // Get the full assignment text and extract value
        let full_text = self.base.get_node_text(&node);
        if let Some(equal_index) = full_text.find('=') {
            let value = full_text.get(equal_index + 1..).unwrap_or("").trim();
            if !value.is_empty() {
                return Some(format!("{}={}", name, value));
            }
        }

        Some(name)
    }
}
