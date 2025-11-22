//! Utility helper functions for JavaScript extraction
//!
//! This module contains common utility functions used across the extractor,
//! including checks for async/generator functions, visibility detection,
//! parameter extraction, and other shared logic.

use tree_sitter::Node;

impl super::JavaScriptExtractor {
    /// Check if function is async - direct Implementation of isAsync
    pub(super) fn is_async(&self, node: &Node) -> bool {
        // Direct check: node has async child (reference logic)
        if node
            .children(&mut node.walk())
            .any(|c| self.base.get_node_text(&c) == "async" || c.kind() == "async")
        {
            return true;
        }

        // For arrow functions: check if first child is async (reference logic)
        if node.kind() == "arrow_function" {
            if let Some(first_child) = node.child(0) {
                if self.base.get_node_text(&first_child) == "async" {
                    return true;
                }
            }
        }

        // For function expressions and arrow functions assigned to variables, check parent (reference logic)
        let mut current = node.parent();
        while let Some(current_node) = current {
            if current_node.kind() == "program" {
                break;
            }
            if current_node
                .children(&mut current_node.walk())
                .any(|c| self.base.get_node_text(&c) == "async")
            {
                return true;
            }
            current = current_node.parent();
        }

        false
    }

    /// Check if function is generator - direct Implementation of isGenerator
    pub(super) fn is_generator(&self, node: &Node) -> bool {
        node.kind().contains("generator")
            || node.children(&mut node.walk()).any(|c| c.kind() == "*")
            || node
                .parent()
                .map(|p| p.children(&mut p.walk()).any(|c| c.kind() == "*"))
                .unwrap_or(false)
    }

    /// Check if declaration is const - direct Implementation of isConstDeclaration
    pub(super) fn is_const_declaration(&self, node: &Node) -> bool {
        self.get_declaration_type(node) == "const"
    }

    /// Check if declaration is let - direct Implementation of isLetDeclaration
    pub(super) fn is_let_declaration(&self, node: &Node) -> bool {
        self.get_declaration_type(node) == "let"
    }

    /// Check if class has private fields - direct Implementation of hasPrivateFields
    pub(super) fn has_private_fields(&self, node: &Node) -> bool {
        for child in node.children(&mut node.walk()) {
            if child.kind() == "class_body" {
                for member in child.children(&mut child.walk()) {
                    let name_node = member
                        .child_by_field_name("name")
                        .or_else(|| member.child_by_field_name("property"));
                    if let Some(name) = name_node {
                        if self.base.get_node_text(&name).starts_with('#') {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Check if property is computed - direct Implementation of isComputedProperty
    pub(super) fn is_computed_property(&self, node: &Node) -> bool {
        node.child_by_field_name("key")
            .map(|key| key.kind() == "computed_property_name")
            .unwrap_or(false)
    }

    /// Check if import has default - direct Implementation of hasDefaultImport
    pub(super) fn has_default_import(&self, node: &Node) -> bool {
        node.children(&mut node.walk())
            .any(|c| c.kind() == "import_default_specifier")
    }

    /// Check if import has namespace - direct Implementation of hasNamespaceImport
    pub(super) fn has_namespace_import(&self, node: &Node) -> bool {
        node.children(&mut node.walk())
            .any(|c| c.kind() == "namespace_import")
    }

    /// Check if export is default - direct Implementation of isDefaultExport
    pub(super) fn is_default_export(&self, node: &Node) -> bool {
        node.children(&mut node.walk())
            .any(|c| c.kind() == "default")
    }

    /// Check if export is named - direct Implementation of isNamedExport
    pub(super) fn is_named_export(&self, node: &Node) -> bool {
        !self.is_default_export(node)
    }

    /// Extract function parameters - direct Implementation of extractParameters
    pub(super) fn extract_parameters(&self, node: &Node) -> Vec<String> {
        // Look for formal_parameters node (reference logic)
        let formal_params = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "formal_parameters");
        if let Some(params) = formal_params {
            let mut parameters = Vec::new();
            for child in params.children(&mut params.walk()) {
                if matches!(
                    child.kind(),
                    "identifier"
                        | "rest_pattern"
                        | "object_pattern"
                        | "array_pattern"
                        | "assignment_pattern"
                        | "object_assignment_pattern"
                        | "shorthand_property_identifier_pattern"
                ) {
                    parameters.push(self.base.get_node_text(&child));
                }
            }
            return parameters;
        }
        Vec::new()
    }

    /// Get declaration type - direct Implementation of getDeclarationType
    pub(super) fn get_declaration_type(&self, node: &Node) -> String {
        let mut current = node.parent();
        while let Some(current_node) = current {
            if current_node.kind() == "variable_declaration"
                || current_node.kind() == "lexical_declaration"
            {
                // Look for the keyword in the first child (reference logic)
                if let Some(first_child) = current_node.child(0) {
                    let text = self.base.get_node_text(&first_child);
                    if ["const", "let", "var"].contains(&text.as_str()) {
                        return text;
                    }
                }
                // Fallback: look through all children for keywords (reference logic)
                for child in current_node.children(&mut current_node.walk()) {
                    let text = self.base.get_node_text(&child);
                    if ["const", "let", "var"].contains(&text.as_str()) {
                        return text;
                    }
                }
            }
            current = current_node.parent();
        }
        "var".to_string()
    }

    /// Check if node is require call - direct Implementation of isRequireCall
    pub(super) fn is_require_call(&self, node: &Node) -> bool {
        if node.kind() == "call_expression" {
            if let Some(function_node) = node.child_by_field_name("function") {
                return self.base.get_node_text(&function_node) == "require";
            }
        }
        false
    }

    /// Extract require source - direct Implementation of extractRequireSource
    pub(super) fn extract_require_source(&self, node: &Node) -> String {
        if node.kind() == "call_expression" {
            if let Some(args) = node.child_by_field_name("arguments") {
                for child in args.children(&mut args.walk()) {
                    if child.kind() == "string" {
                        return self
                            .base
                            .get_node_text(&child)
                            .replace(&['\'', '"', '`'][..], "");
                    }
                }
            }
        }
        "unknown".to_string()
    }
}
