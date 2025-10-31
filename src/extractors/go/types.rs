use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Type declaration extraction for Go (structs, interfaces, type aliases)
impl super::GoExtractor {
    pub(super) fn extract_package(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Find package identifier node
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "package_identifier" {
                let name = self.get_node_text(child);
                let signature = format!("package {}", name);

                return Some(self.base.create_symbol(
                    &child,
                    name,
                    SymbolKind::Namespace,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: None,
                    },
                ));
            }
        }
        None
    }

    pub(super) fn extract_type_declaration(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Find type_spec or type_alias node which contains the actual type definition
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "type_spec" {
                return self.extract_type_spec(child, parent_id);
            } else if child.kind() == "type_alias" {
                return self.extract_type_alias(child, parent_id);
            }
        }
        None
    }

    pub(super) fn extract_type_spec(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Try to find doc comment - look for it on the node first, then on the parent
        let doc_comment_from_spec = self.base.find_doc_comment(&node);
        let doc_comment = if doc_comment_from_spec.is_none() {
            // If not found on spec, try to find it on the parent type_declaration
            if let Some(parent) = node.parent() {
                self.base.find_doc_comment(&parent)
            } else {
                None
            }
        } else {
            doc_comment_from_spec
        };

        let mut cursor = node.walk();
        let mut type_identifier = None;
        let mut type_parameters = None;
        let mut type_def = None;
        let mut second_type_identifier = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" if type_identifier.is_none() => type_identifier = Some(child),
                "type_identifier"
                    if type_identifier.is_some() && second_type_identifier.is_none() =>
                {
                    // Second type_identifier indicates type alias (type Alias = Target)
                    second_type_identifier = Some(child);
                    type_def = Some(("alias", child));
                }
                "type_parameter_list" => type_parameters = Some(child),
                "struct_type" => type_def = Some(("struct", child)),
                "interface_type" => type_def = Some(("interface", child)),
                "=" => {} // Type alias syntax detected (handled by second_type_identifier)
                // Handle basic type definitions (type UserID int64) and aliases (type UserID = int64)
                "primitive_type" if type_identifier.is_some() && type_def.is_none() => {
                    type_def = Some(("definition", child));
                }
                "pointer_type" | "slice_type" | "map_type" | "array_type" | "channel_type"
                | "function_type" | "qualified_type" | "generic_type"
                    if type_identifier.is_some() && type_def.is_none() =>
                {
                    type_def = Some(("definition", child));
                }
                _ => {}
            }
        }

        if let (Some(type_id), Some((type_kind, type_node))) = (type_identifier, type_def) {
            let name = self.get_node_text(type_id);
            let type_params = type_parameters
                .map(|tp| self.get_node_text(tp))
                .unwrap_or_default();

            let visibility = if self.is_public(&name) {
                Some(Visibility::Public)
            } else {
                Some(Visibility::Private)
            };

            match type_kind {
                "struct" => {
                    let signature = format!("type {}{} struct", name, type_params);
                    Some(self.base.create_symbol(
                        &type_id,
                        name,
                        SymbolKind::Class,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility,
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment: doc_comment.clone(),
                        },
                    ))
                }
                "interface" => {
                    let mut signature = format!("type {}{} interface", name, type_params);

                    // Extract interface body content for union types and methods
                    let interface_body = self.extract_interface_body(type_node);
                    if !interface_body.is_empty() {
                        signature += &format!(" {{ {} }}", interface_body);
                    }

                    Some(self.base.create_symbol(
                        &type_id,
                        name,
                        SymbolKind::Interface,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility,
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment: doc_comment.clone(),
                        },
                    ))
                }
                "alias" => {
                    // For type alias, extract the aliased type
                    let aliased_type = self.extract_type_from_node(type_node);
                    let signature = format!("type {}{} = {}", name, type_params, aliased_type);
                    Some(self.base.create_symbol(
                        &type_id,
                        name,
                        SymbolKind::Type,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility,
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment: doc_comment.clone(),
                        },
                    ))
                }
                "definition" => {
                    // For type definition (no equals sign) - formats these like aliases
                    let aliased_type = self.extract_type_from_node(type_node);
                    let signature = format!("type {}{} = {}", name, type_params, aliased_type);
                    Some(self.base.create_symbol(
                        &type_id,
                        name,
                        SymbolKind::Type,
                        SymbolOptions {
                            signature: Some(signature),
                            visibility,
                            parent_id: parent_id.map(|s| s.to_string()),
                            metadata: None,
                            doc_comment: doc_comment.clone(),
                        },
                    ))
                }
                _ => None,
            }
        } else {
            None
        }
    }

    pub(super) fn extract_type_alias(
        &mut self,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        // Try to find doc comment - look for it on the node first, then on the parent
        let doc_comment_from_alias = self.base.find_doc_comment(&node);
        let doc_comment = if doc_comment_from_alias.is_none() {
            // If not found on alias, try to find it on the parent type_declaration
            if let Some(parent) = node.parent() {
                self.base.find_doc_comment(&parent)
            } else {
                None
            }
        } else {
            doc_comment_from_alias
        };

        // Parse type_alias node: "TypeAlias = string"
        let mut cursor = node.walk();
        let mut alias_name = None;
        let mut target_type = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" if alias_name.is_none() => alias_name = Some(child),
                "type_identifier" | "primitive_type" | "pointer_type" | "slice_type"
                | "map_type" | "array_type" | "channel_type" | "function_type"
                | "qualified_type" | "generic_type"
                    if alias_name.is_some() =>
                {
                    target_type = Some(child);
                }
                _ => {}
            }
        }

        if let (Some(alias_node), Some(target_node)) = (alias_name, target_type) {
            let name = self.get_node_text(alias_node);
            let target_type_text = self.extract_type_from_node(target_node);
            let signature = format!("type {} = {}", name, target_type_text);

            let mut metadata = HashMap::new();
            metadata.insert(
                "alias_target".to_string(),
                serde_json::Value::String(target_type_text),
            );

            return Some(self.base.create_symbol(
                &node,
                name,
                SymbolKind::Type,
                SymbolOptions {
                    signature: Some(signature),
                    visibility: Some(Visibility::Public),
                    parent_id: parent_id.map(|s| s.to_string()),
                    metadata: Some(metadata),
                    doc_comment,
                },
            ));
        }

        None
    }

    pub(super) fn extract_field(&mut self, node: Node, parent_id: Option<&str>) -> Vec<Symbol> {
        // Go field_declaration structure:
        // field_declaration
        //   field_identifier (name) - can have MULTIPLE on same line (X, Y float64)
        //   primitive_type | slice_type | etc. (type)
        //   [optional] field_tag (like `json:"id"`)

        let mut cursor = node.walk();
        let mut field_names = Vec::new();
        let mut field_type = None;
        let mut field_tag = None;

        // Collect all field_identifier nodes (can have multiple on same line: X, Y float64)
        for child in node.children(&mut cursor) {
            match child.kind() {
                "field_identifier" => {
                    field_names.push(child);
                }
                "primitive_type" | "type_identifier" | "pointer_type" | "slice_type"
                | "map_type" | "array_type" | "channel_type" | "function_type"
                | "qualified_type" | "generic_type" | "interface_type" | "struct_type" => {
                    if field_type.is_none() {
                        field_type = Some(child);
                    }
                }
                "field_tag" => {
                    field_tag = Some(child);
                }
                _ => {}
            }
        }

        // Create a symbol for EACH field name (handles X, Y float64 pattern)
        let mut symbols = Vec::new();

        if let Some(type_node) = field_type {
            let type_text = self.extract_type_from_node(type_node);
            let tag_text = field_tag.map(|tag| self.get_node_text(tag));

            for field_name_node in field_names {
                let name = self.get_node_text(field_name_node);

                // Build signature
                let mut signature = format!("{} {}", name, type_text);
                if let Some(ref tag) = tag_text {
                    signature.push(' ');
                    signature.push_str(tag);
                }

                // Determine visibility (Go rule: uppercase first letter = public)
                let visibility = if self.is_public(&name) {
                    Some(Visibility::Public)
                } else {
                    Some(Visibility::Private)
                };

                // Create symbol for this field
                let symbol = self.base.create_symbol(
                    &field_name_node,
                    name,
                    SymbolKind::Field,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility,
                        parent_id: parent_id.map(|s| s.to_string()),
                        metadata: None,
                        doc_comment: None,
                    },
                );

                symbols.push(symbol);
            }
        }

        symbols
    }
}
