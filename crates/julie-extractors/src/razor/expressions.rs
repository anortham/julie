/// Extraction for C# variable declarations in Razor files
use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

impl super::RazorExtractor {
    /// Extract variable declaration
    pub(super) fn extract_variable_declaration(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Extract variable name and type from variable declaration
        let mut variable_type = None;

        // Find the type (if present)
        if let Some(type_node) = self.find_child_by_types(
            node,
            &[
                "predefined_type",
                "identifier",
                "generic_name",
                "qualified_name",
                "nullable_type",
                "array_type",
                "var",
            ],
        ) {
            let type_text = self.base.get_node_text(&type_node);
            if type_text != "var" {
                // Don't use "var" as the actual type
                variable_type = Some(type_text);
            }
        }

        // Find variable declarators
        let mut declarators = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                if let Some(identifier) = self.find_child_by_type(child, "identifier") {
                    let name = self.base.get_node_text(&identifier);

                    // Look for initializer
                    let mut initializer = None;
                    let mut decl_cursor = child.walk();
                    let decl_children: Vec<_> = child.children(&mut decl_cursor).collect();
                    if let Some(equals_pos) = decl_children.iter().position(|c| c.kind() == "=") {
                        if equals_pos + 1 < decl_children.len() {
                            initializer =
                                Some(self.base.get_node_text(&decl_children[equals_pos + 1]));
                        }
                    }

                    declarators.push((name, initializer));
                }
            }
        }

        // For now, handle the first declarator (most common case)
        if let Some((name, initializer)) = declarators.first() {
            let variable_name = name.clone();

            let mut signature_parts = Vec::new();
            if let Some(ref var_type) = variable_type {
                signature_parts.push(var_type.clone());
            } else {
                signature_parts.push("var".to_string());
            }
            signature_parts.push(variable_name.clone());
            if let Some(init) = initializer {
                signature_parts.push(format!("= {}", init));
            }

            Some(self.base.create_symbol(
                &node,
                variable_name,
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(signature_parts.join(" ")),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some({
                        let mut metadata = HashMap::new();
                        metadata.insert(
                            "type".to_string(),
                            serde_json::Value::String("variable-declaration".to_string()),
                        );
                        if let Some(var_type) = variable_type {
                            metadata.insert(
                                "variableType".to_string(),
                                serde_json::Value::String(var_type),
                            );
                        }
                        if let Some(init) = initializer {
                            metadata.insert(
                                "initializer".to_string(),
                                serde_json::Value::String(init.clone()),
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

}
