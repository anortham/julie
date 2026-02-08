use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

use super::attributes::AttributeHandler;
use super::helpers::HTMLHelpers;
use super::types::HTMLTypes;

/// Check if an HTML element should be extracted as a symbol.
///
/// Returns true for meaningful elements:
/// - Elements with `id` or `name` attributes (referenceable)
/// - Semantic landmark elements (header, nav, main, aside, footer, section, article)
/// - Other semantic elements (details, summary, figure, figcaption, time, dialog)
/// - Form elements (input, textarea, select, button, form, fieldset, legend, label)
/// - Media elements (img, video, audio, picture, source, track)
/// - Meta/link/base elements (metadata)
/// - Heading elements (h1-h6, title)
/// - Interactive/embedding elements (a, canvas, iframe, object, embed, svg)
/// - Custom elements (contain a hyphen in the tag name)
///
/// Generic containers (div, span, p, ul, ol, li, table, etc.) are skipped
/// unless they have an `id` or `name` attribute.
pub(super) fn should_extract_element(tag_name: &str, attributes: &HashMap<String, String>) -> bool {
    // Elements with id or name are always meaningful (referenceable)
    if attributes.contains_key("id") || attributes.contains_key("name") {
        return true;
    }

    // Custom elements (contain a hyphen) are always meaningful
    if tag_name.contains('-') {
        return true;
    }

    matches!(
        tag_name,
        // Document structure
        "html" | "head" | "body"
        // Semantic landmarks
        | "header" | "nav" | "main" | "aside" | "footer" | "section" | "article"
        // Other semantic elements
        | "details" | "summary" | "figure" | "figcaption" | "time" | "dialog"
        // Headings
        | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "title"
        // Form elements
        | "form" | "input" | "textarea" | "select" | "button"
        | "fieldset" | "legend" | "label" | "option"
        // Media elements
        | "img" | "video" | "audio" | "picture" | "source" | "track"
        // Meta/link/base
        | "meta" | "link" | "base"
        // Interactive/embedding elements
        | "a" | "canvas" | "iframe" | "object" | "embed"
        // SVG elements
        | "svg" | "defs" | "linearGradient" | "rect" | "circle"
        | "path" | "text" | "animate" | "desc" | "stop"
    )
}

/// HTML element extraction logic
pub(super) struct ElementExtractor;

impl ElementExtractor {
    /// Extract an HTML element and create a symbol, if it passes filtering.
    ///
    /// Generic container elements (div, span, p, etc.) are skipped unless they
    /// have an `id` or `name` attribute. This reduces noise by 90-95%.
    pub(super) fn extract_element(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Option<Symbol> {
        let tag_name = HTMLHelpers::extract_tag_name(base, node)?;
        let attributes = HTMLHelpers::extract_attributes(base, node);

        // Filter: skip generic containers without id/name
        if !should_extract_element(&tag_name, &attributes) {
            return None;
        }

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

        Some(base.create_symbol(
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
        ))
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
            SymbolKind::Namespace,
            SymbolOptions {
                signature: Some(doctype_text),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
            },
        )
    }
}
