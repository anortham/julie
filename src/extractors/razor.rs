use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, SymbolOptions, Visibility};
use tree_sitter::{Tree, Node};
use std::collections::HashMap;

pub struct RazorExtractor {
    base: BaseExtractor,
}

impl RazorExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<String>) {
        if !self.is_valid_node(&node) {
            return;
        }

        let mut symbol = None;
        let node_type = node.kind();




        match node_type {
            "razor_directive" | "razor_inject_directive" | "razor_using_directive"
            | "razor_page_directive" | "razor_namespace_directive" | "razor_model_directive"
            | "razor_attribute_directive" | "razor_inherits_directive" | "razor_implements_directive"
            | "razor_addtaghelper_directive" => {
                symbol = self.extract_directive(node, parent_id.as_deref());
            }
            "at_namespace" | "at_inherits" | "at_implements" => {
                symbol = self.extract_token_directive(node, parent_id.as_deref());
            }
            "razor_section" => {
                symbol = self.extract_section(node, parent_id.as_deref());
            }
            "razor_block" => {
                symbol = self.extract_code_block(node, parent_id.as_deref());
                // Extract C# symbols from within the block
                self.extract_csharp_symbols(node, symbols, symbol.as_ref().map(|s| s.id.as_str()).or(parent_id.as_deref()));
                // Don't visit children since we already extracted them
                return;
            }
            "razor_expression" | "razor_implicit_expression" => {
                symbol = self.extract_expression(node, parent_id.as_deref());
            }
            "html_element" | "element" => {
                symbol = self.extract_html_element(node, parent_id.as_deref());
                // Also extract binding attributes from HTML elements
                self.extract_binding_attributes_from_element(node, symbols, parent_id.as_deref());
                // Create external component symbols for uppercase tag names (Miller's approach)
                self.create_external_component_symbols_if_needed(node, symbols);
            }
            "razor_component" => {
                symbol = self.extract_component(node, parent_id.as_deref());
                // Also create external component symbols for razor components
                self.create_external_component_symbols_if_needed(node, symbols);
            }
            "csharp_code" => {
                self.extract_csharp_symbols(node, symbols, parent_id.as_deref());
            }
            "using_directive" => {
                symbol = self.extract_using(node, parent_id.as_deref());
            }
            "namespace_declaration" => {
                symbol = self.extract_namespace(node, parent_id.as_deref());
            }
            "class_declaration" => {
                symbol = self.extract_class(node, parent_id.as_deref());
            }
            "method_declaration" => {
                symbol = self.extract_method(node, parent_id.as_deref());
            }
            "property_declaration" => {
                symbol = self.extract_property(node, parent_id.as_deref());
            }
            "field_declaration" => {
                symbol = self.extract_field(node, parent_id.as_deref());
            }
            "local_function_statement" => {
                symbol = self.extract_local_function(node, parent_id.as_deref());
            }
            "local_declaration_statement" => {
                symbol = self.extract_local_variable(node, parent_id.as_deref());
            }
            "assignment_expression" => {
                symbol = self.extract_assignment(node, parent_id.as_deref());
            }
            "invocation_expression" => {
                symbol = self.extract_invocation(node, parent_id.as_deref());
            }
            "razor_html_attribute" => {
                symbol = self.extract_html_attribute(node, parent_id.as_deref(), symbols);
            }
            "attribute" => {
                symbol = self.extract_razor_attribute(node, parent_id.as_deref());
            }
            _ => {}
        }

