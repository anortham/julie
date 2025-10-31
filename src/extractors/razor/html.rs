/// HTML element and Razor component extraction
use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

impl super::RazorExtractor {
    /// Extract HTML elements (<div>, <span>, etc.)
    pub(super) fn extract_html_element(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let tag_name = self.extract_html_tag_name(node);
        let attributes = self.extract_html_attributes(node);

        let mut signature = format!("<{}>", tag_name);
        if !attributes.is_empty() {
            signature = format!("<{} {}>", tag_name, attributes.join(" "));
        }

        // Extract HTML/Razor doc comment
        let doc_comment = self.base.find_doc_comment(&node);

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
                doc_comment,
            },
        ))
    }

    /// Extract HTML tag name from element node
    pub(super) fn extract_html_tag_name(&self, node: Node) -> String {
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

    /// Extract HTML tag name from element node (optional return)
    #[allow(dead_code)]
    pub(super) fn extract_html_tag_name_from_node(&self, node: Node) -> Option<String> {
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

    /// Extract attributes from HTML element
    pub(super) fn extract_html_attributes(&self, node: Node) -> Vec<String> {
        let mut attributes = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "attribute" {
                attributes.push(self.base.get_node_text(&child));
            }
        }
        attributes
    }

    /// Extract HTML attributes with Razor bindings from element
    pub(super) fn extract_binding_attributes_from_element(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
        parent_id: Option<&str>,
    ) {
        let element_text = self.base.get_node_text(&node);

        // Extract @bind-Value attributes using regex patterns (standard approach)
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

    /// Extract Razor components (<Component />, <MyCustomComponent />)
    pub(super) fn extract_component(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let component_name = self.extract_component_name(node);
        let parameters = self.extract_component_parameters(node);

        let mut signature = format!("<{} />", component_name);
        if !parameters.is_empty() {
            signature = format!("<{} {} />", component_name, parameters.join(" "));
        }

        // Extract HTML/Razor doc comment
        let doc_comment = self.base.find_doc_comment(&node);

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
                doc_comment,
            },
        ))
    }

    /// Extract component name from razor_component node
    pub(super) fn extract_component_name(&self, node: Node) -> String {
        if let Some(name_node) = self.find_child_by_types(node, &["identifier", "tag_name"]) {
            self.base.get_node_text(&name_node)
        } else {
            "UnknownComponent".to_string()
        }
    }

    /// Extract component parameters from razor_component node
    pub(super) fn extract_component_parameters(&self, node: Node) -> Vec<String> {
        let mut parameters = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "attribute" | "parameter") {
                parameters.push(self.base.get_node_text(&child));
            }
        }
        parameters
    }

    /// Create external component symbols for uppercase tags (standard approach)
    pub(super) fn create_external_component_symbols_if_needed(
        &mut self,
        node: Node,
        symbols: &mut Vec<Symbol>,
    ) {
        let node_text = self.base.get_node_text(&node);

        // Use regex to find all component tags within the element (standard approach)
        if let Ok(component_regex) = regex::Regex::new(r"<([A-Z][A-Za-z0-9]*)\b") {
            for captures in component_regex.captures_iter(&node_text) {
                if let Some(tag_match) = captures.get(1) {
                    let tag_name = tag_match.as_str();

                    // Check if symbol already exists
                    if !symbols.iter().any(|s| s.name == tag_name) {
                        // Create external component symbol (standard approach)
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

    /// Extract HTML attributes with detailed parsing
    pub(super) fn extract_html_attribute(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
        symbols: &mut Vec<Symbol>,
    ) -> Option<Symbol> {
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
            if let Some(captures) = regex::Regex::new(r"([^=]+)=(.*)")
                .unwrap()
                .captures(&attribute_text)
            {
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
                    let binding_name = format!(
                        "{}_binding",
                        value
                            .replace("\"", "")
                            .replace("Model.", "")
                            .replace(".", "_")
                            .to_lowercase()
                    );
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
                                    serde_json::Value::String(value.clone()),
                                );
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
                    let event_binding_name =
                        format!("{}_event_binding", value.replace("\"", "").to_lowercase());
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
                                metadata.insert(
                                    "type".to_string(),
                                    serde_json::Value::String("event-binding".to_string()),
                                );
                                metadata.insert(
                                    "event".to_string(),
                                    serde_json::Value::String(value.clone()),
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
                        metadata.insert(
                            "type".to_string(),
                            serde_json::Value::String("html-attribute".to_string()),
                        );
                        metadata.insert(
                            "attributeName".to_string(),
                            serde_json::Value::String(name.clone()),
                        );
                        if let Some(value) = attr_value {
                            metadata.insert(
                                "attributeValue".to_string(),
                                serde_json::Value::String(value),
                            );
                        }
                        if name.starts_with("@bind") {
                            metadata.insert(
                                "isDataBinding".to_string(),
                                serde_json::Value::String("true".to_string()),
                            );
                        }
                        if name.starts_with("@on") {
                            metadata.insert(
                                "isEventBinding".to_string(),
                                serde_json::Value::Bool(true),
                            );
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

    /// Extract Razor attribute (stub - to be implemented)
    pub(super) fn extract_razor_attribute(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let name = if let Some(name_node) = self.find_child_by_type(node, "identifier") {
            self.base.get_node_text(&name_node)
        } else {
            let raw = self.base.get_node_text(&node);
            raw.split('=')
                .next()
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };

        if name.is_empty() {
            return None;
        }

        let value = if let Some(value_node) =
            self.find_child_by_types(node, &["attribute_value", "string_literal"])
        {
            Some(self.base.get_node_text(&value_node))
        } else {
            None
        };

        let signature = if let Some(value) = &value {
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
                    metadata.insert(
                        "type".to_string(),
                        serde_json::Value::String("razor-attribute".to_string()),
                    );
                    metadata.insert(
                        "attributeName".to_string(),
                        serde_json::Value::String(name.clone()),
                    );
                    if let Some(value) = value {
                        metadata.insert(
                            "attributeValue".to_string(),
                            serde_json::Value::String(value),
                        );
                    }
                    if name.starts_with("@bind") {
                        metadata.insert("isDataBinding".to_string(), serde_json::Value::Bool(true));
                    }
                    if name.starts_with("@on") {
                        metadata
                            .insert("isEventBinding".to_string(), serde_json::Value::Bool(true));
                    }
                    metadata
                }),
                doc_comment: None,
            },
        ))
    }
}
