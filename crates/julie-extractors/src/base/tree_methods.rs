// Tree navigation methods for BaseExtractor
//
// Extracted from extractor.rs to keep modules under 500 lines

use super::extractor::BaseExtractor;
use tree_sitter::Node;

impl BaseExtractor {
    /// Walk tree with visitor - exact port of walkTree
    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    pub fn walk_tree<F>(&self, node: &Node, visitor: &mut F, depth: u32)
    where
        F: FnMut(&Node, u32),
    {
        visitor(node, depth);

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.walk_tree(&child, visitor, depth + 1);
            }
        }
    }

    /// Find nodes by type - exact port of findNodesByType
    pub fn find_nodes_by_type<'a>(&self, node: &Node<'a>, node_type: &str) -> Vec<Node<'a>> {
        let mut nodes = Vec::new();
        self.find_nodes_by_type_recursive(node, node_type, &mut nodes);
        nodes
    }

    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    fn find_nodes_by_type_recursive<'a>(
        &self,
        node: &Node<'a>,
        node_type: &str,
        nodes: &mut Vec<Node<'a>>,
    ) {
        if node.kind() == node_type {
            nodes.push(*node);
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.find_nodes_by_type_recursive(&child, node_type, nodes);
            }
        }
    }

    /// Find parent of type - exact port of findParentOfType
    pub fn find_parent_of_type<'a>(&self, node: &Node<'a>, parent_type: &str) -> Option<Node<'a>> {
        let mut current = node.parent();
        while let Some(parent) = current {
            if parent.kind() == parent_type {
                return Some(parent);
            }
            current = parent.parent();
        }
        None
    }

    /// Check if node has error - exact port of hasError
    pub fn has_error(&self, node: &Node) -> bool {
        node.has_error() || node.kind() == "ERROR"
    }

    /// Get children of type - exact port of getChildrenOfType
    pub fn get_children_of_type<'a>(&self, node: &Node<'a>, child_type: &str) -> Vec<Node<'a>> {
        let mut children = Vec::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == child_type {
                    children.push(child);
                }
            }
        }
        children
    }

    /// Get field text safely - exact port of getFieldText
    pub fn get_field_text(&self, node: &Node, field_name: &str) -> Option<String> {
        node.child_by_field_name(field_name)
            .map(|field_node| self.get_node_text(&field_node))
    }

    /// Traverse tree with error handling - exact port of traverseTree
    #[allow(clippy::only_used_in_recursion)] // &self used in recursive calls
    pub fn traverse_tree<F>(&self, node: &Node, callback: &mut F)
    where
        F: FnMut(&Node),
    {
        use tracing::debug;
        use tracing::warn;

        // Try to process current node
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(node))) {
            Ok(_) => {}
            Err(_) => {
                warn!("Error processing node {}", node.kind());
                return;
            }
        }

        // Recursively traverse children with error handling
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.traverse_tree(&child, callback)
                })) {
                    Ok(_) => {}
                    Err(_) => {
                        debug!("Skipping problematic child node");
                        continue;
                    }
                }
            }
        }
    }

    /// Find first child by type - exact port of findChildByType
    pub fn find_child_by_type<'a>(&self, node: &Node<'a>, child_type: &str) -> Option<Node<'a>> {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == child_type {
                    return Some(child);
                }
            }
        }
        None
    }

    /// Find children by type - exact port of findChildrenByType
    pub fn find_children_by_type<'a>(&self, node: &Node<'a>, child_type: &str) -> Vec<Node<'a>> {
        let mut results = Vec::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == child_type {
                    results.push(child);
                }
            }
        }
        results
    }

    /// Find child by multiple types - exact port of findChildByTypes
    pub fn find_child_by_types<'a>(&self, node: &Node<'a>, types: &[&str]) -> Option<Node<'a>> {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if types.contains(&child.kind()) {
                    return Some(child);
                }
            }
        }
        None
    }

    /// Extract documentation - alias for find_doc_comment (API consistency)
    pub fn extract_documentation(&self, node: &Node) -> Option<String> {
        self.find_doc_comment(node)
    }
}
