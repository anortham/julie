/// Helper functions for relationship extraction (identifier/invocation resolution, symbol lookup)
use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use std::collections::HashMap;
use tree_sitter::Node;

fn symbol_type(symbol: &Symbol) -> Option<&str> {
    symbol
        .metadata
        .as_ref()
        .and_then(|meta| meta.get("type"))
        .and_then(|value| value.as_str())
}

pub(super) fn is_component_symbol(symbol: &Symbol) -> bool {
    matches!(
        symbol_type(symbol),
        Some("razor-component") | Some("external-component") | Some("blazor-component")
    )
}

fn is_invocation_symbol(symbol: &Symbol) -> bool {
    matches!(symbol_type(symbol), Some("method-invocation"))
}

pub(super) fn trim_quotes(value: &str) -> &str {
    value.trim_matches(|c| c == '"' || c == '\'')
}

impl super::RazorExtractor {
    /// Extract identifier component relationships
    pub(super) fn extract_identifier_component_relationships(
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

    /// Extract invocation relationships
    pub(super) fn extract_invocation_relationships(
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

    pub(super) fn resolve_calling_symbol<'a>(
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

    pub(super) fn find_component_target_for_invocation<'a>(
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
