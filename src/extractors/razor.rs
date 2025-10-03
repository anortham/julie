use crate::extractors::base::{
    BaseExtractor, Relationship, Symbol, SymbolKind, SymbolOptions, Visibility,
};
use tree_sitter::{Node, Tree};

// Include stub implementations to reduce file size
include!("razor_stubs.rs");
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
        // Handle ERROR nodes by falling back to text-based extraction
        if node.kind() == "ERROR" {
            self.extract_from_text_content(node, symbols, parent_id.as_deref());
            return;
        }

        if !self.is_valid_node(&node) {
            return;
        }

        let mut symbol = None;
        let node_type = node.kind();

        match node_type {
            "razor_directive"
            | "razor_inject_directive"
            | "razor_using_directive"
            | "razor_page_directive"
            | "razor_namespace_directive"
            | "razor_model_directive"
            | "razor_attribute_directive"
            | "razor_inherits_directive"
            | "razor_implements_directive"
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
                self.extract_csharp_symbols(
                    node,
                    symbols,
                    symbol
                        .as_ref()
                        .map(|s| s.id.as_str())
                        .or(parent_id.as_deref()),
                );
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

    fn extract_from_text_content(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        let content = self.base.get_node_text(&node);

        // Extract Razor directives from text
        use regex::Regex;

        // Look for @inherits directive
        let inherits_regex = Regex::new(r"@inherits\s+(\S+)").unwrap();
        if let Some(captures) = inherits_regex.captures(&content) {
            if let Some(base_class) = captures.get(1) {
                let symbol = self.base.create_symbol(
                    &node,
                    format!("inherits {}", base_class.as_str()),
                    SymbolKind::Import,
                    SymbolOptions {
                        signature: Some(format!("@inherits {}", base_class.as_str())),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: None,
                    },
                );
                symbols.push(symbol);
            }
        }

        // Look for HTML elements/components
        let component_regex = Regex::new(r"<(\w+)").unwrap();
        for captures in component_regex.captures_iter(&content) {
            if let Some(tag_name) = captures.get(1) {
                let tag = tag_name.as_str();
                // Only extract custom components (starting with uppercase)
                if tag.chars().next().unwrap_or('a').is_uppercase() {
                    let symbol = self.base.create_symbol(
                        &node,
                        tag.to_string(),
                        SymbolKind::Class,
                        SymbolOptions {
                            signature: Some(format!("<{}>", tag)),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment: None,
                        },
                    );
                    symbols.push(symbol);
                }
            }
        }

        // Look for @rendermode directives
        let rendermode_regex = Regex::new(r#"@rendermode="([^"]+)""#).unwrap();
        for captures in rendermode_regex.captures_iter(&content) {
            if let Some(mode) = captures.get(1) {
                let symbol = self.base.create_symbol(
                    &node,
                    format!("rendermode {}", mode.as_str()),
                    SymbolKind::Property,
                    SymbolOptions {
                        signature: Some(format!("@rendermode=\"{}\"", mode.as_str())),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: None,
                    },
                );
                symbols.push(symbol);
            }
        }
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
            "using" => directive_value
                .clone()
                .unwrap_or_else(|| format!("@{}", directive_name)),
            "inject" => {
                // Extract property name from "@inject IService PropertyName"
                if let Some(value) = &directive_value {
                    let parts: Vec<&str> = value.split_whitespace().collect();
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
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-directive".to_string()),
                    );
                    metadata.insert(
                        "directiveName".to_string(),
                        serde_json::Value::String(directive_name.clone()),
                    );
                    if let Some(value) = directive_value {
                        metadata.insert(
                            "directiveValue".to_string(),
                            serde_json::Value::String(value),
                        );
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
                } else if let Some(captures) = regex::Regex::new(r"@(\w+)").unwrap().captures(&text)
                {
                    captures[1].to_string()
                } else {
                    "unknown".to_string()
                }
            }
        }
    }

    fn extract_directive_value(&self, node: Node) -> Option<String> {
        match node.kind() {
            "razor_page_directive" => self
                .find_child_by_type(node, "string_literal")
                .map(|n| self.base.get_node_text(&n)),
            "razor_model_directive" | "razor_inherits_directive" | "razor_implements_directive" => {
                self.find_child_by_type(node, "identifier")
                    .map(|n| self.base.get_node_text(&n))
            }
            "razor_using_directive" | "razor_namespace_directive" => self
                .find_child_by_types(node, &["qualified_name", "identifier"])
                .map(|n| self.base.get_node_text(&n)),
            "razor_inject_directive" => self
                .find_child_by_type(node, "variable_declaration")
                .map(|n| self.base.get_node_text(&n)),
            "razor_attribute_directive" => self
                .find_child_by_type(node, "attribute_list")
                .map(|n| self.base.get_node_text(&n)),
            "razor_addtaghelper_directive" => {
                let text = self.base.get_node_text(&node);
                regex::Regex::new(r"@addTagHelper\s+(.+)")
                    .unwrap()
                    .captures(&text)
                    .map(|captures| captures[1].trim().to_string())
            }
            _ => {
                let text = self.base.get_node_text(&node);
                if text.contains("@addTagHelper") {
                    regex::Regex::new(r"@addTagHelper\s+(.+)")
                        .unwrap()
                        .captures(&text)
                        .map(|captures| captures[1].trim().to_string())
                } else {
                    regex::Regex::new(r"@\w+\s+(.*)")
                        .unwrap()
                        .captures(&text)
                        .map(|captures| captures[1].trim().to_string())
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
            regex::Regex::new(&format!(r"@{}\s+(\S+)", directive_type))
                .unwrap()
                .captures(&text)
                .map(|captures| captures[1].to_string())
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
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-token-directive".to_string()),
                    );
                    metadata.insert(
                        "directiveType".to_string(),
                        serde_json::Value::String(directive_type.clone()),
                    );
                    if let Some(value) = directive_value {
                        metadata.insert(
                            "directiveValue".to_string(),
                            serde_json::Value::String(value),
                        );
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
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-section".to_string()),
                    );
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
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-code-block".to_string()),
                    );
                    metadata.insert(
                        "blockType".to_string(),
                        serde_json::Value::String(block_type.clone()),
                    );
                    metadata.insert(
                        "content".to_string(),
                        serde_json::Value::String(content[..content.len().min(200)].to_string()),
                    );
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
        let variable_name = self
            .extract_variable_from_expression(&expression)
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
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-expression".to_string()),
                    );
                    metadata.insert(
                        "expression".to_string(),
                        serde_json::Value::String(expression.clone()),
                    );
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }

    fn extract_variable_from_expression(&self, expression: &str) -> Option<String> {
        regex::Regex::new(r"(\w+)")
            .unwrap()
            .captures(expression)
            .map(|captures| captures[1].to_string())
    }

    fn create_external_component_symbols_if_needed(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
    ) {
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
                                    metadata.insert(
                                        "type".to_string(),
                                        serde_json::Value::String("external-component".to_string()),
                                    );
                                    metadata.insert(
                                        "source".to_string(),
                                        serde_json::Value::String("inferred".to_string()),
                                    );
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

    fn extract_binding_attributes_from_element(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        let element_text = self.base.get_node_text(&node);

        // Extract @bind-Value attributes using regex patterns (Miller approach)
        if let Ok(value_regex) = regex::Regex::new(r#"@bind-Value="([^"]+)""#) {
            for captures in value_regex.captures_iter(&element_text) {
                if let Some(value_match) = captures.get(1) {
                    let binding_value = value_match.as_str();
                    let binding_name = format!(
                        "{}_binding",
                        binding_value
                            .replace("Model.", "")
                            .replace(".", "_")
                            .to_lowercase()
                    );
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
                                metadata.insert(
                                    "type".to_string(),
                                    serde_json::Value::String("data-binding".to_string()),
                                );
                                metadata.insert(
                                    "bindingType".to_string(),
                                    serde_json::Value::String("two-way".to_string()),
                                );
                                metadata.insert(
                                    "property".to_string(),
                                    serde_json::Value::String(binding_value.to_string()),
                                );
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
                                metadata.insert(
                                    "type".to_string(),
                                    serde_json::Value::String("event-binding".to_string()),
                                );
                                metadata.insert(
                                    "event".to_string(),
                                    serde_json::Value::String(event_value.to_string()),
                                );
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
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("html-element".to_string()),
                    );
                    metadata.insert("tagName".to_string(), serde_json::Value::String(tag_name));
                    metadata.insert(
                        "attributes".to_string(),
                        serde_json::Value::String(attributes.join(", ")),
                    );
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

    #[allow(dead_code)]
    fn extract_html_tag_name_from_node(&self, node: Node) -> Option<String> {
        if let Some(tag_node) = self.find_child_by_types(node, &["tag_name", "identifier"]) {
            return Some(self.base.get_node_text(&tag_node));
        }

        // Fallback: extract from node text
        let node_text = self.base.get_node_text(&node);
        regex::Regex::new(r"^<(\w+)")
            .unwrap()
            .captures(&node_text)
            .map(|captures| captures[1].to_string())
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
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-component".to_string()),
                    );
                    metadata.insert(
                        "componentName".to_string(),
                        serde_json::Value::String(component_name),
                    );
                    metadata.insert(
                        "parameters".to_string(),
                        serde_json::Value::String(parameters.join(", ")),
                    );
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

    fn extract_csharp_symbols(
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

    fn visit_csharp_node(
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
                doc_comment: None,
            },
        ))
    }

    fn extract_namespace(&mut self, node: Node, parent_id: Option<&str>) -> Option<Symbol> {
        let name = if let Some(name_node) =
            self.find_child_by_types(node, &["qualified_name", "identifier"])
        {
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
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("namespace".to_string()),
                    );
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
        signature_parts.push(format!(
            "{}{}{}",
            interface_qualification,
            name,
            parameters.clone().unwrap_or_else(|| "()".to_string())
        ));

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
                doc_comment: None,
            },
        ))
    }
}
