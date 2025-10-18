use crate::extractors::base::SymbolKind;
use std::collections::HashMap;

/// Type inference and semantic classification for HTML elements
pub(super) struct HTMLTypes;

impl HTMLTypes {
    /// Determine symbol kind for an HTML element
    pub(super) fn get_symbol_kind_for_element(
        tag_name: &str,
        attributes: &HashMap<String, String>,
    ) -> SymbolKind {
        match tag_name {
            // Meta elements are properties
            "meta" => SymbolKind::Property,

            // Link elements with stylesheet are imports
            "link"
                if attributes
                    .get("rel")
                    .map(|v| v == "stylesheet")
                    .unwrap_or(false) =>
            {
                SymbolKind::Import
            }

            // Form input elements are fields
            _ if Self::is_form_field(tag_name) => SymbolKind::Field,

            // Media elements are variables
            _ if matches!(
                tag_name,
                "img" | "video" | "audio" | "picture" | "source" | "track"
            ) =>
            {
                SymbolKind::Variable
            }

            // All other HTML elements are classes
            _ => SymbolKind::Class,
        }
    }

    /// Check if a tag is a form-related field
    pub(super) fn is_form_field(tag_name: &str) -> bool {
        matches!(
            tag_name,
            "input" | "textarea" | "select" | "button" | "fieldset" | "legend" | "label"
        )
    }

    /// Check if a tag is a void (self-closing) element
    pub(super) fn is_void_element(tag_name: &str) -> bool {
        matches!(
            tag_name,
            "area"
                | "base"
                | "br"
                | "col"
                | "embed"
                | "hr"
                | "img"
                | "input"
                | "link"
                | "meta"
                | "param"
                | "source"
                | "track"
                | "wbr"
        )
    }

    /// Check if a tag is a semantic HTML element
    pub(super) fn is_semantic_element(tag_name: &str) -> bool {
        matches!(
            tag_name,
            "article"
                | "aside"
                | "details"
                | "figcaption"
                | "figure"
                | "footer"
                | "header"
                | "main"
                | "nav"
                | "section"
                | "summary"
                | "time"
        )
    }

    /// Infer type information from symbol metadata
    pub(super) fn infer_types(
        symbols: &[crate::extractors::base::Symbol],
    ) -> HashMap<String, String> {
        let mut types = HashMap::new();

        for symbol in symbols {
            let metadata = &symbol.metadata;
            if let Some(symbol_type) = metadata
                .as_ref()
                .and_then(|m| m.get("type"))
                .and_then(|v| v.as_str())
            {
                types.insert(symbol.id.clone(), symbol_type.to_string());
            } else if let Some(tag_name) = metadata
                .as_ref()
                .and_then(|m| m.get("tagName"))
                .and_then(|v| v.as_str())
            {
                types.insert(symbol.id.clone(), format!("html:{}", tag_name));
            }
        }

        types
    }
}
