//! Variable and destructuring extraction for JavaScript
//!
//! Handles extraction of variable declarations, including destructuring
//! patterns for objects and arrays.

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::Node;

impl super::JavaScriptExtractor {
    /// Extract variable declarations - direct port of Miller's extractVariable
    pub(super) fn extract_variable(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());

        let value_node = node.child_by_field_name("value");
        let signature = self.build_variable_signature(&node, &name);

        // Check if this is a CommonJS require statement (Miller's logic)
        if let Some(value) = &value_node {
            if self.is_require_call(value) {
                let mut metadata = HashMap::new();
                metadata.insert(
                    "source".to_string(),
                    json!(self.extract_require_source(value)),
                );
                metadata.insert("isCommonJS".to_string(), json!(true));

                return self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Import,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(self.extract_visibility(&node)),
                        parent_id,
                        metadata: Some(metadata),
                        doc_comment: None,
                    },
                );
            }

            // For function expressions, create a function symbol with the variable's name (Miller's logic)
            if value.kind() == "arrow_function"
                || value.kind() == "function_expression"
                || value.kind() == "generator_function"
            {
                let mut metadata = HashMap::new();
                metadata.insert("isAsync".to_string(), json!(self.is_async(value)));
                metadata.insert("isGenerator".to_string(), json!(self.is_generator(value)));
                metadata.insert(
                    "isArrowFunction".to_string(),
                    json!(value.kind() == "arrow_function"),
                );
                metadata.insert("isExpression".to_string(), json!(true));
                metadata.insert(
                    "parameters".to_string(),
                    json!(self.extract_parameters(value)),
                );

                return self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Function,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(self.extract_visibility(&node)),
                        parent_id,
                        metadata: Some(metadata),
                        doc_comment: None,
                    },
                );
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "declarationType".to_string(),
            json!(self.get_declaration_type(&node)),
        );
        metadata.insert(
            "initializer".to_string(),
            json!(value_node.map(|v| self.base.get_node_text(&v))),
        );
        metadata.insert(
            "isConst".to_string(),
            json!(self.is_const_declaration(&node)),
        );
        metadata.insert("isLet".to_string(), json!(self.is_let_declaration(&node)));

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.extract_visibility(&node)),
                parent_id,
                metadata: Some(metadata),
                doc_comment: None,
            },
        )
    }

    /// Extract destructuring variables - direct port of Miller's extractDestructuringVariables
    pub(super) fn extract_destructuring_variables(
        &mut self,
        node: Node,
        parent_id: Option<String>,
    ) -> Vec<Symbol> {
        let name_node = node.child_by_field_name("name");
        let value_node = node.child_by_field_name("value");
        let mut symbols = Vec::new();

        if let Some(name) = name_node {
            let declaration_type = self.get_declaration_type(&node);
            let value_text = value_node
                .map(|v| self.base.get_node_text(&v))
                .unwrap_or_default();

            match name.kind() {
                "object_pattern" => {
                    // Handle object destructuring: const { name, age, ...rest } = user (Miller's logic)
                    for child in name.children(&mut name.walk()) {
                        match child.kind() {
                            "shorthand_property_identifier_pattern"
                            | "property_identifier"
                            | "identifier" => {
                                let var_name = self.base.get_node_text(&child);
                                let signature = format!(
                                    "{} {{ {} }} = {}",
                                    declaration_type, var_name, value_text
                                );

                                let mut metadata = HashMap::new();
                                metadata
                                    .insert("declarationType".to_string(), json!(declaration_type));
                                metadata.insert("isDestructured".to_string(), json!(true));
                                metadata.insert("destructuringType".to_string(), json!("object"));

                                symbols.push(self.base.create_symbol(
                                    &node,
                                    var_name,
                                    SymbolKind::Variable,
                                    SymbolOptions {
                                        signature: Some(signature),
                                        visibility: Some(self.extract_visibility(&node)),
                                        parent_id: parent_id.clone(),
                                        metadata: Some(metadata),
                                        doc_comment: None,
                                    },
                                ));
                            }
                            "rest_pattern" => {
                                // Handle rest parameters: const { name, ...rest } = user (Miller's logic)
                                if let Some(rest_identifier) = child
                                    .children(&mut child.walk())
                                    .find(|c| c.kind() == "identifier")
                                {
                                    let var_name = self.base.get_node_text(&rest_identifier);
                                    let signature = format!(
                                        "{} {{ ...{} }} = {}",
                                        declaration_type, var_name, value_text
                                    );

                                    let mut metadata = HashMap::new();
                                    metadata.insert(
                                        "declarationType".to_string(),
                                        json!(declaration_type),
                                    );
                                    metadata.insert("isDestructured".to_string(), json!(true));
                                    metadata
                                        .insert("destructuringType".to_string(), json!("object"));
                                    metadata.insert("isRestParameter".to_string(), json!(true));

                                    symbols.push(self.base.create_symbol(
                                        &node,
                                        var_name,
                                        SymbolKind::Variable,
                                        SymbolOptions {
                                            signature: Some(signature),
                                            visibility: Some(self.extract_visibility(&node)),
                                            parent_id: parent_id.clone(),
                                            metadata: Some(metadata),
                                            doc_comment: None,
                                        },
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "array_pattern" => {
                    // Handle array destructuring: const [first, second] = array (Miller's logic)
                    let mut index = 0;
                    for child in name.children(&mut name.walk()) {
                        if child.kind() == "identifier" {
                            let var_name = self.base.get_node_text(&child);
                            let signature =
                                format!("{} [{}] = {}", declaration_type, var_name, value_text);

                            let mut metadata = HashMap::new();
                            metadata.insert("declarationType".to_string(), json!(declaration_type));
                            metadata.insert("isDestructured".to_string(), json!(true));
                            metadata.insert("destructuringType".to_string(), json!("array"));
                            metadata.insert("destructuringIndex".to_string(), json!(index));

                            symbols.push(self.base.create_symbol(
                                &node,
                                var_name,
                                SymbolKind::Variable,
                                SymbolOptions {
                                    signature: Some(signature),
                                    visibility: Some(self.extract_visibility(&node)),
                                    parent_id: parent_id.clone(),
                                    metadata: Some(metadata),
                                    doc_comment: None,
                                },
                            ));
                            index += 1;
                        }
                    }
                }
                _ => {}
            }
        }

        symbols
    }
}
