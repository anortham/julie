use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts Swift extensions, imports, and type aliases
impl SwiftExtractor {
    /// Implementation of extractExtension method
    pub(super) fn extract_extension(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let type_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = type_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownExtension".to_string());

        let modifiers = self.extract_modifiers(node);
        let conformance = self.extract_inheritance(node);

        let mut signature = format!("extension {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref conformance) = conformance {
            signature.push_str(&format!(": {}", conformance));
        }

        let metadata = HashMap::from([
            ("type".to_string(), "extension".to_string()),
            ("modifiers".to_string(), modifiers.join(", ")),
            ("extendedType".to_string(), name.clone()),
        ]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        let options = self.create_symbol_options(
            Some(signature),
            Some(Visibility::Public),
            parent_id.map(|s| s.to_string()),
            metadata,
            doc_comment,
        );

        self.base
            .create_symbol(&node, name, SymbolKind::Class, options)
    }

    /// Implementation of extractImport method
    pub(super) fn extract_import(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownImport".to_string());

        let metadata = HashMap::from([("type".to_string(), "import".to_string())]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        let options = self.create_symbol_options(
            Some(format!("import {}", name)),
            Some(Visibility::Public),
            parent_id.map(|s| s.to_string()),
            metadata,
            doc_comment,
        );

        self.base
            .create_symbol(&node, name, SymbolKind::Import, options)
    }

    /// Implementation of extractTypeAlias method
    pub(super) fn extract_type_alias(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownTypeAlias".to_string());

        // Find the type that the alias refers to
        let children: Vec<_> = node.children(&mut node.walk()).collect();
        let aliased_type = if let Some(equal_index) = children
            .iter()
            .position(|c| self.base.get_node_text(c) == "=")
        {
            children
                .get(equal_index + 1)
                .map(|type_node| self.base.get_node_text(type_node))
                .unwrap_or_else(String::new)
        } else {
            String::new()
        };

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);

        let mut signature = format!("typealias {}", name);

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if !aliased_type.is_empty() {
            signature.push_str(&format!(" = {}", aliased_type));
        }

        let metadata = HashMap::from([
            ("type".to_string(), "typealias".to_string()),
            ("aliasedType".to_string(), aliased_type),
            ("modifiers".to_string(), modifiers.join(", ")),
        ]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        let options = self.create_symbol_options(
            Some(signature),
            Some(self.determine_visibility(&modifiers)),
            parent_id.map(|s| s.to_string()),
            metadata,
            doc_comment,
        );

        self.base
            .create_symbol(&node, name, SymbolKind::Type, options)
    }

    /// Helper method to create SymbolOptions with proper serde_json::Value metadata
    pub(super) fn create_symbol_options(
        &self,
        signature: Option<String>,
        visibility: Option<Visibility>,
        parent_id: Option<String>,
        metadata: HashMap<String, String>,
        doc_comment: Option<String>,
    ) -> SymbolOptions {
        let json_metadata: HashMap<String, serde_json::Value> = metadata
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();

        SymbolOptions {
            signature,
            visibility,
            parent_id,
            metadata: Some(json_metadata),
            doc_comment,
        }
    }
}
