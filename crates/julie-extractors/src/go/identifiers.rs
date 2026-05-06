use crate::base::{BaseExtractor, IdentifierKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

/// Identifier extraction for LSP-quality find_references
impl super::GoExtractor {
    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub(super) fn walk_tree_for_identifiers(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        // Extract identifier from this node if applicable
        self.extract_identifier_from_node(node, symbol_map);

        // Recursively walk children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree_for_identifiers(child, symbol_map);
        }
    }

    /// Extract identifier from a single node based on its kind
    pub(super) fn extract_identifier_from_node(
        &mut self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) {
        match node.kind() {
            // Function/method calls: foo(), bar.Baz()
            "call_expression" => {
                // The function being called is typically the first child or in a selector
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "identifier" => {
                            // Simple function call: foo()
                            let name = self.base.get_node_text(&child);
                            let containing_symbol_id =
                                self.find_containing_symbol_id(node, symbol_map);

                            self.base.create_identifier(
                                &child,
                                name,
                                IdentifierKind::Call,
                                containing_symbol_id,
                            );
                            break;
                        }
                        "selector_expression" => {
                            // Method call: obj.Method()
                            // Extract the rightmost identifier (the method name)
                            if let Some(field_node) = child.child_by_field_name("field") {
                                let name = self.base.get_node_text(&field_node);
                                let containing_symbol_id =
                                    self.find_containing_symbol_id(node, symbol_map);

                                self.base.create_identifier(
                                    &field_node,
                                    name,
                                    IdentifierKind::Call,
                                    containing_symbol_id,
                                );
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            }

            // Member access: object.Field
            "selector_expression" => {
                // Only extract if it's NOT part of a call_expression
                // (we handle those in the call_expression case above)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "call_expression" {
                        return; // Skip - handled by call_expression
                    }
                }

                // Extract the rightmost identifier (the field name)
                if let Some(field_node) = node.child_by_field_name("field") {
                    let name = self.base.get_node_text(&field_node);
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &field_node,
                        name,
                        IdentifierKind::MemberAccess,
                        containing_symbol_id,
                    );
                }
            }

            "type_identifier" => {
                let name = self.base.get_node_text(&node);
                if is_go_type_usage_identifier(&self.base, node) && !is_go_builtin_type(&name) {
                    let containing_symbol_id = self.find_containing_symbol_id(node, symbol_map);

                    self.base.create_identifier(
                        &node,
                        name,
                        IdentifierKind::TypeUsage,
                        containing_symbol_id,
                    );
                }
            }

            _ => {}
        }
    }

    /// Find the ID of the symbol that contains this node
    /// CRITICAL: Only search symbols from THIS FILE (file-scoped filtering)
    pub(super) fn find_containing_symbol_id(
        &self,
        node: Node,
        symbol_map: &HashMap<String, &Symbol>,
    ) -> Option<String> {
        self.base
            .find_containing_symbol_from_map(&node, symbol_map)
            .map(|s| s.id.clone())
    }
}

fn is_go_type_usage_identifier(base: &BaseExtractor, node: Node) -> bool {
    if is_go_declaration_type_name(node) || has_go_error_ancestor(node) {
        return false;
    }

    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "field_declaration" {
            if is_embedded_go_field_type(base, parent, node) {
                return true;
            }

            if let (Some(name_node), Some(type_node)) = (
                parent.child_by_field_name("name"),
                parent.child_by_field_name("type"),
            ) {
                return name_node.id() != node.id() && contains_node(type_node, node);
            }

            return false;
        }

        if let Some(type_node) = parent.child_by_field_name("type") {
            if contains_node(type_node, node) {
                return true;
            }
        }

        match parent.kind() {
            "qualified_type"
            | "pointer_type"
            | "slice_type"
            | "array_type"
            | "map_type"
            | "channel_type"
            | "generic_type"
            | "parameter_declaration"
            | "variadic_parameter_declaration" => return true,
            "selector_expression"
            | "call_expression"
            | "argument_list"
            | "statement_list"
            | "source_file" => return false,
            _ => {}
        }

        current = parent;
    }

    false
}

fn is_embedded_go_field_type(base: &BaseExtractor, parent: Node, node: Node) -> bool {
    let mut cursor = parent.walk();
    let named_children: Vec<_> = parent.named_children(&mut cursor).collect();
    if named_children.len() != 1 || named_children[0].id() != node.id() {
        return false;
    }

    if has_prior_recovery_error_sibling(parent) {
        return false;
    }

    let field_text = base.get_node_text(&parent);
    !field_text.contains('(') && !field_text.contains(',') && !field_text.contains("...")
}

fn has_prior_recovery_error_sibling(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    let mut cursor = parent.walk();
    for sibling in parent.children(&mut cursor) {
        if sibling.id() == node.id() {
            return false;
        }
        if sibling.kind() == "ERROR" || sibling.is_error() || sibling.is_missing() {
            return true;
        }
    }

    false
}

fn is_go_declaration_type_name(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if let Some(name_node) = parent.child_by_field_name("name") {
        if name_node.id() == node.id() {
            return matches!(
                parent.kind(),
                "type_spec"
                    | "type_parameter_declaration"
                    | "field_declaration"
                    | "function_declaration"
                    | "method_declaration"
                    | "parameter_declaration"
                    | "variadic_parameter_declaration"
            );
        }
    }

    matches!(parent.kind(), "type_parameter_list")
}

fn has_go_error_ancestor(node: Node) -> bool {
    let mut current = Some(node);
    while let Some(node) = current {
        if node.kind() == "ERROR" || node.is_error() || node.is_missing() {
            return true;
        }
        current = node.parent();
    }

    false
}

fn contains_node(parent: Node, child: Node) -> bool {
    child.start_byte() >= parent.start_byte() && child.end_byte() <= parent.end_byte()
}

fn is_go_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "any"
            | "bool"
            | "byte"
            | "comparable"
            | "complex64"
            | "complex128"
            | "error"
            | "float32"
            | "float64"
            | "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "rune"
            | "string"
            | "uint"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "uintptr"
    )
}