        let current_parent_id = if let Some(sym) = &symbol {
            symbols.push(sym.clone());
            Some(sym.id.clone())
        } else {
            parent_id
        };

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, current_parent_id.clone());
        }
    }

    fn is_valid_node(&self, node: &Node) -> bool {
        !node.kind().is_empty() && !node.is_error()
    }

    fn extract_directive(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let directive_name = self.extract_directive_name(node);
        let directive_value = self.extract_directive_value(node);

        let mut signature = format!("@{}", directive_name);
        if let Some(value) = &directive_value {
            signature.push_str(&format!(" {}", value));
        }

        let symbol_kind = self.get_directive_symbol_kind(&directive_name);

        // For certain directives, use the value as the symbol name
        let symbol_name = match directive_name.as_str() {
            "using" => directive_value.as_ref().map(|s| s.clone()).unwrap_or_else(|| format!("@{}", directive_name)),
            "inject" => {
                // Extract property name from "@inject IService PropertyName"
                if let Some(value) = &directive_value {
                    let parts: Vec<&str> = value.trim().split_whitespace().collect();
                    if parts.len() >= 2 {
                        parts.last().unwrap().to_string()
                    } else {
                        format!("@{}", directive_name)
                    }
                } else {
                    format!("@{}", directive_name)
                }
            }
            _ => format!("@{}", directive_name),
        };

        Some(self.base.create_symbol(
            &node,
            symbol_name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("razor-directive".to_string()));
                    metadata.insert("directiveName".to_string(), serde_json::Value::String(directive_name.clone()));
                    if let Some(value) = directive_value {
                        metadata.insert("directiveValue".to_string(), serde_json::Value::String(value));
                    }
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_directive_name(&self, node: Node) -> String {
        match node.kind() {
            "razor_page_directive" => "page".to_string(),
            "razor_model_directive" => "model".to_string(),
            "razor_using_directive" => "using".to_string(),
            "razor_inject_directive" => "inject".to_string(),
            "razor_attribute_directive" => "attribute".to_string(),
            "razor_namespace_directive" => "namespace".to_string(),
            "razor_inherits_directive" => "inherits".to_string(),
            "razor_implements_directive" => "implements".to_string(),
            "razor_addtaghelper_directive" => "addTagHelper".to_string(),
            _ => {
                let text = self.base.get_node_text(&node);
                if text.contains("@addTagHelper") {
                    "addTagHelper".to_string()
                } else if let Some(captures) = regex::Regex::new(r"@(\w+)").unwrap().captures(&text) {
                    captures[1].to_string()
                } else {
                    "unknown".to_string()
                }
            }
        }
    }

    fn extract_directive_value(&self, node: Node) -> Option<String> {
        match node.kind() {
            "razor_page_directive" => {
                self.find_child_by_type(node, "string_literal")
                    .map(|n| self.base.get_node_text(&n))
            }
            "razor_model_directive" | "razor_inherits_directive" | "razor_implements_directive" => {
                self.find_child_by_type(node, "identifier")
                    .map(|n| self.base.get_node_text(&n))
            }
            "razor_using_directive" | "razor_namespace_directive" => {
                self.find_child_by_types(node, &["qualified_name", "identifier"])
                    .map(|n| self.base.get_node_text(&n))
            }
            "razor_inject_directive" => {
                self.find_child_by_type(node, "variable_declaration")
                    .map(|n| self.base.get_node_text(&n))
            }
            "razor_attribute_directive" => {
                self.find_child_by_type(node, "attribute_list")
                    .map(|n| self.base.get_node_text(&n))
            }
            "razor_addtaghelper_directive" => {
                let text = self.base.get_node_text(&node);
                if let Some(captures) = regex::Regex::new(r"@addTagHelper\s+(.+)").unwrap().captures(&text) {
                    Some(captures[1].trim().to_string())
                } else {
                    None
                }
            }
            _ => {
                let text = self.base.get_node_text(&node);
                if text.contains("@addTagHelper") {
                    if let Some(captures) = regex::Regex::new(r"@addTagHelper\s+(.+)").unwrap().captures(&text) {
                        Some(captures[1].trim().to_string())
                    } else {
                        None
                    }
                } else if let Some(captures) = regex::Regex::new(r"@\w+\s+(.*)").unwrap().captures(&text) {
                    Some(captures[1].trim().to_string())
                } else {
                    None
                }
            }
        }
    }

    fn get_directive_symbol_kind(&self, directive_name: &str) -> SymbolKind {
        match directive_name.to_lowercase().as_str() {
            "model" | "layout" => SymbolKind::Class,
            "page" | "using" | "namespace" => SymbolKind::Import,
            "inherits" | "implements" => SymbolKind::Interface,
            "inject" | "attribute" => SymbolKind::Property,
            "code" | "functions" => SymbolKind::Function,
            _ => SymbolKind::Variable,
        }
    }

    fn extract_token_directive(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let directive_type = node.kind().replace("at_", "");
        let directive_name = format!("@{}", directive_type);

        // Look for the directive value in siblings
        let directive_value = if let Some(parent) = node.parent() {
            let text = self.base.get_node_text(&parent);
            if let Some(captures) = regex::Regex::new(&format!(r"@{}\s+(\S+)", directive_type))
                .unwrap().captures(&text) {
                Some(captures[1].to_string())
            } else {
                None
            }
        } else {
            None
        };

        let signature = if let Some(ref value) = directive_value {
            format!("{} {}", directive_name, value)
        } else {
            directive_name.clone()
        };

        let symbol_kind = self.get_directive_symbol_kind(&directive_type);

        Some(self.base.create_symbol(
            &node,
            directive_name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("razor-token-directive".to_string()));
                    metadata.insert("directiveType".to_string(), serde_json::Value::String(directive_type.clone()));
                    if let Some(value) = directive_value {
                        metadata.insert("directiveValue".to_string(), serde_json::Value::String(value));
                    }
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_section(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let identifier_node = self.find_child_by_type(node, "identifier")?;
        let section_name = self.base.get_node_text(&identifier_node);
        let signature = format!("@section {}", section_name);

        Some(self.base.create_symbol(
            &node,
            section_name,
            SymbolKind::Module,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("razor-section".to_string()));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_code_block(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let block_type = self.get_code_block_type(node);
        let content = self.base.get_node_text(&node);
        let truncated_content = if content.len() > 50 {
            format!("{}...", &content[..50])
        } else {
            content.clone()
        };

        let signature = format!("@{{ {} }}", truncated_content);

        Some(self.base.create_symbol(
            &node,
            format!("{}Block", block_type),
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("razor-code-block".to_string()));
                    metadata.insert("blockType".to_string(), serde_json::Value::String(block_type.clone()));
                    metadata.insert("content".to_string(), serde_json::Value::String(content[..content.len().min(200)].to_string()));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn get_code_block_type(&self, node: Node) -> String {
        let text = self.base.get_node_text(&node);
        if text.contains("@code") {
            "code".to_string()
        } else if text.contains("@functions") {
            "functions".to_string()
        } else if text.contains("@{") {
            "expression".to_string()
        } else {
            "block".to_string()
        }
    }

    fn extract_expression(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let expression = self.base.get_node_text(&node);
        let variable_name = self.extract_variable_from_expression(&expression)
            .unwrap_or_else(|| "expression".to_string());

        Some(self.base.create_symbol(
            &node,
            variable_name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(format!("@{}", expression)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("razor-expression".to_string()));
                    metadata.insert("expression".to_string(), serde_json::Value::String(expression.clone()));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_variable_from_expression(&self, expression: &str) -> Option<String> {
        if let Some(captures) = regex::Regex::new(r"(\w+)").unwrap().captures(expression) {
            Some(captures[1].to_string())
        } else {
            None
        }
    }

    fn create_external_component_symbols_if_needed(&mut self, node: Node, symbols: &mut Vec<Symbol>) {
        let node_text = self.base.get_node_text(&node);

        // Use regex to find all component tags within the element (Miller's approach)
        if let Ok(component_regex) = regex::Regex::new(r"<([A-Z][A-Za-z0-9]*)\b") {
            for captures in component_regex.captures_iter(&node_text) {
                if let Some(tag_match) = captures.get(1) {
                    let tag_name = tag_match.as_str();

                    // Check if symbol already exists
                    if !symbols.iter().any(|s| s.name == tag_name) {
                        // Create external component symbol (Miller's approach)
                        let component_symbol = self.base.create_symbol(
                            &node,
                            tag_name.to_string(),
                            SymbolKind::Class,
                            SymbolOptions {
                                signature: Some(format!("external component {}", tag_name)),
                                visibility: Some(Visibility::Public),
                                parent_id: None,
                                metadata: Some({
                                    let mut metadata = HashMap::new();
                                    metadata.insert("type".to_string(), serde_json::Value::String("external-component".to_string()));
                                    metadata.insert("source".to_string(), serde_json::Value::String("inferred".to_string()));
                                    metadata
                                }),
                                doc_comment: None,
                            },
                        );
                        symbols.push(component_symbol);
                    }
                }
            }
        }
    }

    fn extract_binding_attributes_from_element(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        let element_text = self.base.get_node_text(&node);

        // Extract @bind-Value attributes using regex patterns (Miller approach)
        if let Ok(value_regex) = regex::Regex::new(r#"@bind-Value="([^"]+)""#) {
            for captures in value_regex.captures_iter(&element_text) {
                if let Some(value_match) = captures.get(1) {
                    let binding_value = value_match.as_str();
                    let binding_name = format!("{}_binding",
                        binding_value.replace("Model.", "").replace(".", "_").to_lowercase());
                    let binding_signature = format!("@bind-Value=\"{}\"", binding_value);

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
                                metadata.insert("property".to_string(), serde_json::Value::String(binding_value.to_string()));
                                metadata
                            }),
                            doc_comment: None,
                        },
                    );
                    symbols.push(binding_symbol);
                }
            }
        }

        // Extract @bind-Value:event attributes
        if let Ok(event_regex) = regex::Regex::new(r#"@bind-Value:event="([^"]+)""#) {
            for captures in event_regex.captures_iter(&element_text) {
                if let Some(event_match) = captures.get(1) {
                    let event_value = event_match.as_str();
                    let event_name = format!("{}_event_binding", event_value.to_lowercase());
                    let event_signature = format!("@bind-Value:event=\"{}\"", event_value);

                    let event_symbol = self.base.create_symbol(
                        &node,
                        event_name,
                        SymbolKind::Variable,
                        SymbolOptions {
                            signature: Some(event_signature.clone()),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: Some({
                                let mut metadata = HashMap::new();
                                metadata.insert("type".to_string(), serde_json::Value::String("event-binding".to_string()));
                                metadata.insert("event".to_string(), serde_json::Value::String(event_value.to_string()));
                                metadata
                            }),
                            doc_comment: None,
                        },
                    );
                    symbols.push(event_symbol);
                }
            }
        }
    }

    fn extract_html_element(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let tag_name = self.extract_html_tag_name(node);
        let attributes = self.extract_html_attributes(node);

        let mut signature = format!("<{}>", tag_name);
        if !attributes.is_empty() {
            signature = format!("<{} {}>", tag_name, attributes.join(" "));
        }

        Some(self.base.create_symbol(
            &node,
            tag_name.clone(),
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("html-element".to_string()));
                    metadata.insert("tagName".to_string(), serde_json::Value::String(tag_name));
                    metadata.insert("attributes".to_string(), serde_json::Value::String(attributes.join(", ")));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_html_tag_name(&self, node: Node) -> String {
        if let Some(tag_node) = self.find_child_by_types(node, &["tag_name", "identifier"]) {
            return self.base.get_node_text(&tag_node);
        }

        // Fallback: extract from node text
        let node_text = self.base.get_node_text(&node);
        if let Some(captures) = regex::Regex::new(r"^<(\w+)").unwrap().captures(&node_text) {
            captures[1].to_string()
        } else {
            "div".to_string()
        }
    }

    fn extract_html_tag_name_from_node(&self, node: Node) -> Option<String> {
        if let Some(tag_node) = self.find_child_by_types(node, &["tag_name", "identifier"]) {
            return Some(self.base.get_node_text(&tag_node));
        }

        // Fallback: extract from node text
        let node_text = self.base.get_node_text(&node);
        if let Some(captures) = regex::Regex::new(r"^<(\w+)").unwrap().captures(&node_text) {
            Some(captures[1].to_string())
        } else {
            None
        }
    }

    fn extract_html_attributes(&self, node: Node) -> Vec<String> {
        let mut attributes = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "attribute" {
                attributes.push(self.base.get_node_text(&child));
            }
        }
        attributes
    }

    fn extract_component(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let component_name = self.extract_component_name(node);
        let parameters = self.extract_component_parameters(node);

        let mut signature = format!("<{} />", component_name);
        if !parameters.is_empty() {
            signature = format!("<{} {} />", component_name, parameters.join(" "));
        }

        Some(self.base.create_symbol(
            &node,
            component_name.clone(),
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some({
                    let mut metadata = HashMap::new();
                    metadata.insert("type".to_string(), serde_json::Value::String("razor-component".to_string()));
                    metadata.insert("componentName".to_string(), serde_json::Value::String(component_name));
                    metadata.insert("parameters".to_string(), serde_json::Value::String(parameters.join(", ")));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_component_name(&self, node: Node) -> String {
        if let Some(name_node) = self.find_child_by_types(node, &["identifier", "tag_name"]) {
            self.base.get_node_text(&name_node)
        } else {
            "UnknownComponent".to_string()
        }
    }

    fn extract_component_parameters(&self, node: Node) -> Vec<String> {
        let mut parameters = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "attribute" | "parameter") {
                parameters.push(self.base.get_node_text(&child));
            }
        }
        parameters
    }

    fn extract_csharp_symbols(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_csharp_node(child, symbols, parent_id);
        }
    }

    fn visit_csharp_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
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

    fn extract_using(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let namespace_name = self.extract_namespace_name(node);

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
                    metadata.insert("type".to_string(), serde_json::Value::String("using-directive".to_string()));
                    metadata.insert("namespace".to_string(), serde_json::Value::String(namespace_name));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_namespace(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name = if let Some(name_node) = self.find_child_by_types(node, &["qualified_name", "identifier"]) {
            self.base.get_node_text(&name_node)
        } else {
            "UnknownNamespace".to_string()
        };

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
                    metadata.insert("type".to_string(), serde_json::Value::String("namespace".to_string()));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_class(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
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
                    metadata.insert("type".to_string(), serde_json::Value::String("class".to_string()));
                    metadata.insert("modifiers".to_string(), serde_json::Value::String(modifiers.join(", ")));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_method(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
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
        signature_parts.push(format!("{}{}{}", interface_qualification, name, parameters.as_ref().map(|s| s.clone()).unwrap_or_else(|| "()".to_string())));

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
                    metadata.insert("type".to_string(), serde_json::Value::String("method".to_string()));
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

    fn extract_property(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let mut name = "unknownProperty".to_string();

        // Find property name - should be after type but before accessors
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        for (i, child) in children.iter().enumerate() {
            if child.kind() == "identifier" {
                // Check if this identifier comes after a type node
                let has_preceding_type = children.iter().take(i).any(|c| {
                    matches!(c.kind(), "predefined_type" | "nullable_type" | "array_type" | "generic_name" | "identifier")
                        && children.iter().take(i).any(|prev| prev.kind() == "modifier")
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
            let accessors: Vec<_> = accessor_list.children(&mut cursor)
                .filter(|c| matches!(c.kind(), "get_accessor_declaration" | "set_accessor_declaration"))
                .map(|c| if c.kind() == "get_accessor_declaration" { "get" } else { "set" })
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
                    metadata.insert("type".to_string(), serde_json::Value::String("property".to_string()));
                    metadata.insert("modifiers".to_string(), serde_json::Value::String(modifiers.join(", ")));
                    if let Some(prop_type) = property_type {
                        metadata.insert("propertyType".to_string(), serde_json::Value::String(prop_type));
                    }
                    metadata.insert("attributes".to_string(), serde_json::Value::String(attributes.join(", ")));
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    // Stub implementations for remaining methods
    fn extract_field(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
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

    fn extract_local_function(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
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
        signature_parts.push(format!("{}{}", name, parameters.as_ref().map(|s| s.clone()).unwrap_or_else(|| "()".to_string())));

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

    fn extract_local_variable(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
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

    fn extract_variable_declaration(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        // Extract variable name and type from variable declaration
        let mut variable_name = "unknownVariable".to_string();
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
            variable_name = name.clone();

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

    fn extract_assignment(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
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

    fn extract_invocation(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let invocation_text = self.base.get_node_text(&node);

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

    fn extract_element_access(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
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

    fn extract_html_attribute(&mut self, node: Node, parent_id: Option<&str>, symbols: &mut Vec<Symbol>) -> Option<Symbol> {
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

    fn extract_razor_attribute(&mut self, _node: Node, _parent_id: Option<&str>) -> Option<Symbol> {
        None // TODO: Implement when needed
    }

    // Helper methods
    fn find_child_by_type<'a>(&self, node: Node<'a>, child_type: &str) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == child_type {
                return Some(child);
            }
        }
        None
    }

    fn find_child_by_types<'a>(&self, node: Node<'a>, child_types: &[&str]) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child_types.contains(&child.kind()) {
                return Some(child);
            }
        }
        None
    }

    fn extract_modifiers(&self, node: Node) -> Vec<String> {
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

    fn extract_method_parameters(&self, node: Node) -> Option<String> {
        if let Some(param_list) = self.find_child_by_type(node, "parameter_list") {
            Some(self.base.get_node_text(&param_list))
        } else {
            None
        }
    }

    fn extract_return_type(&self, node: Node) -> Option<String> {
        let type_kinds = [
            "predefined_type", "identifier", "generic_name", "qualified_name",
            "nullable_type", "array_type"
        ];

        if let Some(return_type) = self.find_child_by_types(node, &type_kinds) {
            Some(self.base.get_node_text(&return_type))
        } else {
            None
        }
    }

    fn extract_property_type(&self, node: Node) -> Option<String> {
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

    fn extract_attributes(&self, node: Node) -> Vec<String> {
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

    fn determine_visibility(&self, modifiers: &[String]) -> Visibility {
        if modifiers.iter().any(|m| m == "private") {
            Visibility::Private
        } else if modifiers.iter().any(|m| m == "protected") {
            Visibility::Protected
        } else {
            Visibility::Public
        }
    }

    fn extract_namespace_name(&self, node: Node) -> String {
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
        let element_text = self.base.get_node_text(&node);

        // Look for component tag names (uppercase elements indicate components)
        if let Some(name_node) = self.find_child_by_type(node, "identifier") {
            let component_name = self.base.get_node_text(&name_node);

            // Find the using component (from symbols) - prefer the main page/component
            let from_symbol = symbols.iter().find(|s| s.kind == SymbolKind::Class)
                .or_else(|| symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("@page"))))
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
                        .or_else(|| symbols.iter().find(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("@page")))) {

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
            if let Some(property_type) = metadata.get("propertyType").and_then(|v| v.as_str()) {
                    inferred_type = property_type.to_string();
                } else if let Some(field_type) = metadata.get("fieldType").and_then(|v| v.as_str()) {
                    inferred_type = field_type.to_string();
                } else if let Some(variable_type) = metadata.get("variableType").and_then(|v| v.as_str()) {
                    inferred_type = variable_type.to_string();
                } else if let Some(return_type) = metadata.get("returnType").and_then(|v| v.as_str()) {
                    inferred_type = return_type.to_string();
                } else if let Some(signature) = &symbol.signature {
                    // Try to extract type from signature
                    let type_patterns = [
                        regex::Regex::new(r"(?:\[\w+.*?\]\s+)?(?:public|private|protected|internal|static)\s+(\w+(?:<[^>]+>)?(?:\?|\[\])?)\s+\w+").unwrap(),
                        regex::Regex::new(r"(?:public|private|protected|internal|static|async)\s+(\w+(?:<[^>]+>)?)\s+\w+\s*\(").unwrap(),
                        regex::Regex::new(r"(\w+(?:<[^>]+>)?(?:\?|\[\])?)\s+\w+\s*=").unwrap(),
                        regex::Regex::new(&format!(r"\s+(\w+(?:<[^>]+>)?(?:\?|\[\])?)\s+{}\b", regex::escape(&symbol.name))).unwrap(),
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
            if metadata.get("isDataBinding")
                .and_then(|v| v.as_str())
                .map_or(false, |v| v == "true") {
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