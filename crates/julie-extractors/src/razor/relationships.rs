/// Relationship extraction (component usage, bindings, method calls)
use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
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
        self.extract_using_line_relationships(tree.root_node(), symbols, &mut relationships);
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
        if let Some(namespace_name) = self
            .find_child_by_type(node, "qualified_name")
            .map(|qualified_name| self.base.get_node_text(&qualified_name))
            .or_else(|| {
                self.base
                    .get_node_text(&node)
                    .strip_prefix("@using")
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(ToString::to_string)
            })
        {
            // Find any symbol that could be using this namespace
            if let Some(from_symbol) = symbols
                .iter()
                .find(|s| s.kind == SymbolKind::Class)
                .or_else(|| symbols.iter().find(|s| s.name == "@page"))
                .or_else(|| symbols.first())
            {
                relationships.push(self.base.create_relationship(
                    from_symbol.id.clone(),
                    format!("namespace:{}", namespace_name),
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

    fn extract_using_line_relationships(
        &self,
        root: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let Some(from_symbol) = symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Class)
            .or_else(|| symbols.iter().find(|s| s.name == "@page"))
            .or_else(|| symbols.first())
        else {
            return;
        };

        for line in self.base.content.lines() {
            let namespace_name = line
                .trim()
                .strip_prefix("@using")
                .map(str::trim)
                .filter(|name| is_namespace_like(name));
            if let Some(namespace_name) = namespace_name {
                let to_symbol_id = format!("namespace:{}", namespace_name);
                if relationships.iter().any(|relationship| {
                    relationship.from_symbol_id == from_symbol.id
                        && relationship.to_symbol_id == to_symbol_id
                }) {
                    continue;
                }

                relationships.push(self.base.create_relationship(
                    from_symbol.id.clone(),
                    to_symbol_id,
                    RelationshipKind::Uses,
                    &root,
                    Some(0.8),
                    Some({
                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "namespace".to_string(),
                            serde_json::Value::String(namespace_name.to_string()),
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

                    // Find the component symbol first, then find a different "from" symbol
                    if let Some(component_symbol) = symbols.iter().find(|s| s.name == tag_name) {
                        // Find the page/module that USES this component (must not be the component itself)
                        let from_symbol = symbols
                            .iter()
                            .find(|s| {
                                s.signature
                                    .as_ref()
                                    .is_some_and(|sig| sig.contains("@page"))
                            })
                            .or_else(|| {
                                symbols.iter().find(|s| {
                                    s.kind == SymbolKind::Module && s.id != component_symbol.id
                                })
                            })
                            .or_else(|| {
                                symbols.iter().find(|s| {
                                    s.kind == SymbolKind::Class && s.id != component_symbol.id
                                })
                            });

                        if let Some(from_symbol) = from_symbol {
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
}

fn is_namespace_like(name: &str) -> bool {
    !name.is_empty()
        && name
            .split('.')
            .all(|segment| is_identifier_segment(segment.trim()))
}

fn is_identifier_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
