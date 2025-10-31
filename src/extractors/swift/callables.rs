use crate::extractors::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json;
use std::collections::HashMap;
use tree_sitter::Node;

use super::SwiftExtractor;

/// Extracts Swift callable members: functions, methods, initializers, and deinitializers
impl SwiftExtractor {
    /// Implementation of extractFunction method
    pub(super) fn extract_function(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name_node = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "simple_identifier");
        let name = name_node
            .map(|n| self.base.get_node_text(&n))
            .unwrap_or_else(|| "unknownFunction".to_string());

        let modifiers = self.extract_modifiers(node);
        let generic_params = self.extract_generic_parameters(node);
        let parameters = self.extract_parameters(node);
        let return_type = self.extract_return_type(node);

        let mut signature = format!("func {}", name);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        if let Some(ref generic_params) = generic_params {
            signature.push_str(generic_params);
        }

        let params_str = parameters.unwrap_or_else(|| "()".to_string());
        let return_str = return_type.unwrap_or_else(|| "Void".to_string());

        signature.push_str(&params_str);

        if !return_str.is_empty() && return_str != "Void" {
            signature.push_str(&format!(" -> {}", return_str));
        }

        // Functions inside classes/structs are methods
        let symbol_kind = if parent_id.is_some() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("function".to_string()),
            ),
            (
                "modifiers".to_string(),
                serde_json::Value::String(modifiers.join(", ")),
            ),
            (
                "parameters".to_string(),
                serde_json::Value::String(params_str),
            ),
            (
                "returnType".to_string(),
                serde_json::Value::String(return_str),
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

    /// Implementation of extractInitializer method
    pub(super) fn extract_initializer(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = "init".to_string();
        let modifiers = self.extract_modifiers(node);
        let parameters = self.extract_initializer_parameters(node);

        let params_str = parameters.unwrap_or_else(|| "()".to_string());
        let mut signature = format!("init{}", params_str);

        if !modifiers.is_empty() {
            signature = format!("{} {}", modifiers.join(" "), signature);
        }

        let metadata = HashMap::from([
            (
                "type".to_string(),
                serde_json::Value::String("initializer".to_string()),
            ),
            (
                "parameters".to_string(),
                serde_json::Value::String(params_str),
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
            SymbolKind::Constructor,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Implementation of extractDeinitializer method
    pub(super) fn extract_deinitializer(&mut self, node: Node, parent_id: Option<&str>) -> Symbol {
        let name = "deinit".to_string();
        let signature = "deinit".to_string();

        let metadata = HashMap::from([(
            "type".to_string(),
            serde_json::Value::String("deinitializer".to_string()),
        )]);

        // Extract Swift documentation comment
        let doc_comment = self.base.find_doc_comment(&node);

        self.base.create_symbol(
            &node,
            name,
            SymbolKind::Destructor,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }
}
