//! Signature building functions for JavaScript symbols
//!
//! This module handles the construction of string signatures for various
//! symbol types (classes, functions, methods, properties, variables).
//! Signatures are the human-readable representations shown in IDEs.

use crate::extractors::base::BaseExtractor;
use tree_sitter::Node;

impl super::JavaScriptExtractor {
    /// Build class signature - direct Implementation of buildClassSignature
    pub(super) fn build_class_signature(&self, node: &Node) -> String {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());

        let mut signature = format!("class {}", name);

        // Look for extends clause (reference logic)
        let heritage = node.child_by_field_name("superclass").or_else(|| {
            node.children(&mut node.walk())
                .find(|c| c.kind() == "class_heritage")
        });

        if let Some(h) = heritage {
            if h.kind() == "identifier" {
                // Direct superclass reference
                signature.push_str(&format!(" extends {}", self.base.get_node_text(&h)));
            } else {
                // Look within class_heritage for extends_clause or identifier
                for child in h.children(&mut h.walk()) {
                    if child.kind() == "identifier" {
                        signature
                            .push_str(&format!(" extends {}", self.base.get_node_text(&child)));
                        break;
                    }
                }
            }
        }

        signature
    }

    /// Build function signature - direct Implementation of buildFunctionSignature
    pub(super) fn build_function_signature(&self, node: &Node, name: &str) -> String {
        let is_async = self.is_async(node);
        let is_generator = self.is_generator(node);
        let parameters = self.extract_parameters(node);

        let mut signature = String::new();

        if is_async {
            signature.push_str("async ");
        }

        match node.kind() {
            "arrow_function" => {
                if is_generator {
                    signature.push_str("function* ");
                }
                signature.push_str(&format!("{} = ({}) => ", name, parameters.join(", ")));
            }
            "function_expression" => {
                if is_generator {
                    signature.push_str("function* ");
                } else {
                    signature.push_str("function ");
                }
                signature.push_str(&format!("{}({})", name, parameters.join(", ")));
            }
            _ => {
                if is_generator {
                    signature.push_str("function* ");
                } else {
                    signature.push_str("function ");
                }
                signature.push_str(&format!("{}({})", name, parameters.join(", ")));
            }
        }

        signature
    }

    /// Build method signature - implementation's buildMethodSignature
    pub(super) fn build_method_signature(&self, node: &Node, name: &str) -> String {
        let is_async = self.is_async(node);
        let is_generator = self.is_generator(node);
        let is_static = node
            .children(&mut node.walk())
            .any(|c| c.kind() == "static");
        let is_getter = node.children(&mut node.walk()).any(|c| c.kind() == "get");
        let is_setter = node.children(&mut node.walk()).any(|c| c.kind() == "set");
        let parameters = self.extract_parameters(node);

        let mut signature = String::new();

        if is_static {
            signature.push_str("static ");
        }
        if is_async {
            signature.push_str("async ");
        }
        if is_getter {
            signature.push_str("get ");
        }
        if is_setter {
            signature.push_str("set ");
        }
        if is_generator {
            signature.push('*');
        }

        signature.push_str(&format!("{}({})", name, parameters.join(", ")));

        signature
    }

    /// Build variable signature - implementation's buildVariableSignature
    pub(super) fn build_variable_signature(&self, node: &Node, name: &str) -> String {
        let declaration_type = self.get_declaration_type(node);
        let value_node = node.child_by_field_name("value");

        let mut signature = format!("{} {}", declaration_type, name);

        if let Some(value) = value_node {
            match value.kind() {
                "function_expression" => {
                    signature.push_str(" = function");
                    let params = self.extract_parameters(&value);
                    signature.push_str(&format!("({})", params.join(", ")));
                }
                "arrow_function" => {
                    let is_async = self.is_async(&value);
                    if is_async {
                        signature.push_str(" = async ");
                    } else {
                        signature.push_str(" = ");
                    }

                    let params = self.extract_parameters(&value);
                    signature.push_str(&format!("({}) =>", params.join(", ")));

                    // For simple arrow functions, include the body if it's a simple expression (reference logic)
                    let body_node = value.children(&mut value.walk()).find(|c| {
                        matches!(
                            c.kind(),
                            "expression"
                                | "binary_expression"
                                | "call_expression"
                                | "identifier"
                                | "number"
                                | "string"
                        )
                    });

                    if let Some(body) = body_node {
                        let body_text = self.base.get_node_text(&body);
                        if body_text.len() <= 30 {
                            signature.push_str(&format!(" {}", body_text));
                        }
                    }
                }
                _ => {
                    let value_text = self.base.get_node_text(&value);
                    // Truncate very long values (reference logic) - safely handling UTF-8
                    let truncated_value = BaseExtractor::truncate_string(&value_text, 50);
                    signature.push_str(&format!(" = {}", truncated_value));
                }
            }
        }

        signature
    }

    /// Build property signature - implementation's buildPropertySignature
    pub(super) fn build_property_signature(&self, node: &Node, name: &str) -> String {
        let value_node = node.child_by_field_name("value");

        let mut signature = name.to_string();

        if let Some(value) = value_node {
            let value_text = self.base.get_node_text(&value);
            // Safely truncate UTF-8 string at character boundary
            let truncated_value = BaseExtractor::truncate_string(&value_text, 30);
            signature.push_str(&format!(": {}", truncated_value));
        }

        signature
    }
}
