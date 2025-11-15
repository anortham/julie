//! Function and method extraction for JavaScript
//!
//! Handles extraction of function declarations, function expressions,
//! arrow functions, methods, and constructors.

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::Node;

impl super::JavaScriptExtractor {
    /// Extract function declarations - direct Implementation of extractFunction
    pub(super) fn extract_function(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let mut name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());

        // Handle arrow functions assigned to variables (reference logic)
        if node.kind() == "arrow_function" || node.kind() == "function_expression" {
            if let Some(parent) = node.parent() {
                if parent.kind() == "variable_declarator" {
                    if let Some(var_name_node) = parent.child_by_field_name("name") {
                        name = self.base.get_node_text(&var_name_node);
                    }
                } else if parent.kind() == "assignment_expression" {
                    if let Some(left_node) = parent.child_by_field_name("left") {
                        name = self.base.get_node_text(&left_node);
                    }
                } else if parent.kind() == "pair" {
                    if let Some(key_node) = parent.child_by_field_name("key") {
                        name = self.base.get_node_text(&key_node);
                    }
                }
            }
        }

        let signature = self.build_function_signature(&node, &name);

        let mut metadata = HashMap::new();
        metadata.insert("isAsync".to_string(), json!(self.is_async(&node)));
        metadata.insert("isGenerator".to_string(), json!(self.is_generator(&node)));
        metadata.insert(
            "isArrowFunction".to_string(),
            json!(node.kind() == "arrow_function"),
        );
        metadata.insert(
            "parameters".to_string(),
            json!(self.extract_parameters(&node)),
        );
        metadata.insert(
            "isExpression".to_string(),
            json!(node.kind() == "function_expression"),
        );

        // Extract JSDoc comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Function,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.extract_visibility(&node)),
                parent_id,
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Extract method definitions - implementation's extractMethod
    pub(super) fn extract_method(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("property"))
            .or_else(|| node.child_by_field_name("key"));

        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());

        let signature = self.build_method_signature(&node, &name);

        // Determine if it's a constructor (reference logic)
        let symbol_kind = if name == "constructor" {
            SymbolKind::Constructor
        } else {
            SymbolKind::Method
        };

        // Check for getters and setters (reference logic)
        let is_getter = node.children(&mut node.walk()).any(|c| c.kind() == "get");
        let is_setter = node.children(&mut node.walk()).any(|c| c.kind() == "set");

        let mut metadata = HashMap::new();
        metadata.insert(
            "isStatic".to_string(),
            json!(
                node.children(&mut node.walk())
                    .any(|c| c.kind() == "static")
            ),
        );
        metadata.insert("isAsync".to_string(), json!(self.is_async(&node)));
        metadata.insert("isGenerator".to_string(), json!(self.is_generator(&node)));
        metadata.insert("isGetter".to_string(), json!(is_getter));
        metadata.insert("isSetter".to_string(), json!(is_setter));
        metadata.insert("isPrivate".to_string(), json!(name.starts_with('#')));
        metadata.insert(
            "parameters".to_string(),
            json!(self.extract_parameters(&node)),
        );

        // Extract JSDoc comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.extract_visibility(&node)),
                parent_id,
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }
}
