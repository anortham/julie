use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use regex::Regex;
use std::collections::HashMap;
use tree_sitter::Node;

use super::attributes::AttributeHandler;
use super::types::HTMLTypes;

/// Fallback extraction when normal parsing fails
pub(super) struct FallbackExtractor;

impl FallbackExtractor {
    /// Extract basic structure when normal parsing fails
    pub(super) fn extract_basic_structure(
        base: &mut BaseExtractor,
        tree: &tree_sitter::Tree,
    ) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let content = base.get_node_text(&tree.root_node());

        // Extract DOCTYPE if present
        if let Some(doctype_match) = Self::find_doctype(&content) {
            let mut metadata = HashMap::new();
            metadata.insert(
                "type".to_string(),
                serde_json::Value::String("doctype".to_string()),
            );
            metadata.insert(
                "declaration".to_string(),
                serde_json::Value::String(doctype_match.clone()),
            );

            // Extract HTML comment (if any)
            let doc_comment = base.find_doc_comment(&tree.root_node());

            let symbol = base.create_symbol(
                &tree.root_node(),
                "DOCTYPE".to_string(),
                SymbolKind::Variable,
                SymbolOptions {
                    signature: Some(doctype_match.clone()),
                    visibility: Some(Visibility::Public),
                    parent_id: None,
                    metadata: Some(metadata),
                    doc_comment,
                },
            );
            symbols.push(symbol);
        }

        // Extract elements using regex as fallback
        symbols.extend(Self::extract_elements_with_regex(
            base,
            &content,
            &tree.root_node(),
        ));

        symbols
    }

    /// Find DOCTYPE declaration in content
    fn find_doctype(content: &str) -> Option<String> {
        if let Some(start) = content.find("<!DOCTYPE") {
            if let Some(end) = content[start..].find('>') {
                let total_len = start + end + 1;
                // SAFETY: Check char boundary before slicing to prevent UTF-8 panic
                if content.is_char_boundary(total_len) {
                    return Some(content[start..total_len].to_string());
                } else {
                    // Fallback: return complete DOCTYPE up to found end, or full content if boundary check fails
                    return Some(content.to_string());
                }
            }
        }
        None
    }

    /// Extract elements using regex pattern matching
    fn extract_elements_with_regex(
        base: &mut BaseExtractor,
        content: &str,
        root_node: &Node,
    ) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        // Enhanced regex for HTML elements - handles both self-closing and container elements
        // Note: Rust regex doesn't support backreferences, so we match any closing tag
        let re =
            Regex::new(r#"<([a-zA-Z][a-zA-Z0-9\-]*)(?:\s+([^>]*?))?\s*(?:/>|>(.*?)</[^>]+>|>)"#)
                .unwrap();

        for captures in re.captures_iter(content) {
            if let Some(tag_name_match) = captures.get(1) {
                let tag_name = tag_name_match.as_str().to_string();
                let attributes_text = captures.get(2).map(|m| m.as_str()).unwrap_or("");
                let text_content = captures.get(3).map(|m| m.as_str());

                // Parse attributes
                let attributes = AttributeHandler::parse_attributes_from_text(attributes_text);

                // Build signature
                let signature =
                    AttributeHandler::build_element_signature(&tag_name, &attributes, text_content);

                // Determine symbol kind
                let symbol_kind = HTMLTypes::get_symbol_kind_for_element(&tag_name, &attributes);

                // Create metadata
                let mut metadata = HashMap::new();
                metadata.insert(
                    "type".to_string(),
                    serde_json::Value::String("html-element-fallback".to_string()),
                );
                metadata.insert(
                    "tagName".to_string(),
                    serde_json::Value::String(tag_name.clone()),
                );
                metadata.insert("isFallback".to_string(), serde_json::Value::Bool(true));

                if !attributes.is_empty() {
                    metadata.insert(
                        "attributes".to_string(),
                        serde_json::to_value(&attributes).unwrap_or_default(),
                    );
                }

                if let Some(content) = text_content {
                    if !content.trim().is_empty() {
                        metadata.insert(
                            "textContent".to_string(),
                            serde_json::Value::String(content.trim().to_string()),
                        );
                    }
                }

                // Extract HTML comment (if any)
                let doc_comment = base.find_doc_comment(root_node);

                let symbol = base.create_symbol(
                    root_node,
                    tag_name,
                    symbol_kind,
                    SymbolOptions {
                        signature: Some(signature),
                        visibility: Some(Visibility::Public),
                        parent_id: None,
                        metadata: Some(metadata),
                        doc_comment,
                    },
                );
                symbols.push(symbol);
            }
        }

        symbols
    }
}
