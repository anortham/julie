//! Type extraction for classes, properties, and exports
//!
//! Handles extraction of class declarations, property definitions,
//! and export statements.

use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use serde_json::json;
use std::collections::HashMap;
use tree_sitter::Node;

impl super::JavaScriptExtractor {
    /// Extract class declarations - direct Implementation of extractClass
    pub(super) fn extract_class(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node.child_by_field_name("name");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());

        // Extract extends clause (reference logic)
        let heritage = node.child_by_field_name("heritage").or_else(|| {
            node.children(&mut node.walk())
                .find(|c| c.kind() == "class_heritage")
        });

        let extends_clause = heritage.and_then(|h| {
            h.children(&mut h.walk())
                .find(|c| c.kind() == "extends_clause")
        });

        let signature = self.build_class_signature(&node);

        let mut metadata = HashMap::new();
        metadata.insert(
            "extends".to_string(),
            json!(extends_clause.map(|ec| self.base.get_node_text(&ec))),
        );
        metadata.insert("isGenerator".to_string(), json!(false)); // JavaScript classes are not generators
        metadata.insert(
            "hasPrivateFields".to_string(),
            json!(self.has_private_fields(&node)),
        );

        // Extract JSDoc comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Class,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.extract_visibility(&node)),
                parent_id,
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Extract property definitions - implementation's extractProperty
    pub(super) fn extract_property(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let name_node = node
            .child_by_field_name("key")
            .or_else(|| node.child_by_field_name("name"))
            .or_else(|| node.child_by_field_name("property"));

        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "Anonymous".to_string());
        let value_node = node.child_by_field_name("value");
        let signature = self.build_property_signature(&node, &name);

        // If the value is a function, treat it as a method (reference logic)
        if let Some(value) = &value_node {
            if value.kind() == "arrow_function"
                || value.kind() == "function_expression"
                || value.kind() == "generator_function"
            {
                let method_signature = self.build_method_signature(value, &name);

                let mut metadata = HashMap::new();
                metadata.insert("isAsync".to_string(), json!(self.is_async(value)));
                metadata.insert("isGenerator".to_string(), json!(self.is_generator(value)));
                metadata.insert(
                    "parameters".to_string(),
                    json!(self.extract_parameters(value)),
                );

                // Extract JSDoc comment
                let doc_comment = self.base.find_doc_comment(&node);

                return self.base.create_symbol(
                    &node,
                    name,
                    SymbolKind::Method,
                    SymbolOptions {
                        signature: Some(method_signature),
                        visibility: Some(self.extract_visibility(&node)),
                        parent_id,
                        metadata: Some(metadata),
                        doc_comment,
                    },
                );
            }
        }

        // Determine if this is a class field or regular property (reference logic)
        let symbol_kind = match node.kind() {
            "public_field_definition" | "field_definition" | "property_definition" => {
                SymbolKind::Field
            }
            _ => SymbolKind::Property,
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "value".to_string(),
            json!(value_node.map(|v| self.base.get_node_text(&v))),
        );
        metadata.insert(
            "isComputed".to_string(),
            json!(self.is_computed_property(&node)),
        );
        metadata.insert("isPrivate".to_string(), json!(name.starts_with('#')));

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

    /// Extract export declarations - implementation's extractExport
    pub(super) fn extract_export(&mut self, node: Node, parent_id: Option<String>) -> Symbol {
        let exported_name = self.extract_exported_name(&node);
        let signature = self.base.get_node_text(&node);

        let mut metadata = HashMap::new();
        metadata.insert("exportedName".to_string(), json!(exported_name));
        metadata.insert(
            "isDefault".to_string(),
            json!(self.is_default_export(&node)),
        );
        metadata.insert("isNamed".to_string(), json!(self.is_named_export(&node)));

        // Extract JSDoc comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            exported_name.clone(),
            SymbolKind::Export,
            SymbolOptions {
                signature: Some(signature),
                visibility: None,
                parent_id,
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Extract exported name - implementation's extractExportedName
    pub(super) fn extract_exported_name(&self, node: &Node) -> String {
        // Handle different export patterns (reference logic)
        for child in node.children(&mut node.walk()) {
            match child.kind() {
                // Direct exports: export const Component = ..., export function foo() {}, export class Bar {}
                "variable_declaration" | "lexical_declaration" => {
                    let declarator = child
                        .children(&mut child.walk())
                        .find(|c| c.kind() == "variable_declarator");
                    if let Some(decl) = declarator {
                        if let Some(name_node) = decl.child_by_field_name("name") {
                            return self.base.get_node_text(&name_node);
                        }
                    }
                }
                "class_declaration" | "function_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        return self.base.get_node_text(&name_node);
                    }
                }
                "identifier" => {
                    // Simple export: export identifier
                    return self.base.get_node_text(&child);
                }
                "export_clause" => {
                    // Handle export { default as Component } patterns (reference logic)
                    for clause_child in child.children(&mut child.walk()) {
                        if clause_child.kind() == "export_specifier" {
                            let children: Vec<_> =
                                clause_child.children(&mut clause_child.walk()).collect();
                            for i in 0..children.len() {
                                if self.base.get_node_text(&children[i]) == "as"
                                    && i + 1 < children.len()
                                {
                                    return self.base.get_node_text(&children[i + 1]);
                                }
                            }
                            // If no "as", return the export name
                            if let Some(name_node) =
                                children.iter().find(|c| c.kind() == "identifier")
                            {
                                return self.base.get_node_text(name_node);
                            }
                        }
                    }
                }
                "export_specifier" => {
                    // Named export specifier (direct child)
                    if let Some(name_node) = child.child_by_field_name("name") {
                        return self.base.get_node_text(&name_node);
                    }
                }
                _ => {}
            }
        }

        // Look for default exports (reference logic)
        if self.is_default_export(node) {
            return "default".to_string();
        }

        "unknown".to_string()
    }
}
