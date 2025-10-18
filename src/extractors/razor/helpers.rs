/// Helper utilities for AST navigation and common extraction patterns
use crate::extractors::base::Visibility;
use tree_sitter::Node;

/// Common helper methods for finding and extracting node information
impl super::RazorExtractor {
    /// Find the first child node of a specific type
    #[allow(clippy::manual_find)] // Manual loop required for borrow checker
    pub(super) fn find_child_by_type<'a>(
        &self,
        node: Node<'a>,
        child_type: &str,
    ) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == child_type {
                return Some(child);
            }
        }
        None
    }

    /// Find the first child node matching any of the provided types
    #[allow(clippy::manual_find)] // Manual loop required for borrow checker
    pub(super) fn find_child_by_types<'a>(
        &self,
        node: Node<'a>,
        child_types: &[&str],
    ) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child_types.contains(&child.kind()) {
                return Some(child);
            }
        }
        None
    }

    /// Extract modifier keywords from a node (public, private, static, etc.)
    pub(super) fn extract_modifiers(&self, node: Node) -> Vec<String> {
        let mut modifiers = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let child_text = self.base.get_node_text(&child);
            let modifier_types = [
                "public",
                "private",
                "protected",
                "internal",
                "static",
                "virtual",
                "override",
                "abstract",
                "sealed",
                "async",
            ];
            if modifier_types.contains(&child.kind())
                || modifier_types.contains(&child_text.as_str())
            {
                modifiers.push(child_text);
            }
        }
        modifiers
    }

    /// Extract method parameters from a parameter_list node
    pub(super) fn extract_method_parameters(&self, node: Node) -> Option<String> {
        self.find_child_by_type(node, "parameter_list")
            .map(|param_list| self.base.get_node_text(&param_list))
    }

    /// Extract return type from a node
    pub(super) fn extract_return_type(&self, node: Node) -> Option<String> {
        let type_kinds = [
            "predefined_type",
            "identifier",
            "generic_name",
            "qualified_name",
            "nullable_type",
            "array_type",
        ];

        self.find_child_by_types(node, &type_kinds)
            .map(|return_type| self.base.get_node_text(&return_type))
    }

    /// Extract property type from a property_declaration node
    pub(super) fn extract_property_type(&self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        for (i, child) in children.iter().enumerate() {
            // Skip attributes and modifiers
            if child.kind() == "attribute_list"
                || [
                    "public",
                    "private",
                    "protected",
                    "internal",
                    "static",
                    "virtual",
                    "override",
                    "abstract",
                    "sealed",
                ]
                .contains(&child.kind())
                || [
                    "public",
                    "private",
                    "protected",
                    "internal",
                    "static",
                    "virtual",
                    "override",
                    "abstract",
                    "sealed",
                ]
                .contains(&self.base.get_node_text(child).as_str())
            {
                continue;
            }

            // Look for type nodes
            if matches!(
                child.kind(),
                "predefined_type" | "nullable_type" | "array_type" | "generic_name"
            ) || (child.kind() == "identifier" && i < children.len() - 2)
            {
                return Some(self.base.get_node_text(child));
            }
        }

        None
    }

    /// Extract attributes (annotations) from a node
    pub(super) fn extract_attributes(&self, node: Node) -> Vec<String> {
        let mut attributes = Vec::new();

        // Look for attribute_list nodes
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "attribute_list" {
                attributes.push(self.base.get_node_text(&child));
            }
        }

        // Also check siblings for attributes that might be before the declaration
        if let Some(parent) = node.parent() {
            let mut cursor = parent.walk();
            let children: Vec<_> = parent.children(&mut cursor).collect();
            if let Some(node_index) = children.iter().position(|c| c.id() == node.id()) {
                for i in (0..node_index).rev() {
                    let sibling = &children[i];
                    if sibling.kind() == "attribute_list" {
                        attributes.insert(0, self.base.get_node_text(sibling));
                    } else if !matches!(sibling.kind(), "newline" | "whitespace") {
                        break;
                    }
                }
            }
        }

        attributes
    }

    /// Determine visibility (public/private/protected) from modifiers
    pub(super) fn determine_visibility(&self, modifiers: &[String]) -> Visibility {
        if modifiers.iter().any(|m| m == "private") {
            Visibility::Private
        } else if modifiers.iter().any(|m| m == "protected") {
            Visibility::Protected
        } else {
            Visibility::Public
        }
    }

    /// Extract namespace name from a using directive or namespace declaration
    pub(super) fn extract_namespace_name(&self, node: Node) -> String {
        if let Some(name_node) = self.find_child_by_types(node, &["qualified_name", "identifier"]) {
            self.base.get_node_text(&name_node)
        } else {
            "UnknownNamespace".to_string()
        }
    }

    /// Check if a node is valid (not empty, not an error)
    pub(super) fn is_valid_node(&self, node: &Node) -> bool {
        !node.kind().is_empty() && !node.is_error()
    }
}
