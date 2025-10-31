use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Function and method extraction for Go
impl super::GoExtractor {
    pub(super) fn extract_function(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let mut cursor = node.walk();
        let mut func_name = None;
        let mut type_parameters = None;
        let mut parameters = Vec::new();
        let mut return_type = None;
        let mut param_list_found = false;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => func_name = Some(self.get_node_text(child)),
                "type_parameter_list" => type_parameters = Some(self.get_node_text(child)),
                "parameter_list" => {
                    parameters = self.extract_parameter_list(child);
                    param_list_found = true;
                }
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "channel_type" | "interface_type" | "function_type" | "map_type"
                | "array_type" | "qualified_type" | "generic_type" => {
                    // Only treat as return type if we've seen parameters already
                    if param_list_found {
                        return_type = Some(self.extract_type_from_node(child));
                    }
                }
                _ => {}
            }
        }

        let name = func_name.unwrap_or_else(|| "anonymous".to_string());
        let visibility = if name == "main" || name == "init" {
            Some(Visibility::Private) // Special Go functions
        } else if self.is_public(&name) {
            Some(Visibility::Public)
        } else {
            Some(Visibility::Private)
        };

        let type_params = type_parameters.unwrap_or_default();
        let signature = self.build_function_signature_with_generics(
            "func",
            &name,
            &type_params,
            &parameters,
            return_type.as_deref(),
        );

        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility,
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment,
            },
        )
    }

    pub(super) fn extract_method(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let mut cursor = node.walk();
        let mut receiver = None;
        let mut func_name = None;
        let mut type_parameters = None;
        let mut parameters = Vec::new();
        let mut return_types = Vec::new();
        let mut param_lists_found = 0;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "parameter_list" => {
                    param_lists_found += 1;
                    if param_lists_found == 1 {
                        // First parameter list is the receiver
                        let receiver_params = self.extract_parameter_list(child);
                        if !receiver_params.is_empty() {
                            receiver = Some(receiver_params[0].clone());
                        }
                    } else if param_lists_found == 2 {
                        // Second parameter list is the actual parameters
                        parameters = self.extract_parameter_list(child);
                    } else if param_lists_found == 3 {
                        // Third parameter list is the return types (Go methods can have 3 parameter lists)
                        return_types = self.extract_parameter_list(child);
                    }
                }
                "field_identifier" => func_name = Some(self.get_node_text(child)), // Uses field_identifier for method names
                "type_parameter_list" => type_parameters = Some(self.get_node_text(child)),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "channel_type" | "interface_type" | "function_type" | "map_type"
                | "array_type" | "qualified_type" | "generic_type" => {
                    // Only treat as return type if we've seen parameters already
                    if param_lists_found >= 2 {
                        return_types.push(self.extract_type_from_node(child));
                    }
                }
                _ => {}
            }
        }

        let name = func_name.unwrap_or_else(|| "anonymous".to_string());
        let visibility = if self.is_public(&name) {
            Some(Visibility::Public)
        } else {
            Some(Visibility::Private)
        };

        let type_params = type_parameters.unwrap_or_default();

        let signature = if let Some(recv) = receiver {
            format!(
                "func ({}) {}{}",
                recv,
                name,
                self.build_method_signature_with_return_types(
                    &type_params,
                    &parameters,
                    &return_types
                )
            )
        } else {
            self.build_function_signature_with_return_types(
                "func",
                &name,
                &parameters,
                &return_types,
            )
        };

        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature),
                visibility,
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: None,
                doc_comment,
            },
        )
    }

    pub(super) fn extract_parameter_list(&self, node: Node) -> Vec<String> {
        let mut parameters = Vec::new();
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "parameter_declaration" | "variadic_parameter_declaration" => {
                    let param = self.extract_parameter_declaration(child);
                    if !param.is_empty() {
                        parameters.push(param);
                    }
                }
                _ => {}
            }
        }

        parameters
    }

    pub(super) fn extract_parameter_declaration(&self, node: Node) -> String {
        // Handle variadic parameter declarations
        if node.kind() == "variadic_parameter_declaration" {
            return self.get_node_text(node);
        }

        let mut names = Vec::new();
        let mut param_type = None;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => names.push(self.get_node_text(child)),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "map_type" | "channel_type" | "interface_type" | "function_type"
                | "array_type" | "qualified_type" | "generic_type" => {
                    param_type = Some(self.extract_type_from_node(child));
                }
                "variadic_parameter" => {
                    // Handle variadic parameters like ...interface{}
                    let variadic_text = self.get_node_text(child);
                    param_type = Some(variadic_text);
                }
                _ => {}
            }
        }

        if let Some(typ) = param_type {
            if names.is_empty() {
                typ // Anonymous parameter
            } else {
                format!("{} {}", names.join(", "), typ)
            }
        } else if !names.is_empty() {
            names[0].clone() // Just the name if no type found
        } else {
            String::new()
        }
    }

    pub(super) fn extract_from_error_node(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Try to recover function signatures from ERROR nodes
        // Look for identifier + parenthesized_type pattern (function signature)
        let mut cursor = node.walk();
        let mut identifier = None;
        let mut param_type = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => identifier = Some(child),
                "parenthesized_type" => param_type = Some(child),
                _ => {}
            }
        }

        if let (Some(id_node), Some(param_node)) = (identifier, param_type) {
            let name = self.get_node_text(id_node);
            let params = self.get_node_text(param_node);

            // This looks like a function signature trapped in an ERROR node
            let signature = format!("func {}{}", name, params);

            let doc_comment = self.base.find_doc_comment(&node);

            return Some(self.base.create_symbol(
                &node,
                name.clone(),
                SymbolKind::Function,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: if self.is_public(&name) {
                        Some(Visibility::Public)
                    } else {
                        Some(Visibility::Private)
                    },
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: None,
                    doc_comment,
                },
            ));
        }

        None
    }
}
