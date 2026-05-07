use crate::base::{
    BaseExtractor, EmbeddedSpanOffset, NormalizedSpan, Symbol, SymbolKind, SymbolOptions,
    Visibility,
};
use crate::css::CSSExtractor;
use crate::javascript::JavaScriptExtractor;
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
    ) -> Vec<Symbol> {
        let attributes = HTMLHelpers::extract_attributes(base, node);
        let content = HTMLHelpers::extract_text_content(base, node);

        if !attributes.contains_key("src") {
            let symbols = content
                .as_deref()
                .map(|content| extract_embedded_javascript_symbols(base, node, content))
                .unwrap_or_default();
            if !symbols.is_empty() {
                return symbols;
            }
        }

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

        vec![base.create_symbol(
            &node,
            "script".to_string(),
            symbol_kind,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
                annotations: Vec::new(),
            },
        )]
    }

    /// Extract a style element and create a symbol
    pub(super) fn extract_style_element(
        base: &mut BaseExtractor,
        node: Node,
        parent_id: Option<&str>,
    ) -> Vec<Symbol> {
        let attributes = HTMLHelpers::extract_attributes(base, node);
        let content = HTMLHelpers::extract_text_content(base, node);

        let symbols = content
            .as_deref()
            .map(|content| extract_embedded_css_symbols(base, node, content))
            .unwrap_or_default();
        if !symbols.is_empty() {
            return symbols;
        }

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

        vec![base.create_symbol(
            &node,
            "style".to_string(),
            SymbolKind::Variable,
            SymbolOptions {
                signature: Some(signature),
                visibility: Some(Visibility::Public),
                parent_id: parent_id.map(|s| s.to_string()),
                metadata: Some(metadata),
                doc_comment,
                annotations: Vec::new(),
            },
        )]
    }
}

fn extract_embedded_javascript_symbols(
    base: &BaseExtractor,
    node: Node,
    content: &str,
) -> Vec<Symbol> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let mut extractor = JavaScriptExtractor::new(
        "javascript".to_string(),
        base.file_path.clone(),
        content.to_string(),
        std::path::Path::new(""),
    );
    let mut symbols = extractor.extract_symbols(&tree);
    let Some(offset) = embedded_content_offset(base, node, content) else {
        return Vec::new();
    };
    for symbol in &mut symbols {
        apply_embedded_offset(symbol, base, offset);
    }
    symbols
}

fn extract_embedded_css_symbols(base: &BaseExtractor, node: Node, content: &str) -> Vec<Symbol> {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let mut extractor = CSSExtractor::new(
        "css".to_string(),
        base.file_path.clone(),
        content.to_string(),
        std::path::Path::new(""),
    );
    let mut symbols = extractor.extract_symbols(&tree);
    let Some(offset) = embedded_content_offset(base, node, content) else {
        return Vec::new();
    };
    for symbol in &mut symbols {
        apply_embedded_offset(symbol, base, offset);
    }
    symbols
}

fn embedded_content_offset(base: &BaseExtractor, node: Node, content: &str) -> Option<u32> {
    let node_text = base.content.get(node.start_byte()..node.end_byte())?;
    let local_offset = node_text.find(content)?;
    Some((node.start_byte() + local_offset) as u32)
}

fn apply_embedded_offset(symbol: &mut Symbol, base: &BaseExtractor, byte_offset: u32) {
    let Some(offset) = EmbeddedSpanOffset::from_host_byte(&base.content, byte_offset as usize)
    else {
        return;
    };
    let span = NormalizedSpan {
        start_line: symbol.start_line,
        start_column: symbol.start_column,
        end_line: symbol.end_line,
        end_column: symbol.end_column,
        start_byte: symbol.start_byte,
        end_byte: symbol.end_byte,
    };
    symbol.file_path = base.file_path.clone();
    symbol.apply_normalized_span(offset.apply(span));
}
