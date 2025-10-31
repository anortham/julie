use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions};
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts Swift type declarations: classes, structs, protocols, and enums
impl SwiftExtractor {
    /// Implementation of extractClass method with full Swift class support
    pub(super) fn extract_class(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        // Swift parser uses class_declaration for classes
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier" || c.kind() == "user_type");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownClass".to_string());

        // Check what type this actually is
        let is_enum = node.children(&mut node.walk()).any(|c| c.kind() == "enum");
        let is_struct = node
            .children(&mut node.walk())
            .any(|c| c.kind() == "struct");
        let is_extension = node
            .children(&mut node.walk())
            .any(|c| c.kind() == "extension");

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);
        let inheritance = self.extract_inheritance(node);

        // Determine the correct keyword and symbol kind
        let (keyword, symbol_kind) = if is_enum {
            // Check for indirect modifier for enums
            let is_indirect = node
                .children(&mut node.walk())
                .any(|c| c.kind() == "indirect");
            if is_indirect {
                ("indirect enum", SymbolKind::Enum)
            } else {
                ("enum", SymbolKind::Enum)
            }
        } else if is_struct {
            ("struct", SymbolKind::Struct)
        } else if is_extension {
            ("extension", SymbolKind::Class)
        } else {
            ("class", SymbolKind::Class)
        };

        let mut signature = format!("{} {}", keyword, name);

        // For enums with indirect modifier, don't add modifiers again
        let is_enum_with_indirect = is_enum && keyword.contains("indirect");
        if !modifiers.is_empty() && !is_enum_with_indirect {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        if let Some(ref inheritance) = inheritance {
            signature.push_str(&format!(": {}", inheritance));
        }

        // Add where clause if present
        if let Some(where_clause) = self.extract_where_clause(node) {
            signature.push_str(&format!(" {}", where_clause));
        }

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("class".to_string()),
            ),
            (
                "modifiers".to_string(),
                serde_json::Value::String(modifiers.join(", ")),
            ),
        ]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Implementation of extractStruct method
    pub(super) fn extract_struct(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownStruct".to_string());

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);
        let conformance = self.extract_inheritance(node);

        let mut signature = format!("struct {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        if let Some(ref conformance) = conformance {
            signature.push_str(&format!(": {}", conformance));
        }

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("struct".to_string()),
            ),
            (
                "modifiers".to_string(),
                serde_json::Value::String(modifiers.join(", ")),
            ),
        ]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Struct,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Implementation of extractProtocol method
    pub(super) fn extract_protocol(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownProtocol".to_string());

        let modifiers = self.extract_modifiers(node);
        let inheritance = self.extract_inheritance(node);

        let mut signature = format!("protocol {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref inheritance) = inheritance {
            signature.push_str(&format!(": {}", inheritance));
        }

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("protocol".to_string()),
            ),
            (
                "modifiers".to_string(),
                serde_json::Value::String(modifiers.join(", ")),
            ),
        ]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Interface,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Implementation of extractEnum method
    pub(super) fn extract_enum(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "type_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "UnknownEnum".to_string());

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);
        let inheritance = self.extract_inheritance(node);

        let mut signature = format!("enum {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        if let Some(ref inheritance) = inheritance {
            signature.push_str(&format!(": {}", inheritance));
        }

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("enum".to_string()),
            ),
            (
                "modifiers".to_string(),
                serde_json::Value::String(modifiers.join(", ")),
            ),
        ]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Enum,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(self.determine_visibility(&modifiers)),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }
}
