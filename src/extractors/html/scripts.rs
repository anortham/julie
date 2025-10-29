use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

use super::attributes::AttributeHandler;
use super::helpers::HTMLHelpers;

/// Script and style tag extraction
pub(super) struct ScriptStyleExtractor;

impl ScriptStyleExtractor {
    /// Extract a script element and create a symbol
    pub(super) fn extract_script_element(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Symbol {
        let attributes = HTMLHelpers::extract_attributes(base, node);
        let content = HTMLHelpers::extract_text_content(base, node);
        let signature =
            AttributeHandler::build_element_signature("script", &attributes, content.as_deref());

        // Determine symbol kind based on src attribute
        let symbol_kind = if attributes.contains_key("src") {
            SymbolKind::Import
        } else {
            SymbolKind::Variable
        };

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("script-element".to_string()),
        );
        metadata.insert(
            "isInline".to_string(),
            serde_json::Value::Bool(!attributes.contains_key("src")),
        );

        if !attributes.is_empty() {
            metadata.insert(
                "attributes".to_string(),
                serde_json::to_value(&attributes).unwrap_or_default(),
            );
        }

        let script_type = attributes
            .get("type")
            .cloned()
            .unwrap_or_else(|| "text/javascript".to_string());
        metadata.insert(
            "scriptType".to_string(),
            serde_json::Value::String(script_type),
        );

        if let Some(content) = content {
            // Safely truncate UTF-8 string at character boundary
            let truncated_content = BaseExtractor::truncate_string(&content, 100);
            metadata.insert(
                "content".to_string(),
                serde_json::Value::String(truncated_content),
            );
        }

        // Extract HTML comment
        let doc_comment = base.find_doc_comment(&node);

        base.create_symbol(
            &node,
            "script".to_string(),
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Extract a style element and create a symbol
    pub(super) fn extract_style_element(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Symbol {
        let attributes = HTMLHelpers::extract_attributes(base, node);
        let content = HTMLHelpers::extract_text_content(base, node);
        let signature =
            AttributeHandler::build_element_signature("style", &attributes, content.as_deref());

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("style-element".to_string()),
        );
        metadata.insert("isInline".to_string(), serde_json::Value::Bool(true));

        if !attributes.is_empty() {
            metadata.insert(
                "attributes".to_string(),
                serde_json::to_value(&attributes).unwrap_or_default(),
            );
        }

        if let Some(content) = content {
            // Safely truncate UTF-8 string at character boundary
            let truncated_content = BaseExtractor::truncate_string(&content, 100);
            metadata.insert(
                "content".to_string(),
                serde_json::Value::String(truncated_content),
            );
        }

        // Extract HTML comment
        let doc_comment = base.find_doc_comment(&node);

        base.create_symbol(
            &node,
            "style".to_string(),
            SymbolKind::Variable,
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
