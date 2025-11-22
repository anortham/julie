//! Helper utilities for Bash node traversal and extraction
//!
//! Provides common functions for finding specific node types and working with
//! tree-sitter nodes in Bash code.

use tree_sitter::Node;

impl super::BashExtractor {
    /// Find the name node for a function definition
    pub(super) fn find_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        // Look for function name nodes
        if let Some(name_field) = node.child_by_field_name("name") {
            return Some(name_field);
        }

        // Fallback: look for 'word' or 'identifier' children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "word" | "identifier") {
                return Some(child);
            }
        }
        None
    }

    /// Find variable name node in variable assignments
    #[allow(clippy::manual_find)] // Manual loops required for borrow checker
    pub(super) fn find_variable_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        // Look for variable name in assignments
        if let Some(name_field) = node.child_by_field_name("name") {
            return Some(name_field);
        }

        // Look for variable_name child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_name" {
                return Some(child);
            }
        }

        // Fallback: look for word child (first one usually)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "word" {
                return Some(child);
            }
        }
        None
    }

    /// Find command name node in command invocations
    #[allow(clippy::manual_find)] // Manual loops required for borrow checker
    pub(super) fn find_command_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        // Look for command name field
        if let Some(name_field) = node.child_by_field_name("name") {
            return Some(name_field);
        }

        // Look for command_name child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "command_name" {
                return Some(child);
            }
        }

        // Fallback: first word child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "word" {
                return Some(child);
            }
        }
        None
    }

    /// Get all children of a specific type
    pub(super) fn get_children_of_type<'a>(
        &self,
        node: Node<'a>,
        node_type: &str,
    ) -> Vec<Node<'a>> {
        let mut children = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == node_type {
                children.push(child);
            }
        }

        children
    }

    /// Recursively collect parameter nodes (simple_expansion or expansion)
    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    pub(super) fn collect_parameter_nodes<'a>(
        &self,
        node: Node<'a>,
        param_nodes: &mut Vec<Node<'a>>,
    ) {
        if matches!(node.kind(), "simple_expansion" | "expansion") {
            param_nodes.push(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_parameter_nodes(child, param_nodes);
        }
    }

    /// Generic tree walk with callback - useful for debugging and analysis
    #[allow(dead_code)]
    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    pub(super) fn walk_tree<'a, F>(&self, node: Node<'a>, callback: &mut F)
    where
        F: FnMut(Node<'a>),
    {
        callback(node);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(child, callback);
        }
    }
}
