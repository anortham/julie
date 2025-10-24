/// Relationship extraction (component usage, bindings, method calls)
use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::Node;

impl super::RazorExtractor {
    /// Extract relationships between symbols
    pub fn extract_relationships(
        &mut self,
        tree: &tree_sitter::Tree,
        symbols: &[Symbol],
    ) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    /// Visit nodes and extract relationships
    fn visit_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "razor_component" => self.extract_component_relationships(node, symbols, relationships),
            "using_directive" => self.extract_using_relationships(node, symbols, relationships),
            "html_element" | "element" => {
                self.extract_element_relationships(node, symbols, relationships)
            }
            "identifier" => {
                self.extract_identifier_component_relationships(node, symbols, relationships)
            }
            "invocation_expression" => {
                self.extract_invocation_relationships(node, symbols, relationships)
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_relationships(child, symbols, relationships);
        }
    }

    /// Extract relationships between Razor components
    fn extract_component_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        // Extract relationships between Razor components
        let _element_text = self.base.get_node_text(&node);

        // Look for component tag names (uppercase elements indicate components)
        if let Some(name_node) = self.find_child_by_type(node, "identifier") {
            let component_name = self.base.get_node_text(&name_node);

            // Find the using component (from symbols) - prefer the main page/component
            let from_symbol = symbols
                .iter()
                .find(|s| s.kind == SymbolKind::Class)
                .or_else(|| {
                    symbols.iter().find(|s| {
                        s.signature
                            .as_ref()
                            .is_some_and(|sig| sig.contains("@page"))
                    })
                })
                .or_else(|| symbols.iter().find(|s| s.kind == SymbolKind::Module));

            if let Some(from_sym) = from_symbol {
                // Create synthetic relationship to used component
                let to_symbol_id = format!("component-{}", component_name);

                relationships.push(self.base.create_relationship(
                    from_sym.id.clone(),
                    to_symbol_id,
                    RelationshipKind::Uses,
                    &node,
                    Some(1.0),
                    Some({
                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "component".to_string(),
                            serde_json::Value::String(component_name.clone()),
                        );
                        metadata.insert(
                            "type".to_string(),
                            serde_json::Value::String("component-usage".to_string()),
                        );
                        metadata
                    }),
                ));
            }
        }
    }

    /// Extract using directive relationships
    fn extract_using_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        // Extract using directive relationships
        if let Some(qualified_name) = self.find_child_by_type(node, "qualified_name") {
            let namespace_name = self.base.get_node_text(&qualified_name);

            // Find any symbol that could be using this namespace
            if let Some(from_symbol) = symbols.iter().find(|s| s.kind == SymbolKind::Class) {
                relationships.push(self.base.create_relationship(
                    from_symbol.id.clone(),
                    format!("using-{}", namespace_name), // Create synthetic ID for namespaces
                    RelationshipKind::Uses,
                    &node,
                    Some(0.8),
                    Some({
                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "namespace".to_string(),
                            serde_json::Value::String(namespace_name),
                        );
                        metadata.insert(
                            "type".to_string(),
                            serde_json::Value::String("using-directive".to_string()),
                        );
                        metadata
                    }),
                ));
            }
        }
    }

    /// Extract relationships from HTML elements with bindings
    fn extract_element_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        // Extract relationships from HTML elements that might bind to properties
        let element_text = self.base.get_node_text(&node);

        // Check for component usage using regex to find all components in the element
        if let Ok(component_regex) = regex::Regex::new(r"<([A-Z][A-Za-z0-9]*)\b") {
            for captures in component_regex.captures_iter(&element_text) {
                if let Some(tag_match) = captures.get(1) {
                    let tag_name = tag_match.as_str();

                    if let Some(from_symbol) = symbols
                        .iter()
                        .find(|s| s.kind == SymbolKind::Class)
                        .or_else(|| {
                            symbols.iter().find(|s| {
                                s.signature
                                    .as_ref()
                                    .is_some_and(|sig| sig.contains("@page"))
                            })
                        })
                    {
                        // Find the component symbol (should exist now due to symbol extraction)
                        if let Some(component_symbol) = symbols.iter().find(|s| s.name == tag_name)
                        {
                            relationships.push(self.base.create_relationship(
                                from_symbol.id.clone(),
                                component_symbol.id.clone(),
                                RelationshipKind::Uses,
                                &node,
                                Some(1.0),
                                Some({
                                    let mut metadata = HashMap::new();
                                    metadata.insert(
                                        "component".to_string(),
                                        serde_json::Value::String(tag_name.to_string()),
                                    );
                                    metadata.insert(
                                        "type".to_string(),
                                        serde_json::Value::String("component-usage".to_string()),
                                    );
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
                if let Some(captures) = regex::Regex::new(r"@bind-(\w+)")
                    .unwrap()
                    .captures(&element_text)
                {
                    if let Some(property_match) = captures.get(1) {
                        let property_name = property_match.as_str().to_string();

                        relationships.push(self.base.create_relationship(
                            from_symbol.id.clone(),
                            format!("property-{}", property_name), // Create synthetic ID for bound properties
                            RelationshipKind::Uses,
                            &node,
                            Some(0.9),
                            Some({
                                let mut metadata = HashMap::new();
                                metadata.insert(
                                    "property".to_string(),
                                    serde_json::Value::String(property_name),
                                );
                                metadata.insert(
                                    "type".to_string(),
                                    serde_json::Value::String("data-binding".to_string()),
                                );
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
                if let Some(captures) = regex::Regex::new(r"@on(\w+)")
                    .unwrap()
                    .captures(&element_text)
                {
                    if let Some(event_match) = captures.get(1) {
                        let event_name = event_match.as_str().to_string();

                        relationships.push(self.base.create_relationship(
                            from_symbol.id.clone(),
                            format!("event-{}", event_name), // Create synthetic ID for events
                            RelationshipKind::Uses,
                            &node,
                            Some(0.9),
                            Some({
                                let mut metadata = HashMap::new();
                                metadata.insert(
                                    "event".to_string(),
                                    serde_json::Value::String(event_name),
                                );
                                metadata.insert(
                                    "type".to_string(),
                                    serde_json::Value::String("event-binding".to_string()),
                                );
                                metadata
                            }),
                        ));
                    }
                }
            }
        }
    }

    /// Extract identifier component relationships (stub)
    fn extract_identifier_component_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let identifier = self.base.get_node_text(&node);
        if identifier.is_empty() {
            return;
        }

        // Only consider potential component identifiers (PascalCase)
        if !identifier
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
        {
            return;
        }

        let component_symbol = symbols
            .iter()
            .find(|symbol| is_component_symbol(symbol) && symbol.name == identifier);

        let Some(component_symbol) = component_symbol else {
            return;
        };

        let Some(caller_symbol) = self.resolve_calling_symbol(node, symbols) else {
            return;
        };

        if caller_symbol.id == component_symbol.id {
            return;
        }

        // Avoid duplicate entries
        if relationships.iter().any(|rel| {
            rel.kind == RelationshipKind::Uses
                && rel.from_symbol_id == caller_symbol.id
                && rel.to_symbol_id == component_symbol.id
        }) {
            return;
        }

        relationships.push(self.base.create_relationship(
            caller_symbol.id.clone(),
            component_symbol.id.clone(),
            RelationshipKind::Uses,
            &node,
            Some(0.85),
            Some({
                let mut metadata = HashMap::new();
                metadata.insert(
                    "type".to_string(),
                    serde_json::Value::String("component-identifier".to_string()),
                );
                metadata.insert(
                    "component".to_string(),
                    serde_json::Value::String(identifier),
                );
                metadata
            }),
        ));
    }

    /// Extract invocation relationships (stub)
    fn extract_invocation_relationships(
        &self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let method_node = self.find_child_by_types(
            node,
            &["identifier", "member_access_expression", "qualified_name"],
        );
        let Some(method_node) = method_node else {
            return;
        };

        let method_name = self.base.get_node_text(&method_node);
        if method_name.is_empty() {
            return;
        }

        let Some(caller_symbol) = self.resolve_calling_symbol(node, symbols) else {
            return;
        };

        let invocation_symbol = self.find_invocation_symbol(node, symbols, &method_name);

        let callee_symbol = symbols.iter().find(|symbol| {
            !is_invocation_symbol(symbol)
                && matches!(
                    symbol.kind,
                    SymbolKind::Function
                        | SymbolKind::Method
                        | SymbolKind::Class
                        | SymbolKind::Module
                )
                && symbol.name == method_name
        });

        let component_target = if method_name.contains("Component.InvokeAsync") {
            self.find_component_target_for_invocation(node, symbols)
        } else {
            None
        };

        let target_id = if let Some(component_symbol) = component_target {
            component_symbol.id.clone()
        } else if let Some(target) = callee_symbol {
            target.id.clone()
        } else if let Some(invocation) = invocation_symbol {
            invocation.id.clone()
        } else {
            format!("method:{}", method_name)
        };

        // Avoid duplicate call relationships
        if relationships.iter().any(|rel| {
            rel.kind == RelationshipKind::Calls
                && rel.from_symbol_id == caller_symbol.id
                && rel.to_symbol_id == target_id
        }) {
            return;
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "method".to_string(),
            serde_json::Value::String(method_name.clone()),
        );

        if let Some(component_symbol) = component_target {
            metadata.insert(
                "component".to_string(),
                serde_json::Value::String(component_symbol.name.clone()),
            );
        } else if let Some(invocation) = invocation_symbol {
            if let Some(invocation_meta) = invocation.metadata.as_ref() {
                if let Some(arguments) = invocation_meta
                    .get("arguments")
                    .and_then(|value| value.as_str())
                {
                    metadata.insert(
                        "arguments".to_string(),
                        serde_json::Value::String(arguments.to_string()),
                    );
                }
                if let Some(component_invocation) = invocation_meta
                    .get("isComponentInvocation")
                    .and_then(|value| value.as_bool())
                {
                    metadata.insert(
                        "isComponentInvocation".to_string(),
                        serde_json::Value::Bool(component_invocation),
                    );
                }
                if let Some(html_helper) = invocation_meta
                    .get("isHtmlHelper")
                    .and_then(|value| value.as_bool())
                {
                    metadata.insert(
                        "isHtmlHelper".to_string(),
                        serde_json::Value::Bool(html_helper),
                    );
                }
                if let Some(render_section) = invocation_meta
                    .get("isRenderSection")
                    .and_then(|value| value.as_bool())
                {
                    metadata.insert(
                        "isRenderSection".to_string(),
                        serde_json::Value::Bool(render_section),
                    );
                }
                if let Some(render_body) = invocation_meta
                    .get("isRenderBody")
                    .and_then(|value| value.as_bool())
                {
                    metadata.insert(
                        "isRenderBody".to_string(),
                        serde_json::Value::Bool(render_body),
                    );
                }
            }
        }

        relationships.push(self.base.create_relationship(
            caller_symbol.id.clone(),
            target_id,
            RelationshipKind::Calls,
            &node,
            Some(0.9),
            Some(metadata),
        ));
    }
}

fn symbol_type(symbol: &Symbol) -> Option<&str> {
    symbol
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("type"))
        .and_then(|value| value.as_str())
}

fn is_component_symbol(symbol: &Symbol) -> bool {
    matches!(
        symbol_type(symbol),
        Some("razor-component") | Some("external-component")
    )
}

fn is_invocation_symbol(symbol: &Symbol) -> bool {
    matches!(symbol_type(symbol), Some("method-invocation"))
}

impl super::RazorExtractor {
    fn resolve_calling_symbol<'a>(
        &self,
        node: Node<'a>,
        symbols: &'a [Symbol],
    ) -> Option<&'a Symbol> {
        let mut current = self.base.find_containing_symbol(&node, symbols)?;
        if is_invocation_symbol(current) {
            if let Some(parent_id) = &current.parent_id {
                if let Some(parent) = symbols.iter().find(|symbol| &symbol.id == parent_id) {
                    current = parent;
                }
            }
        }
        Some(current)
    }

    fn find_invocation_symbol<'a>(
        &self,
        node: Node<'a>,
        symbols: &'a [Symbol],
        method_name: &str,
    ) -> Option<&'a Symbol> {
        let position = node.start_position();
        symbols.iter().find(|symbol| {
            is_invocation_symbol(symbol)
                && symbol.name == method_name
                && symbol.start_line == (position.row + 1) as u32
                && symbol.start_column == position.column as u32
        })
    }

    fn find_component_target_for_invocation<'a>(
        &self,
        node: Node<'a>,
        symbols: &'a [Symbol],
    ) -> Option<&'a Symbol> {
        let component_name = self.extract_first_string_literal(node)?;
        symbols
            .iter()
            .find(|symbol| is_component_symbol(symbol) && symbol.name == component_name)
    }

    fn extract_first_string_literal(&self, node: Node) -> Option<String> {
        if node.kind() == "string_literal" {
            let text = self.base.get_node_text(&node);
            return Some(trim_quotes(&text).to_string());
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(value) = self.extract_first_string_literal(child) {
                return Some(value);
            }
        }

        None
    }
}

fn trim_quotes(value: &str) -> &str {
    value.trim_matches(|c| c == '"' || c == '\'')
}
