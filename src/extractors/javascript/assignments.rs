//! Assignment expression handling for JavaScript
//!
//! Handles extraction of member assignment expressions,
//! including prototype methods and static method assignments.

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::Node;

impl super::JavaScriptExtractor {
    /// Extract assignment expressions - direct Implementation of extractAssignment
    pub(super) fn extract_assignment(
        &mut self,
        node: Node,
        parent_id: Option<String>,
    ) -> Option<Symbol> {
        let left_node = node.child_by_field_name("left");
        let right_node = node.child_by_field_name("right");

        if let Some(left) = left_node {
            // Handle member expression assignments like: Constructor.prototype.method = function() {} (reference logic)
            if left.kind() == "member_expression" {
                let object_node = left.child_by_field_name("object");
                let property_node = left.child_by_field_name("property");

                if let (Some(object), Some(property)) = (object_node, property_node) {
                    let object_text = self.base.get_node_text(&object);
                    let property_name = self.base.get_node_text(&property);
                    let signature = self.base.get_node_text(&node);

                    // Check if this is a prototype assignment (reference logic)
                    if object_text.contains(".prototype") {
                        let mut metadata = HashMap::new();
                        metadata.insert("isPrototypeMethod".to_string(), json!(true));
                        metadata.insert(
                            "isFunction".to_string(),
                            json!(
                                right_node
                                    .map(|r| r.kind() == "function_expression"
                                        || r.kind() == "arrow_function")
                                    .unwrap_or(false)
                            ),
                        );

                        // Extract JSDoc comment
                        let doc_comment = self.base.find_doc_comment(&node);

                        return Some(self.base.create_symbol(
                            &node,
                            property_name,
                            SymbolKind::Method,
                            SymbolOptions {
                                signature: Some(signature),
                                visibility: Some(Visibility::Public),
                                parent_id,
                                metadata: Some(metadata),
                                doc_comment,
                            },
                        ));
                    }
                    // Check if this is a static method assignment (reference logic)
                    else if let Some(right) = right_node {
                        if right.kind() == "function_expression" || right.kind() == "arrow_function"
                        {
                            let mut metadata = HashMap::new();
                            metadata.insert("isStaticMethod".to_string(), json!(true));
                            metadata.insert("isFunction".to_string(), json!(true));
                            metadata.insert("className".to_string(), json!(object_text));

                            // Extract JSDoc comment
                            let doc_comment = self.base.find_doc_comment(&node);

                            return Some(self.base.create_symbol(
                                &node,
                                property_name,
                                SymbolKind::Method,
                                SymbolOptions {
                                    signature: Some(signature),
                                    visibility: Some(Visibility::Public),
                                    parent_id,
                                    metadata: Some(metadata),
                                    doc_comment,
                                },
                            ));
                        }
                    }
                }
            }
        }

        None
    }
}
