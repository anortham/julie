// Stub implementations for remaining Razor extractor methods
// Split from razor.rs to reduce file size and improve maintainability

use regex::Regex;
use std::sync::LazyLock;

// Static regexes compiled once for performance
static RAZOR_TYPE_PATTERN1: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\[\w+.*?\]\s+)?(?:public|private|protected|internal|static)\s+(\w+(?:<[^>]+>)?(?:\?|\[\])?)\s+\w+").unwrap()
});
static RAZOR_TYPE_PATTERN2: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:public|private|protected|internal|static|async)\s+(\w+(?:<[^>]+>)?)\s+\w+\s*\(").unwrap()
});
static RAZOR_TYPE_PATTERN3: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\w+(?:<[^>]+>)?(?:\?|\[\])?)\s+\w+\s*=").unwrap()
});

impl RazorExtractor {
    // Stub implementations for remaining methods
    pub fn extract_field(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract field name and type
        let mut field_name = "unknownField".to_string();
        let mut field_type = None;

        // Find variable declarator in field declaration
        if let Some(var_decl) = self.find_child_by_type(node, "variable_declaration") {
            // Extract type
            if let Some(type_node) = self.find_child_by_types(var_decl, &[
                "predefined_type", "identifier", "generic_name", "qualified_name",
                "nullable_type", "array_type"
            ]) {
                field_type = Some(self.base.get_node_text(&type_node));
            }

            // Find variable declarator(s)
            if let Some(var_declarator) = self.find_child_by_type(var_decl, "variable_declarator") {
                if let Some(identifier) = self.find_child_by_type(var_declarator, "identifier") {
                    field_name = self.base.get_node_text(&identifier);
                }
            }
        }

        let modifiers = self.extract_modifiers(node);
        let attributes = self.extract_attributes(node);

        let mut signature_parts = Vec::new();
        if !attributes.is_empty() {
            signature_parts.push(attributes.join(" "));
        }
        if !modifiers.is_empty() {
            signature_parts.push(modifiers.join(" "));
        }
        if let Some(ref f_type) = field_type {
            signature_parts.push(f_type.clone());
        }
        signature_parts.push(field_name.clone());

        Some(self.base.create_symbol(
            &node,
            field_name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature_parts.join(" ")),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("field".to_string()));
                    metadata.insert("modifiers".to_string(), serde_json::Value::String(modifiers.join(", ")));
                    if let Some(f_type) = field_type {
                        metadata.insert("fieldType".to_string(), serde_json::Value::String(f_type));
                    }
                    metadata.insert("attributes".to_string(), serde_json::Value::String(attributes.join(", ")));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    pub fn extract_local_function(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract function name using same logic as extract_method
        let mut name = "unknownFunction".to_string();

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
        } else {
            signature_parts.push("void".to_string()); // Default return type for local functions
        }
        signature_parts.push(format!("{}{}", name, parameters.clone().unwrap_or_else(|| "()".to_string())));

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
                    metadata.insert("type".to_string(), serde_json::Value::String("local-function".to_string()));
                    metadata.insert("modifiers".to_string(), serde_json::Value::String(modifiers.join(", ")));
                    if let Some(params) = &parameters {
                        metadata.insert("parameters".to_string(), serde_json::Value::String(params.clone()));
                    }
                    if let Some(ret_type) = return_type {
                        metadata.insert("returnType".to_string(), serde_json::Value::String(ret_type));
                    }
                    metadata.insert("attributes".to_string(), serde_json::Value::String(attributes.join(", ")));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    pub fn extract_local_variable(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract variable name and type from local declaration
        let mut variable_name = "unknownVariable".to_string();
        let mut variable_type = None;
        let mut initializer = None;

        // Find variable declarator
        if let Some(var_declarator) = self.find_child_by_type(node, "variable_declarator") {
            if let Some(identifier) = self.find_child_by_type(var_declarator, "identifier") {
                variable_name = self.base.get_node_text(&identifier);
            }

            // Look for initializer (= expression)
            let mut cursor = var_declarator.walk();
            let children: Vec<_> = var_declarator.children(&mut cursor).collect();
            if let Some(equals_pos) = children.iter().position(|c| c.kind() == "=") {
                if equals_pos + 1 < children.len() {
                    initializer = Some(self.base.get_node_text(&children[equals_pos + 1]));
                }
            }
        }

        // Find variable type declaration
        if let Some(var_decl) = self.find_child_by_type(node, "variable_declaration") {
            if let Some(type_node) = self.find_child_by_types(var_decl, &[
                "predefined_type", "identifier", "generic_name", "qualified_name",
                "nullable_type", "array_type"
            ]) {
                variable_type = Some(self.base.get_node_text(&type_node));
            }
        }

        let modifiers = self.extract_modifiers(node);
        let attributes = self.extract_attributes(node);

        let mut signature_parts = Vec::new();
        if !attributes.is_empty() {
            signature_parts.push(attributes.join(" "));
        }
        if !modifiers.is_empty() {
            signature_parts.push(modifiers.join(" "));
        }
        if let Some(ref var_type) = variable_type {
            signature_parts.push(var_type.clone());
        }
        signature_parts.push(variable_name.clone());
        if let Some(ref init) = initializer {
            signature_parts.push(format!("= {}", init));
        }

        Some(self.base.create_symbol(
            &node,
            variable_name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature_parts.join(" ")),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("local-variable".to_string()));
                    if let Some(var_type) = variable_type {
                        metadata.insert("variableType".to_string(), serde_json::Value::String(var_type));
                    }
                    if let Some(init) = initializer {
                        metadata.insert("initializer".to_string(), serde_json::Value::String(init));
                    }
                    metadata.insert("modifiers".to_string(), serde_json::Value::String(modifiers.join(", ")));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    pub fn extract_variable_declaration(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract variable name and type from variable declaration
        let mut variable_type = None;

        // Find the type (if present)
        if let Some(type_node) = self.find_child_by_types(node, &[
            "predefined_type", "identifier", "generic_name", "qualified_name",
            "nullable_type", "array_type", "var"
        ]) {
            let type_text = self.base.get_node_text(&type_node);
            if type_text != "var" {  // Don't use "var" as the actual type
                variable_type = Some(type_text);
            }
        }

        // Find variable declarators
        let mut declarators = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                if let Some(identifier) = self.find_child_by_type(child, "identifier") {
                    let name = self.base.get_node_text(&identifier);

                    // Look for initializer
                    let mut initializer = None;
                    let mut decl_cursor = child.walk();
                    let decl_children: Vec<_> = child.children(&mut decl_cursor).collect();
                    if let Some(equals_pos) = decl_children.iter().position(|c| c.kind() == "=") {
                        if equals_pos + 1 < decl_children.len() {
                            initializer = Some(self.base.get_node_text(&decl_children[equals_pos + 1]));
                        }
                    }

                    declarators.push((name, initializer));
                }
            }
        }

        // For now, handle the first declarator (most common case)
        if let Some((name, initializer)) = declarators.first() {
            let variable_name = name.clone();

            let mut signature_parts = Vec::new();
            if let Some(ref var_type) = variable_type {
                signature_parts.push(var_type.clone());
            } else {
                signature_parts.push("var".to_string());
            }
            signature_parts.push(variable_name.clone());
            if let Some(ref init) = initializer {
                signature_parts.push(format!("= {}", init));
            }

            Some(self.base.create_symbol(
                &node,
                variable_name,
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(signature_parts.join(" ")),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some({
                        let mut metadata = HashMap::new();
                        metadata.insert("type".to_string(), serde_json::Value::String("variable-declaration".to_string()));
                        if let Some(var_type) = variable_type {
                            metadata.insert("variableType".to_string(), serde_json::Value::String(var_type));
                        }
                        if let Some(init) = initializer {
                            metadata.insert("initializer".to_string(), serde_json::Value::String(init.clone()));
                        }
                        metadata
                    }),
                    doc_comment: None,
                },
            ))
        } else {
            None
        }
    }

    pub fn extract_assignment(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract left side (variable being assigned to) and right side (value)
        let mut left_side = None;
        let mut right_side = None;

        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // Find the assignment operator (=) and extract left/right sides
        if let Some(equals_pos) = children.iter().position(|c| c.kind() == "=") {
            if equals_pos > 0 {
                left_side = Some(self.base.get_node_text(&children[equals_pos - 1]));
            }
            if equals_pos + 1 < children.len() {
                right_side = Some(self.base.get_node_text(&children[equals_pos + 1]));
            }
        }

        if let (Some(left), Some(right)) = (&left_side, &right_side) {
            let signature = format!("{} = {}", left, right);
            let variable_name = if left.contains('[') {
                // Handle ViewData["Title"] -> extract as ViewData assignment
                left.split('[').next().unwrap_or(left).to_string()
            } else {
                left.clone()
            };

            Some(self.base.create_symbol(
                &node,
                variable_name,
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some({
                        let mut metadata = HashMap::new();
                        metadata.insert("type".to_string(), serde_json::Value::String("assignment".to_string()));
                        metadata.insert("leftSide".to_string(), serde_json::Value::String(left.clone()));
                        metadata.insert("rightSide".to_string(), serde_json::Value::String(right.clone()));
                        if left.contains("ViewData") {
                            metadata.insert("isViewData".to_string(), serde_json::Value::Bool(true));
                        }
                        if left.contains("ViewBag") {
                            metadata.insert("isViewBag".to_string(), serde_json::Value::Bool(true));
                        }
                        if left.contains("Layout") {
                            metadata.insert("isLayout".to_string(), serde_json::Value::Bool(true));
                        }
                        metadata
                    }),
                    doc_comment: None,
                },
            ))
        } else {
            None
        }
    }

    pub fn extract_invocation(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let _invocation_text = self.base.get_node_text(&node);

        // Extract method name and arguments
        let mut method_name = "unknownMethod".to_string();
        let mut arguments = None;

        // Look for the invoked expression (method name)
        if let Some(expression) = self.find_child_by_types(node, &["identifier", "member_access_expression"]) {
            method_name = self.base.get_node_text(&expression);
        }

        // Look for argument list
        if let Some(arg_list) = self.find_child_by_type(node, "argument_list") {
            arguments = Some(self.base.get_node_text(&arg_list));
        }

        let signature = if let Some(args) = &arguments {
            format!("{}{}", method_name, args)
        } else {
            format!("{}()", method_name)
        };

        Some(self.base.create_symbol(
            &node,
            method_name.clone(),
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("method-invocation".to_string()));
                    metadata.insert("methodName".to_string(), serde_json::Value::String(method_name.clone()));
                    if let Some(args) = arguments {
                        metadata.insert("arguments".to_string(), serde_json::Value::String(args));
                    }
                    // Detect special method types
                    if method_name.contains("Component.InvokeAsync") {
                        metadata.insert("isComponentInvocation".to_string(), serde_json::Value::Bool(true));
                    }
                    if method_name.contains("Html.Raw") {
                        metadata.insert("isHtmlHelper".to_string(), serde_json::Value::Bool(true));
                    }
                    if method_name.contains("RenderSectionAsync") {
                        metadata.insert("isRenderSection".to_string(), serde_json::Value::Bool(true));
                    }
                    if method_name.contains("RenderBody") {
                        metadata.insert("isRenderBody".to_string(), serde_json::Value::Bool(true));
                    }
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    pub fn extract_element_access(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Handle expressions like ViewData["Title"], ViewBag.MetaDescription
        let element_text = self.base.get_node_text(&node);

        let mut object_name = "unknown".to_string();
        let mut access_key = None;

        // Try to find the object being accessed
        if let Some(expression) = self.find_child_by_type(node, "identifier") {
            object_name = self.base.get_node_text(&expression);
        } else if let Some(member_access) = self.find_child_by_type(node, "member_access_expression") {
            object_name = self.base.get_node_text(&member_access);
        }

        // Try to find the access key
        if let Some(bracket_expr) = self.find_child_by_type(node, "bracket_expression") {
            access_key = Some(self.base.get_node_text(&bracket_expr));
        }

        let signature = element_text.clone();
        let symbol_name = if let Some(key) = &access_key {
            format!("{}[{}]", object_name, key)
        } else {
            object_name.clone()
        };

        Some(self.base.create_symbol(
            &node,
            symbol_name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("element-access".to_string()));
                    metadata.insert("objectName".to_string(), serde_json::Value::String(object_name.clone()));
                    if let Some(key) = access_key {
                        metadata.insert("accessKey".to_string(), serde_json::Value::String(key));
                    }
                    if object_name.contains("ViewData") {
                        metadata.insert("isViewData".to_string(), serde_json::Value::Bool(true));
                    }
                    if object_name.contains("ViewBag") {
                        metadata.insert("isViewBag".to_string(), serde_json::Value::Bool(true));
                    }
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    pub fn extract_html_attribute(&mut self, node: Node, parent_id: Option<&str>, symbols: &mut Vec<Symbol>) -> Option<Symbol> {
        let attribute_text = self.base.get_node_text(&node);

        // Extract attribute name and value
        let mut attr_name = None;
        let mut attr_value = None;

        if let Some(name_node) = self.find_child_by_type(node, "attribute_name") {
            attr_name = Some(self.base.get_node_text(&name_node));
        } else if let Some(identifier) = self.find_child_by_type(node, "identifier") {
            attr_name = Some(self.base.get_node_text(&identifier));
        }

        if let Some(value_node) = self.find_child_by_type(node, "attribute_value") {
            attr_value = Some(self.base.get_node_text(&value_node));
        } else if let Some(string_literal) = self.find_child_by_type(node, "string_literal") {
            attr_value = Some(self.base.get_node_text(&string_literal));
        }

        // If we can't parse structured, fall back to parsing the text
        if attr_name.is_none() {
            if let Some(captures) = regex::Regex::new(r"([^=]+)=(.*)").unwrap().captures(&attribute_text) {
                attr_name = Some(captures[1].trim().to_string());
                attr_value = Some(captures[2].trim().to_string());
            } else {
                attr_name = Some(attribute_text.clone());
            }
        }

        // Handle special binding attributes - create separate binding symbols
        if let Some(name) = &attr_name {
            if name.starts_with("@bind-Value") {
                if let Some(value) = &attr_value {
                    // Create a separate symbol for the binding
                    let binding_name = format!("{}_binding",
                        value.replace("\"", "").replace("Model.", "").replace(".", "_").to_lowercase());
                    let binding_signature = format!("{}={}", name, value);

                    let binding_symbol = self.base.create_symbol(
                        &node,
                        binding_name,
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(binding_signature.clone()),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: Some({
                                let mut metadata = HashMap::new();
                                metadata.insert("type".to_string(), serde_json::Value::String("data-binding".to_string()));
                                metadata.insert("bindingType".to_string(), serde_json::Value::String("two-way".to_string()));
                                metadata.insert("property".to_string(), serde_json::Value::String(value.clone()));
                                metadata
                            }),
                            doc_comment: None,
                        },
                    );
                    symbols.push(binding_symbol);
                }
            }

            // Handle event binding with custom event
            if name.starts_with("@bind-Value:event") {
                if let Some(value) = &attr_value {
                    let event_binding_name = format!("{}_event_binding",
                        value.replace("\"", "").to_lowercase());
                    let event_signature = format!("{}={}", name, value);

                    let event_symbol = self.base.create_symbol(
                        &node,
                        event_binding_name,
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(event_signature.clone()),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: Some({
                                let mut metadata = HashMap::new();
                                metadata.insert("type".to_string(), serde_json::Value::String("event-binding".to_string()));
                                metadata.insert("event".to_string(), serde_json::Value::String(value.clone()));
                                metadata
                            }),
                            doc_comment: None,
                        },
                    );
                    symbols.push(event_symbol);
                }
            }
        }

        // Return the regular attribute symbol
        if let Some(name) = attr_name {
            let signature = if let Some(value) = &attr_value {
                format!("{}={}", name, value)
            } else {
                name.clone()
            };

            Some(self.base.create_symbol(
                &node,
                name.clone(),
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some({
                        let mut metadata = HashMap::new();
                        metadata.insert("type".to_string(), serde_json::Value::String("html-attribute".to_string()));
                        metadata.insert("attributeName".to_string(), serde_json::Value::String(name.clone()));
                        if let Some(value) = attr_value {
                            metadata.insert("attributeValue".to_string(), serde_json::Value::String(value));
                        }
                        if name.starts_with("@bind") {
                            metadata.insert("isDataBinding".to_string(), serde_json::Value::String("true".to_string()));
                        }
                        if name.starts_with("@on") {
                            metadata.insert("isEventBinding".to_string(), serde_json::Value::Bool(true));
                        }
                        metadata
                    }),
                    doc_comment: None,
                },
            ))
        } else {
            None
        }
    }

    pub fn extract_razor_attribute(&mut self, _node: Node, _parent_id: Option<&str>) -> Option<Symbol> {
        None // TODO: Implement when needed
    }

    // Helper methods
    #[allow(clippy::manual_find)] // Manual loop required for borrow checker
    pub fn find_child_by_type<'a>(&self, node: Node<'a>, child_type: &str) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == child_type {
                return Some(child);
            }
        }
        None
    }

    #[allow(clippy::manual_find)] // Manual loop required for borrow checker
    pub fn find_child_by_types<'a>(&self, node: Node<'a>, child_types: &[&str]) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child_types.contains(&child.kind()) {
                return Some(child);
            }
        }
        None
    }

    pub fn extract_modifiers(&self, node: Node) -> Vec<String> {
        let mut modifiers = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let child_text = self.base.get_node_text(&child);
            let modifier_types = [
                "public", "private", "protected", "internal", "static",
                "virtual", "override", "abstract", "sealed", "async"
            ];
            if modifier_types.contains(&child.kind()) || modifier_types.contains(&child_text.as_str()) {
                modifiers.push(child_text);
            }
        }
        modifiers
    }

    pub fn extract_method_parameters(&self, node: Node) -> Option<String> {
        self.find_child_by_type(node, "parameter_list").map(|param_list| self.base.get_node_text(&param_list))
    }

    pub fn extract_return_type(&self, node: Node) -> Option<String> {
        let type_kinds = [
            "predefined_type", "identifier", "generic_name", "qualified_name",
            "nullable_type", "array_type"
        ];

        self.find_child_by_types(node, &type_kinds).map(|return_type| self.base.get_node_text(&return_type))
    }

    pub fn extract_property_type(&self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        for (i, child) in children.iter().enumerate() {
            // Skip attributes and modifiers
            if child.kind() == "attribute_list" ||
               ["public", "private", "protected", "internal", "static", "virtual", "override", "abstract", "sealed"]
                   .contains(&child.kind()) ||
               ["public", "private", "protected", "internal", "static", "virtual", "override", "abstract", "sealed"]
                   .contains(&self.base.get_node_text(child).as_str()) {
                continue;
            }

            // Look for type nodes
            if matches!(child.kind(), "predefined_type" | "nullable_type" | "array_type" | "generic_name") ||
               (child.kind() == "identifier" && i < children.len() - 2) {
                return Some(self.base.get_node_text(child));
            }
        }

        None
    }

    pub fn extract_attributes(&self, node: Node) -> Vec<String> {
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

    pub fn determine_visibility(&self, modifiers: &[String]) -> Visibility {
        if modifiers.iter().any(|m| m == "private") {
            Visibility::Private
        } else if modifiers.iter().any(|m| m == "protected") {
            Visibility::Protected
        } else {
            Visibility::Public
        }
    }

    pub fn extract_namespace_name(&self, node: Node) -> String {
        if let Some(name_node) = self.find_child_by_types(node, &["qualified_name", "identifier"]) {
            self.base.get_node_text(&name_node)
        } else {
            "UnknownNamespace".to_string()
        }
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn visit_relationships(&self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        match node.kind() {
            "razor_component" => self.extract_component_relationships(node, symbols, relationships),
            "using_directive" => self.extract_using_relationships(node, symbols, relationships),
            "html_element" | "element" => self.extract_element_relationships(node, symbols, relationships),
            "identifier" => self.extract_identifier_component_relationships(node, symbols, relationships),
            "invocation_expression" => self.extract_invocation_relationships(node, symbols, relationships),
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_relationships(child, symbols, relationships);
        }
    }

    fn extract_component_relationships(&self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        // Extract relationships between Razor components
        let _element_text = self.base.get_node_text(&node);

        // Look for component tag names (uppercase elements indicate components)
        if let Some(name_node) = self.find_child_by_type(node, "identifier") {
            let component_name = self.base.get_node_text(&name_node);

            // Find the using component (from symbols) - prefer the main page/component
            let from_symbol = symbols.iter().find(|s| s.kind == SymbolKind::Class)
                .or_else(|| symbols.iter().find(|s| s.signature.as_ref().is_some_and(|sig| sig.contains("@page"))))
                .or_else(|| symbols.iter().find(|s| s.kind == SymbolKind::Module));

            if let Some(from_sym) = from_symbol {
                // Create synthetic relationship to used component
                let to_symbol_id = format!("component-{}", component_name);

                relationships.push(self.base.create_relationship(
                    from_sym.id.clone(),
                    to_symbol_id,
                    crate::extractors::base::RelationshipKind::Uses,
                    &node,
                    Some(1.0),
                    Some({
                        let mut metadata = HashMap::new();
                        metadata.insert("component".to_string(), serde_json::Value::String(component_name.clone()));
                        metadata.insert("type".to_string(), serde_json::Value::String("component-usage".to_string()));
                        metadata
                    }),
                ));
            }
        }
    }

    fn extract_using_relationships(&self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        // Extract using directive relationships
        if let Some(qualified_name) = self.find_child_by_type(node, "qualified_name") {
            let namespace_name = self.base.get_node_text(&qualified_name);

            // Find any symbol that could be using this namespace
            if let Some(from_symbol) = symbols.iter().find(|s| s.kind == SymbolKind::Class) {
                relationships.push(self.base.create_relationship(
                    from_symbol.id.clone(),
                    format!("using-{}", namespace_name), // Create synthetic ID for namespaces
                    crate::extractors::base::RelationshipKind::Uses,
                    &node,
                    Some(0.8),
                    Some({
                        let mut metadata = HashMap::new();
                        metadata.insert("namespace".to_string(), serde_json::Value::String(namespace_name));
                        metadata.insert("type".to_string(), serde_json::Value::String("using-directive".to_string()));
                        metadata
                    }),
                ));
            }
        }
    }

    fn extract_element_relationships(&self, node: Node, symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
        // Extract relationships from HTML elements that might bind to properties
        let element_text = self.base.get_node_text(&node);

        // Check for component usage using regex to find all components in the element
        if let Ok(component_regex) = regex::Regex::new(r"<([A-Z][A-Za-z0-9]*)\b") {
            for captures in component_regex.captures_iter(&element_text) {
                if let Some(tag_match) = captures.get(1) {
                    let tag_name = tag_match.as_str();

                    if let Some(from_symbol) = symbols.iter().find(|s| s.kind == SymbolKind::Class)
                        .or_else(|| symbols.iter().find(|s| s.signature.as_ref().is_some_and(|sig| sig.contains("@page")))) {

                        // Find the component symbol (should exist now due to symbol extraction)
                        if let Some(component_symbol) = symbols.iter().find(|s| s.name == tag_name) {
                            relationships.push(self.base.create_relationship(
                                from_symbol.id.clone(),
                                component_symbol.id.clone(),
                                crate::extractors::base::RelationshipKind::Uses,
                                &node,
                                Some(1.0),
                                Some({
                                    let mut metadata = HashMap::new();
                                    metadata.insert("component".to_string(), serde_json::Value::String(tag_name.to_string()));
                                    metadata.insert("type".to_string(), serde_json::Value::String("component-usage".to_string()));
                                    metadata
                                }),
                            ));
                        }
                    }
                }
            }
        }

        // Check for data binding attributes (e.g., @bind-Value)
        if element_text.contains("@bind") {
            if let Some(from_symbol) = symbols.iter().find(|s| s.kind == SymbolKind::Class) {
                // Extract property being bound
                if let Some(captures) = regex::Regex::new(r"@bind-(\w+)").unwrap().captures(&element_text) {
                    if let Some(property_match) = captures.get(1) {
                        let property_name = property_match.as_str().to_string();

                        relationships.push(self.base.create_relationship(
                            from_symbol.id.clone(),
                            format!("property-{}", property_name), // Create synthetic ID for bound properties
                            crate::extractors::base::RelationshipKind::Uses,
                            &node,
                            Some(0.9),
                            Some({
                                let mut metadata = HashMap::new();
                                metadata.insert("property".to_string(), serde_json::Value::String(property_name));
                                metadata.insert("type".to_string(), serde_json::Value::String("data-binding".to_string()));
                                metadata
                            }),
                        ));
                    }
                }
            }
        }

        // Check for event binding attributes (e.g., @onclick)
        if element_text.contains("@on") {
            if let Some(from_symbol) = symbols.iter().find(|s| s.kind == SymbolKind::Class) {
                if let Some(captures) = regex::Regex::new(r"@on(\w+)").unwrap().captures(&element_text) {
                    if let Some(event_match) = captures.get(1) {
                        let event_name = event_match.as_str().to_string();

                        relationships.push(self.base.create_relationship(
                            from_symbol.id.clone(),
                            format!("event-{}", event_name), // Create synthetic ID for events
                            crate::extractors::base::RelationshipKind::Uses,
                            &node,
                            Some(0.9),
                            Some({
                                let mut metadata = HashMap::new();
                                metadata.insert("event".to_string(), serde_json::Value::String(event_name));
                                metadata.insert("type".to_string(), serde_json::Value::String("event-binding".to_string()));
                                metadata
                            }),
                        ));
                    }
                }
            }
        }
    }

    fn extract_identifier_component_relationships(&self, _node: Node, _symbols: &[Symbol], _relationships: &mut Vec<Relationship>) {
        // TODO: Implement identifier component relationships
    }

    fn extract_invocation_relationships(&self, _node: Node, _symbols: &[Symbol], _relationships: &mut Vec<Relationship>) {
        // TODO: Implement invocation relationships
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            let mut inferred_type = "unknown".to_string();

            // Use actual type information from metadata
            let metadata = &symbol.metadata;
            if let Some(property_type) = metadata.as_ref().and_then(|m| m.get("propertyType")).and_then(|v| v.as_str()) {
                    inferred_type = property_type.to_string();
                } else if let Some(field_type) = metadata.as_ref().and_then(|m| m.get("fieldType")).and_then(|v| v.as_str()) {
                    inferred_type = field_type.to_string();
                } else if let Some(variable_type) = metadata.as_ref().and_then(|m| m.get("variableType")).and_then(|v| v.as_str()) {
                    inferred_type = variable_type.to_string();
                } else if let Some(return_type) = metadata.as_ref().and_then(|m| m.get("returnType")).and_then(|v| v.as_str()) {
                    inferred_type = return_type.to_string();
                } else if let Some(signature) = &symbol.signature {
                    // Try to extract type from signature
                    let type_patterns: Vec<&Regex> = vec![
                        &*RAZOR_TYPE_PATTERN1,
                        &*RAZOR_TYPE_PATTERN2,
                        &*RAZOR_TYPE_PATTERN3,
                    ];

                    for pattern in &type_patterns {
                        if let Some(captures) = pattern.captures(signature) {
                            if let Some(type_match) = captures.get(1) {
                                let matched_type = type_match.as_str();
                                if matched_type != symbol.name {
                                    inferred_type = matched_type.to_string();
                                    break;
                                }
                            }
                        }
                    }
                }

            // Handle special cases
            if metadata.as_ref().and_then(|m| m.get("isDataBinding"))
                .and_then(|v| v.as_str()) == Some("true") {
                inferred_type = "bool".to_string();
            }

            if symbol.kind == SymbolKind::Method {
                if let Some(signature) = &symbol.signature {
                    if signature.contains("async Task") {
                        inferred_type = "Task".to_string();
                    } else if signature.contains("void") {
                        inferred_type = "void".to_string();
                    }
                }
            }

            types.insert(symbol.id.clone(), inferred_type);
        }
        types
    }
}