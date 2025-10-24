/// C# symbol extraction within Razor code blocks
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

impl super::RazorExtractor {
    /// Extract C# symbols from within code blocks
    pub(super) fn extract_csharp_symbols(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_csharp_node(child, symbols, parent_id);
        }
    }

    /// Visit a C# node and extract symbols
    pub(super) fn visit_csharp_node(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        let mut symbol = None;
        let current_parent_id = parent_id;

        match node.kind() {
            "local_declaration_statement" => {
                symbol = self.extract_local_variable(node, parent_id);
            }
            "method_declaration" => {
                symbol = self.extract_method(node, parent_id);
            }
            "local_function_statement" => {
                symbol = self.extract_local_function(node, parent_id);
            }
            "property_declaration" => {
                symbol = self.extract_property(node, parent_id);
            }
            "field_declaration" => {
                symbol = self.extract_field(node, parent_id);
            }
            "variable_declaration" => {
                symbol = self.extract_variable_declaration(node, parent_id);
            }
            "assignment_expression" => {
                symbol = self.extract_assignment(node, parent_id);
            }
            "invocation_expression" => {
                symbol = self.extract_invocation(node, parent_id);
            }
            "element_access_expression" => {
                symbol = self.extract_element_access(node, parent_id);
            }
            "class_declaration" => {
                symbol = self.extract_class(node, parent_id);
            }
            "namespace_declaration" => {
                symbol = self.extract_namespace(node, parent_id);
            }
            _ => {}
        }

        let new_parent_id = if let Some(sym) = &symbol {
            symbols.push(sym.clone());
            Some(sym.id.as_str())
        } else {
            current_parent_id
        };

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_csharp_node(child, symbols, new_parent_id);
        }
    }

    /// Extract using directive (namespace import)
    pub(super) fn extract_using(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let namespace_name = self.extract_namespace_name(node);

        // Extract C# XML doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            namespace_name.clone(),
            SymbolKind::Import,
            SymbolOptions {
                signature: Some(format!("@using {}", namespace_name)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("using-directive".to_string()),
                    );
                    metadata.insert(
                        "namespace".to_string(),
                        serde_json::Value::String(namespace_name),
                    );
                    metadata
                }),
                doc_comment,
            },
        ))
    }

    /// Extract namespace declaration
    pub(super) fn extract_namespace(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let name = if let Some(name_node) =
            self.find_child_by_types(node, &["qualified_name", "identifier"])
        {
            self.base.get_node_text(&name_node)
        } else {
            "UnknownNamespace".to_string()
        };

        // Extract C# XML doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            name.clone(),
            SymbolKind::Namespace,
            SymbolOptions {
                signature: Some(format!("@namespace {}", name)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("namespace".to_string()),
                    );
                    metadata
                }),
                doc_comment,
            },
        ))
    }

    /// Extract class declaration
    pub(super) fn extract_class(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name = if let Some(name_node) = self.find_child_by_type(node, "identifier") {
            self.base.get_node_text(&name_node)
        } else {
            "UnknownClass".to_string()
        };

        let modifiers = self.extract_modifiers(node);
        let mut signature = format!("class {}", name);
        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        // Extract C# XML doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("class".to_string()),
                    );
                    metadata.insert(
                        "modifiers".to_string(),
                        serde_json::Value::String(modifiers.join(", ")),
                    );
                    metadata
                }),
                doc_comment,
            },
        ))
    }

    /// Extract method declaration
    pub(super) fn extract_method(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut name = "unknownMethod".to_string();
        let mut interface_qualification = String::new();

        // Handle explicit interface implementations
        if let Some(explicit_impl) = self.find_child_by_type(node, "explicit_interface_specifier") {
            if let Some(interface_node) = self.find_child_by_type(explicit_impl, "identifier") {
                let interface_name = self.base.get_node_text(&interface_node);
                interface_qualification = format!("{}.", interface_name);
            }
        }

        // Find method name - should be the identifier immediately before parameter_list
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        if let Some(param_list_idx) = children.iter().position(|c| c.kind() == "parameter_list") {
            // Look backwards from parameter list to find the method name identifier
            for i in (0..param_list_idx).rev() {
                if children[i].kind() == "identifier" {
                    name = self.base.get_node_text(&children[i]);
                    break;
                }
            }
        } else {
            // Fallback: find the last identifier (which should be method name in most cases)
            for child in children.iter().rev() {
                if child.kind() == "identifier" {
                    name = self.base.get_node_text(child);
                    break;
                }
            }
        }

        let modifiers = self.extract_modifiers(node);
        let parameters = self.extract_method_parameters(node);
        let return_type = self.extract_return_type(node);
        let attributes = self.extract_attributes(node);

        let mut signature_parts = Vec::new();
        if !attributes.is_empty() {
            signature_parts.push(attributes.join(" "));
        }
        if !modifiers.is_empty() {
            signature_parts.push(modifiers.join(" "));
        }
        if let Some(ref ret_type) = return_type {
            signature_parts.push(ret_type.clone());
        }
        signature_parts.push(format!(
            "{}{}{}",
            interface_qualification,
            name,
            parameters.clone().unwrap_or_else(|| "()".to_string())
        ));

        // Extract C# XML doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Method,
            SymbolOptions {
                signature: Some(signature_parts.join(" ")),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("method".to_string()),
                    );
                    metadata.insert(
                        "modifiers".to_string(),
                        serde_json::Value::String(modifiers.join(", ")),
                    );
                    if let Some(params) = &parameters {
                        metadata.insert(
                            "parameters".to_string(),
                            serde_json::Value::String(params.clone()),
                        );
                    }
                    if let Some(ret_type) = return_type {
                        metadata.insert(
                            "returnType".to_string(),
                            serde_json::Value::String(ret_type),
                        );
                    }
                    metadata.insert(
                        "attributes".to_string(),
                        serde_json::Value::String(attributes.join(", ")),
                    );
                    metadata
                }),
                doc_comment,
            },
        ))
    }

    /// Extract property declaration
    pub(super) fn extract_property(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let mut name = "unknownProperty".to_string();

        // Find property name - should be after type but before accessors
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        for (i, child) in children.iter().enumerate() {
            if child.kind() == "identifier" {
                // Check if this identifier comes after a type node
                let has_preceding_type = children.iter().take(i).any(|c| {
                    matches!(
                        c.kind(),
                        "predefined_type"
                            | "nullable_type"
                            | "array_type"
                            | "generic_name"
                            | "identifier"
                    ) && children
                        .iter()
                        .take(i)
                        .any(|prev| prev.kind() == "modifier")
                });

                if has_preceding_type {
                    name = self.base.get_node_text(child);
                    break;
                }
            }
        }

        let modifiers = self.extract_modifiers(node);
        let property_type = self.extract_property_type(node);
        let attributes = self.extract_attributes(node);

        let mut signature_parts = Vec::new();
        if !attributes.is_empty() {
            signature_parts.push(attributes.join(" "));
        }
        if !modifiers.is_empty() {
            signature_parts.push(modifiers.join(" "));
        }
        if let Some(ref prop_type) = property_type {
            signature_parts.push(prop_type.clone());
        }
        signature_parts.push(name.clone());

        // Check for accessors
        if let Some(accessor_list) = self.find_child_by_type(node, "accessor_list") {
            let mut cursor = accessor_list.walk();
            let accessors: Vec<_> = accessor_list
                .children(&mut cursor)
                .filter(|c| {
                    matches!(
                        c.kind(),
                        "get_accessor_declaration" | "set_accessor_declaration"
                    )
                })
                .map(|c| {
                    if c.kind() == "get_accessor_declaration" {
                        "get"
                    } else {
                        "set"
                    }
                })
                .collect();

            if !accessors.is_empty() {
                signature_parts.push(format!("{{ {}; }}", accessors.join("; ")));
            }
        }

        // Check for initializer
        if self.find_child_by_type(node, "=").is_some() {
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            if let Some(equals_idx) = children.iter().position(|c| c.kind() == "=") {
                if equals_idx + 1 < children.len() {
                    let initializer = self.base.get_node_text(&children[equals_idx + 1]);
                    signature_parts.push(format!("= {}", initializer));
                }
            }
        }

        // Extract C# XML doc comment
        let doc_comment = self.base.find_doc_comment(&node);

        Some(self.base.create_symbol(
            &node,
            name,
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(signature_parts.join(" ")),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("property".to_string()),
                    );
                    metadata.insert(
                        "modifiers".to_string(),
                        serde_json::Value::String(modifiers.join(", ")),
                    );
                    if let Some(prop_type) = property_type {
                        metadata.insert(
                            "propertyType".to_string(),
                            serde_json::Value::String(prop_type),
                        );
                    }
                    metadata.insert(
                        "attributes".to_string(),
                        serde_json::Value::String(attributes.join(", ")),
                    );
                    metadata
                }),
                doc_comment,
            },
        ))
    }
}
