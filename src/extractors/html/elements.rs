use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

use super::attributes::AttributeHandler;
use super::helpers::HTMLHelpers;
use super::types::HTMLTypes;

/// HTML element extraction logic
pub(super) struct ElementExtractor;

impl ElementExtractor {
    /// Extract an HTML element and create a symbol
    pub(super) fn extract_element(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Symbol {
        let tag_name = HTMLHelpers::extract_tag_name(base, node);
        let attributes = HTMLHelpers::extract_attributes(base, node);
        let text_content = HTMLHelpers::extract_element_text_content(base, node);
        let signature = AttributeHandler::build_element_signature(
            &tag_name,
            &attributes,
            text_content.as_deref(),
        );

        // Determine symbol kind based on element type
        let symbol_kind = HTMLTypes::get_symbol_kind_for_element(&tag_name, &attributes);

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("html-element".to_string()),
        );
        metadata.insert(
            "tagName".to_string(),
            serde_json::Value::String(tag_name.clone()),
        );
        metadata.insert(
            "isVoid".to_string(),
            serde_json::Value::Bool(HTMLTypes::is_void_element(&tag_name)),
        );
        metadata.insert(
            "isSemantic".to_string(),
            serde_json::Value::Bool(HTMLTypes::is_semantic_element(&tag_name)),
        );

        if !attributes.is_empty() {
            metadata.insert(
                "attributes".to_string(),
                serde_json::to_value(&attributes).unwrap_or_default(),
            );
        }

        if let Some(content) = text_content {
            metadata.insert(
                "textContent".to_string(),
                serde_json::Value::String(content),
            );
        }

        // Extract HTML comment
        let doc_comment = base.find_doc_comment(&node);

        base.create_symbol(
            &node,
            tag_name,
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

    /// Extract DOCTYPE declaration
    pub(super) fn extract_doctype(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Symbol {
        let doctype_text = base.get_node_text(&node);

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("doctype".to_string()),
        );
        metadata.insert(
            "declaration".to_string(),
            serde_json::Value::String(doctype_text.clone()),
        );

        // Extract HTML comment
        let doc_comment = base.find_doc_comment(&node);

        base.create_symbol(
            &node,
            "DOCTYPE".to_string(),
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(doctype_text),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }

    /// Extract HTML comment
    pub(super) fn extract_comment(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let comment_text = base.get_node_text(&node);
        let clean_comment = comment_text
            .replace("<!--", "")
            .replace("-->", "")
            .trim()
            .to_string();

        // Only extract meaningful comments (not empty or very short)
        if clean_comment.len() < 3 {
            return None;
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::Value::String("comment".to_string()),
        );
        metadata.insert(
            "content".to_string(),
            serde_json::Value::String(clean_comment.clone()),
        );

        // Extract HTML comment (comments typically don't have preceding comments themselves)
        let doc_comment = base.find_doc_comment(&node);

        Some(base.create_symbol(
            &node,
            "comment".to_string(),
            SymbolKind::Property,
            SymbolOptions {
                signature: Some(format!("<!-- {} -->", clean_comment)),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        ))
    }
}
