use crate::base::{
    LocalTargetResolution, Relationship, RelationshipKind, ScopedSymbolIndex, Symbol, SymbolKind,
    UnresolvedTarget,
};
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts inheritance, protocol conformance, and call relationships in Swift
impl SwiftExtractor {
    /// Extract relationships between Swift types and function calls
    /// Implementation of extractRelationships method
    pub fn extract_relationships(
        &mut self,
        tree: &tree_sitter::Tree,
        symbols: &[Symbol],
    ) -> Vec<Relationship> {
        let mut relationships = Vec::new();
        self.visit_node_for_relationships(tree.root_node(), symbols, &mut relationships);
        relationships
    }

    fn visit_node_for_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        match node.kind() {
            "class_declaration"
            | "struct_declaration"
            | "enum_declaration"
            | "extension_declaration" => {
                self.extract_inheritance_relationships(node, symbols, relationships);
            }
            "call_expression" => {
                self.extract_call_relationship(node, symbols, relationships);
            }
            _ => {}
        }

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node_for_relationships(child, symbols, relationships);
        }
    }

    /// Implementation of extractInheritanceRelationships method
    fn extract_inheritance_relationships(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        if let Some(type_symbol) = self.find_type_symbol(node, symbols) {
            let mut inheritance_entry_index = 0usize;
            let declaration_kind = self.declaration_kind_for_relationships(node);

            // Try type_inheritance_clause first
            if let Some(inheritance) = node
                .children(&mut node.walk())
                .find(|c| c.kind() == "type_inheritance_clause")
            {
                for child in inheritance.children(&mut inheritance.walk()) {
                    if let Some(base_type_name) = self.inheritance_type_name(child) {
                        let pending_kind = Self::pending_inheritance_kind(
                            &type_symbol,
                            declaration_kind,
                            inheritance_entry_index,
                        );
                        self.add_inheritance_relationship(
                            &type_symbol,
                            &base_type_name,
                            pending_kind,
                            symbols,
                            relationships,
                            node,
                        );
                        inheritance_entry_index += 1;
                    }
                }
            }

            // Also handle direct inheritance_specifier nodes
            for spec in node
                .children(&mut node.walk())
                .filter(|c| c.kind() == "inheritance_specifier")
            {
                if let Some(type_node) = spec
                    .children(&mut spec.walk())
                    .find(|c| matches!(c.kind(), "user_type" | "type_identifier" | "type"))
                {
                    let base_type_name = if type_node.kind() == "user_type" {
                        if let Some(inner_type_node) = type_node
                            .children(&mut type_node.walk())
                            .find(|c| c.kind() == "type_identifier")
                        {
                            self.base.get_node_text(&inner_type_node)
                        } else {
                            self.base.get_node_text(&type_node)
                        }
                    } else {
                        self.base.get_node_text(&type_node)
                    };
                    let pending_kind = Self::pending_inheritance_kind(
                        &type_symbol,
                        declaration_kind,
                        inheritance_entry_index,
                    );
                    self.add_inheritance_relationship(
                        &type_symbol,
                        &base_type_name,
                        pending_kind,
                        symbols,
                        relationships,
                        node,
                    );
                    inheritance_entry_index += 1;
                }
            }
        }
    }

    fn declaration_kind_for_relationships(&self, node: Node) -> &'static str {
        let declaration_text = self.base.get_node_text(&node);
        let declaration_head = declaration_text.trim_start();

        if declaration_head.starts_with("extension ") {
            "extension_declaration"
        } else if declaration_head.starts_with("enum ") {
            "enum_declaration"
        } else if declaration_head.starts_with("struct ") {
            "struct_declaration"
        } else if declaration_head.starts_with("protocol ") {
            "protocol_declaration"
        } else {
            node.kind()
        }
    }

    fn inheritance_type_name(&self, node: Node) -> Option<String> {
        match node.kind() {
            "type_identifier" | "type" => Some(self.base.get_node_text(&node)),
            "user_type" => node
                .children(&mut node.walk())
                .find(|child| child.kind() == "type_identifier")
                .map(|child| self.base.get_node_text(&child)),
            _ => None,
        }
    }

    fn pending_inheritance_kind(
        type_symbol: &Symbol,
        declaration_kind: &str,
        inheritance_entry_index: usize,
    ) -> RelationshipKind {
        match declaration_kind {
            "extension_declaration" | "struct_declaration" | "enum_declaration" => {
                RelationshipKind::Implements
            }
            "class_declaration" if inheritance_entry_index == 0 => RelationshipKind::Extends,
            "class_declaration" => RelationshipKind::Implements,
            _ => match type_symbol.kind {
                SymbolKind::Interface => RelationshipKind::Extends,
                SymbolKind::Struct | SymbolKind::Enum => RelationshipKind::Implements,
                SymbolKind::Class if inheritance_entry_index == 0 => RelationshipKind::Extends,
                SymbolKind::Class => RelationshipKind::Implements,
                _ => RelationshipKind::Extends,
            },
        }
    }

    /// Implementation of addInheritanceRelationship method
    fn add_inheritance_relationship(
        &mut self,
        type_symbol: &Symbol,
        base_type_name: &str,
        pending_kind: RelationshipKind,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
        node: Node,
    ) {
        // Find the actual base type symbol
        if let Some(base_type_symbol) = symbols.iter().find(|s| {
            s.name == base_type_name
                && matches!(
                    s.kind,
                    SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct
                )
        }) {
            // Determine relationship kind: classes extend, protocols implement
            let relationship_kind = if base_type_symbol.kind == SymbolKind::Interface {
                RelationshipKind::Implements
            } else {
                RelationshipKind::Extends
            };

            let metadata = HashMap::from([(
                "baseType".to_string(),
                serde_json::Value::String(base_type_name.to_string()),
            )]);

            relationships.push(Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    type_symbol.id,
                    base_type_symbol.id,
                    relationship_kind,
                    node.start_position().row
                ),
                from_symbol_id: type_symbol.id.clone(),
                to_symbol_id: base_type_symbol.id.clone(),
                kind: relationship_kind,
                file_path: self.base.file_path.clone(),
                line_number: (node.start_position().row + 1) as u32,
                confidence: 1.0,
                metadata: Some(metadata),
            });
        } else {
            let pending = self.base.create_pending_relationship(
                type_symbol.id.clone(),
                UnresolvedTarget::simple(base_type_name.to_string()),
                pending_kind,
                &node,
                Some(type_symbol.id.clone()),
                Some(0.9),
            );
            self.add_structured_pending_relationship(pending);
        }
    }

    /// Implementation of infer_types method
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            // For functions/methods, prefer returnType over generic type
            if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                if let Some(return_type) =
                    symbol.metadata.as_ref().and_then(|m| m.get("returnType"))
                {
                    if let Some(return_type_str) = return_type.as_str() {
                        types.insert(symbol.id.clone(), return_type_str.to_string());
                        continue;
                    }
                }
            }
            // For properties/variables, prefer propertyType or variableType
            else if matches!(symbol.kind, SymbolKind::Property | SymbolKind::Variable) {
                if let Some(property_type) =
                    symbol.metadata.as_ref().and_then(|m| m.get("propertyType"))
                {
                    if let Some(property_type_str) = property_type.as_str() {
                        types.insert(symbol.id.clone(), property_type_str.to_string());
                        continue;
                    }
                }
                if let Some(variable_type) =
                    symbol.metadata.as_ref().and_then(|m| m.get("variableType"))
                {
                    if let Some(variable_type_str) = variable_type.as_str() {
                        types.insert(symbol.id.clone(), variable_type_str.to_string());
                        continue;
                    }
                }
            }

            // Fallback to generic type from metadata
            if let Some(symbol_type) = symbol.metadata.as_ref().and_then(|m| m.get("type")) {
                if let Some(symbol_type_str) = symbol_type.as_str() {
                    types.insert(symbol.id.clone(), symbol_type_str.to_string());
                }
            } else if let Some(return_type) =
                symbol.metadata.as_ref().and_then(|m| m.get("returnType"))
            {
                if let Some(return_type_str) = return_type.as_str() {
                    types.insert(symbol.id.clone(), return_type_str.to_string());
                }
            }
        }
        types
    }

    /// Implementation of findTypeSymbol method
    pub(super) fn find_type_symbol(&self, node: Node, symbols: &[Symbol]) -> Option<Symbol> {
        if let Some(name_node) = self.declaration_name_node(node) {
            let type_name = self.base.get_node_text(&name_node);
            symbols
                .iter()
                .find(|s| {
                    s.name == type_name
                        && matches!(
                            s.kind,
                            SymbolKind::Class
                                | SymbolKind::Struct
                                | SymbolKind::Interface
                                | SymbolKind::Enum
                        )
                        && s.file_path == self.base.file_path
                })
                .cloned()
        } else {
            None
        }
    }

    fn declaration_name_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        node.children(&mut cursor).find_map(|child| {
            if child.kind() == "type_identifier" {
                Some(child)
            } else if child.kind() == "user_type" {
                child
                    .children(&mut child.walk())
                    .find(|nested| nested.kind() == "type_identifier")
            } else {
                None
            }
        })
    }

    /// Extract function/method call relationships
    ///
    /// Creates resolved Relationship when target is a local function/method.
    /// Creates PendingRelationship when target is not found in local symbol_map.
    fn extract_call_relationship(
        &mut self,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let symbol_index = ScopedSymbolIndex::new(symbols);

        // Extract the function/method name being called
        let function_name = self.extract_call_target_name(node);

        let Some(function_name) = function_name else {
            return;
        };

        let Some(caller) = self.base.find_containing_symbol(&node, symbols) else {
            return;
        };

        let target = self.unresolved_call_target(node, &function_name);
        let line_number = node.start_position().row as u32 + 1;
        let file_path = self.base.file_path.clone();

        match symbol_index.resolve_call_target(
            function_name.as_str(),
            Some(caller),
            target.receiver.as_deref(),
        ) {
            LocalTargetResolution::Resolved(called_symbol) => {
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        caller.id,
                        called_symbol.id,
                        RelationshipKind::Calls,
                        node.start_position().row
                    ),
                    from_symbol_id: caller.id.clone(),
                    to_symbol_id: called_symbol.id.clone(),
                    kind: RelationshipKind::Calls,
                    file_path,
                    line_number,
                    confidence: 0.9,
                    metadata: None,
                });
            }
            LocalTargetResolution::Import(_)
            | LocalTargetResolution::Ambiguous
            | LocalTargetResolution::ReceiverQualified
            | LocalTargetResolution::Missing => {
                let pending = self.base.create_pending_relationship(
                    caller.id.clone(),
                    target,
                    RelationshipKind::Calls,
                    &node,
                    Some(caller.id.clone()),
                    Some(0.7),
                );
                self.add_structured_pending_relationship(pending);
            }
        }
    }

    fn unresolved_call_target(&self, node: Node, fallback_name: &str) -> UnresolvedTarget {
        let call_text = self.base.get_node_text(&node);
        let call_head = call_text.split('(').next().unwrap_or(call_text.as_str());
        if let Some((receiver, terminal_name)) = call_head.rsplit_once('.') {
            let receiver = receiver.trim();
            let terminal_name = terminal_name.trim();
            if !receiver.is_empty() && !terminal_name.is_empty() {
                return UnresolvedTarget {
                    display_name: format!("{receiver}.{terminal_name}"),
                    terminal_name: terminal_name.to_string(),
                    receiver: Some(receiver.to_string()),
                    namespace_path: Vec::new(),
                    import_context: None,
                };
            }
        }

        let mut identifiers = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "simple_identifier" || child.kind() == "identifier" {
                identifiers.push(self.base.get_node_text(&child));
            }
        }

        if identifiers.len() >= 2 {
            let terminal_name = identifiers
                .pop()
                .unwrap_or_else(|| fallback_name.to_string());
            let receiver = identifiers.pop();
            let namespace_path = identifiers;
            let mut display_parts = namespace_path.clone();
            if let Some(receiver_name) = receiver.as_ref() {
                display_parts.push(receiver_name.clone());
            }
            display_parts.push(terminal_name.clone());
            return UnresolvedTarget {
                display_name: display_parts.join("."),
                terminal_name,
                receiver,
                namespace_path,
                import_context: None,
            };
        }

        UnresolvedTarget::simple(fallback_name.to_string())
    }

    /// Extract the name of the function/method being called
    fn extract_call_target_name(&self, node: Node) -> Option<String> {
        // Try to extract the function/method name from a call_expression
        // A call_expression can be:
        // 1. function_name(...) -> simple_identifier
        // 2. object.method(...) -> postfix_expression with member_access

        // Get the first child that's not a comment or whitespace
        let mut cursor = node.walk();
        let first_child = node.children(&mut cursor).next()?;

        match first_child.kind() {
            "simple_identifier" => {
                // Direct function call
                Some(self.base.get_node_text(&first_child))
            }
            "postfix_expression" | "navigation_expression" => {
                // Method call or qualified call
                self.extract_rightmost_call_identifier(first_child)
            }
            _ => None,
        }
    }

    /// Extract the rightmost simple_identifier from a call node
    fn extract_rightmost_call_identifier(&self, node: Node) -> Option<String> {
        let mut result = None;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if child.kind() == "simple_identifier" || child.kind() == "identifier" {
                result = Some(self.base.get_node_text(&child));
            } else if child.kind() == "postfix_expression"
                || child.kind() == "member_access_expression"
                || child.kind() == "navigation_expression"
            {
                // Recursively look in nested expressions
                if let Some(inner) = self.extract_rightmost_call_identifier(child) {
                    result = Some(inner);
                }
            }
        }

        result
    }
}
