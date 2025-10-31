use crate::extractors::base::Visibility;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts and constructs function/type signatures and related metadata
impl SwiftExtractor {
    /// Implementation of extractModifiers method
    pub(super) fn extract_modifiers(&self, node: Node) -> Vec<String> {
        let mut modifiers = Vec::new();

        if let Some(modifiers_list) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "modifiers")
        {
            for child in modifiers_list.children(&mut modifiers_list.walk()) {
                // Collect all modifiers, keywords, and attributes
                if matches!(
                    child.kind(),
                    "visibility_modifier"
                        | "mutation_modifier"
                        | "declaration_modifier"
                        | "access_level_modifier"
                        | "property_modifier"
                        | "member_modifier"
                        | "public"
                        | "private"
                        | "internal"
                        | "fileprivate"
                        | "open"
                        | "final"
                        | "static"
                        | "class"
                        | "override"
                        | "lazy"
                        | "weak"
                        | "unowned"
                        | "required"
                        | "convenience"
                        | "dynamic"
                        | "attribute"
                ) {
                    modifiers.push(self.base.get_node_text(&child));
                }
            }
        }

        // Check for direct modifier nodes
        for child in node.children(&mut node.walk()) {
            if child.kind() == "lazy" || self.base.get_node_text(&child) == "lazy" {
                modifiers.push("lazy".to_string());
            } else if child.kind() == "attribute" {
                modifiers.push(self.base.get_node_text(&child));
            }
        }

        modifiers
    }

    /// Implementation of extractGenericParameters method
    pub(super) fn extract_generic_parameters(&self, node: Node) -> Option<String> {
        node.children(&mut node.walk())
            .find(|c| c.kind() == "type_parameters")
            .map(|generic_params| self.base.get_node_text(&generic_params))
    }

    /// Implementation of extractInheritance method
    pub(super) fn extract_inheritance(&self, node: Node) -> Option<String> {
        // First try the standard type_inheritance_clause
        if let Some(inheritance) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_inheritance_clause")
        {
            let types: Vec<_> = inheritance
                .children(&mut inheritance.walk())
                .filter(|c| c.kind() == "type_identifier" || c.kind() == "type")
                .map(|t| self.base.get_node_text(&t))
                .collect();
            if !types.is_empty() {
                return Some(types.join(", "));
            }
        }

        // For Swift enums, inheritance is represented as direct inheritance_specifier nodes
        let inheritance_specifiers: Vec<_> = node
            .children(&mut node.walk())
            .filter(|c| c.kind() == "inheritance_specifier")
            .collect();
        if !inheritance_specifiers.is_empty() {
            let types: Vec<_> = inheritance_specifiers
                .iter()
                .filter_map(|spec| {
                    spec.children(&mut spec.walk())
                        .find(|c| matches!(c.kind(), "user_type" | "type_identifier" | "type"))
                        .map(|type_node| self.base.get_node_text(&type_node))
                })
                .collect();
            if !types.is_empty() {
                return Some(types.join(", "));
            }
        }

        None
    }

    /// Implementation of extractWhereClause method
    pub(super) fn extract_where_clause(&self, node: Node) -> Option<String> {
        // Look for where clause in class/function declarations
        if let Some(where_clause) = node.children(&mut node.walk()).find(|c| {
            matches!(
                c.kind(),
                "where_clause" | "generic_where_clause" | "type_constraints"
            ) || self.base.get_node_text(c).starts_with("where")
        }) {
            return Some(self.base.get_node_text(&where_clause));
        }

        // Fallback: scan for any child containing "where"
        for child in node.children(&mut node.walk()) {
            let text = self.base.get_node_text(&child);
            if text.contains("where ") {
                if let Some(captures) = text.find("where ") {
                    let where_part = &text[captures..];
                    if let Some(end) = where_part.find('{') {
                        return Some(where_part[..end].trim().to_string());
                    } else {
                        return Some(where_part.trim().to_string());
                    }
                }
            }
        }

        None
    }

    /// Implementation of extractParameters method
    pub(super) fn extract_parameters(&self, node: Node) -> Option<String> {
        // First try parameter_clause
        if let Some(param_clause) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "parameter_clause")
        {
            return Some(self.base.get_node_text(&param_clause));
        }

        // For Swift functions, parameters are individual nodes between ( and )
        let parameters: Vec<_> = node
            .children(&mut node.walk())
            .filter(|c| c.kind() == "parameter")
            .map(|p| self.base.get_node_text(&p))
            .collect();

        if !parameters.is_empty() {
            return Some(format!("({})", parameters.join(", ")));
        }

        // Check if there are parentheses (indicating a function with no parameters)
        if node.children(&mut node.walk()).any(|c| c.kind() == "(") {
            Some("()".to_string())
        } else {
            None
        }
    }

    /// Implementation of extractInitializerParameters method
    pub(super) fn extract_initializer_parameters(&self, node: Node) -> Option<String> {
        // Look for parameter nodes
        if let Some(parameter_node) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "parameter")
        {
            return Some(format!("({})", self.base.get_node_text(&parameter_node)));
        }

        // Check if there are parentheses but no parameters
        if node.children(&mut node.walk()).any(|c| c.kind() == "(") {
            Some("()".to_string())
        } else {
            None
        }
    }

    /// Implementation of extractReturnType method
    pub(super) fn extract_return_type(&self, node: Node) -> Option<String> {
        // Try function_type first
        if let Some(return_clause) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "function_type")
        {
            if let Some(type_node) = return_clause
                .children(&mut return_clause.walk())
                .find(|c| c.kind() == "type")
            {
                return Some(self.base.get_node_text(&type_node));
            }
        }

        // Try type_annotation
        if let Some(type_annotation) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_annotation")
        {
            if let Some(type_node) = type_annotation
                .children(&mut type_annotation.walk())
                .find(|c| matches!(c.kind(), "type" | "type_identifier" | "user_type"))
            {
                return Some(self.base.get_node_text(&type_node));
            }
        }

        // Try direct type nodes (for simple cases)
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        if let Some((node_index, direct_type)) = children
            .iter()
            .enumerate()
            .find(|(_, c)| matches!(c.kind(), "type" | "type_identifier" | "user_type"))
        {
            let has_arrow = children
                .iter()
                .take(node_index)
                .any(|child| self.base.get_node_text(child).contains("->"));
            if has_arrow {
                return Some(self.base.get_node_text(direct_type));
            }
        }

        None
    }

    /// Implementation of extractVariableType method
    pub(super) fn extract_variable_type(&self, node: Node) -> Option<String> {
        if let Some(type_annotation) = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_annotation")
        {
            if let Some(type_node) =
                type_annotation
                    .children(&mut type_annotation.walk())
                    .find(|c| {
                        matches!(
                            c.kind(),
                            "type"
                                | "user_type"
                                | "primitive_type"
                                | "optional_type"
                                | "function_type"
                                | "tuple_type"
                                | "dictionary_type"
                                | "array_type"
                        )
                    })
            {
                return Some(self.base.get_node_text(&type_node));
            }
        }
        None
    }

    /// Implementation of extractPropertyType method
    pub(super) fn extract_property_type(&self, node: Node) -> Option<String> {
        self.extract_variable_type(node)
    }

    /// Implementation of determineVisibility method
    pub(super) fn determine_visibility(&self, modifiers: &[String]) -> Visibility {
        if modifiers
            .iter()
            .any(|m| m == "private" || m == "fileprivate")
        {
            Visibility::Private
        } else if modifiers.iter().any(|m| m == "internal") {
            Visibility::Protected
        } else {
            Visibility::Public
        }
    }
}
