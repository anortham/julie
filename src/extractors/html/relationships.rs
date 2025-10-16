use crate::extractors::base::{BaseExtractor, Relationship, RelationshipKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

use super::helpers::HTMLHelpers;

/// Relationship extraction logic for HTML elements
pub(super) struct RelationshipExtractor;

impl RelationshipExtractor {
    /// Extract relationships from HTML elements
    pub(super) fn extract_element_relationships(
        base: &BaseExtractor,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let attributes = HTMLHelpers::extract_attributes(base, node);

        // Extract href relationships (links)
        if let Some(href) = attributes.get("href") {
            if let Some(element) = Self::find_element_symbol(base, node, symbols) {
                let to_id = format!("url:{}", href);
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        element.id,
                        to_id,
                        RelationshipKind::References,
                        node.start_position().row
                    ),
                    from_symbol_id: element.id.clone(),
                    to_symbol_id: to_id,
                    kind: RelationshipKind::References,
                    file_path: base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: Some({
                        let mut meta = HashMap::new();
                        meta.insert("href".to_string(), serde_json::Value::String(href.clone()));
                        meta
                    }),
                });
            }
        }

        // Extract src relationships (images, scripts, etc.)
        if let Some(src) = attributes.get("src") {
            if let Some(element) = Self::find_element_symbol(base, node, symbols) {
                let to_id = format!("resource:{}", src);
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        element.id,
                        to_id,
                        RelationshipKind::Uses,
                        node.start_position().row
                    ),
                    from_symbol_id: element.id.clone(),
                    to_symbol_id: to_id,
                    kind: RelationshipKind::Uses,
                    file_path: base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: Some({
                        let mut meta = HashMap::new();
                        meta.insert("src".to_string(), serde_json::Value::String(src.clone()));
                        meta
                    }),
                });
            }
        }

        // Extract form relationships
        if let Some(action) = attributes.get("action") {
            if let Some(element) = Self::find_element_symbol(base, node, symbols) {
                let to_id = format!("endpoint:{}", action);
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        element.id,
                        to_id,
                        RelationshipKind::Calls,
                        node.start_position().row
                    ),
                    from_symbol_id: element.id.clone(),
                    to_symbol_id: to_id,
                    kind: RelationshipKind::Calls,
                    file_path: base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: Some({
                        let mut meta = HashMap::new();
                        meta.insert(
                            "action".to_string(),
                            serde_json::Value::String(action.clone()),
                        );
                        meta.insert(
                            "method".to_string(),
                            serde_json::Value::String(
                                attributes
                                    .get("method")
                                    .cloned()
                                    .unwrap_or_else(|| "GET".to_string()),
                            ),
                        );
                        meta
                    }),
                });
            }
        }
    }

    /// Extract relationships from script elements
    pub(super) fn extract_script_relationships(
        base: &BaseExtractor,
        node: Node,
        symbols: &[Symbol],
        relationships: &mut Vec<Relationship>,
    ) {
        let attributes = HTMLHelpers::extract_attributes(base, node);

        if let Some(src) = attributes.get("src") {
            if let Some(script_symbol) = symbols.iter().find(|s| {
                s.metadata
                    .as_ref()
                    .and_then(|m| m.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|t| t == "script-element")
                    .unwrap_or(false)
            }) {
                let to_id = format!("script:{}", src);
                relationships.push(Relationship {
                    id: format!(
                        "{}_{}_{:?}_{}",
                        script_symbol.id,
                        to_id,
                        RelationshipKind::Imports,
                        node.start_position().row
                    ),
                    from_symbol_id: script_symbol.id.clone(),
                    to_symbol_id: to_id,
                    kind: RelationshipKind::Imports,
                    file_path: base.file_path.clone(),
                    line_number: (node.start_position().row + 1) as u32,
                    confidence: 1.0,
                    metadata: Some({
                        let mut meta = HashMap::new();
                        meta.insert("src".to_string(), serde_json::Value::String(src.clone()));
                        if let Some(script_type) = attributes.get("type") {
                            meta.insert(
                                "type".to_string(),
                                serde_json::Value::String(script_type.clone()),
                            );
                        }
                        meta
                    }),
                });
            }
        }
    }

    /// Find the symbol matching a node
    pub(super) fn find_element_symbol<'a>(
        base: &BaseExtractor,
        node: Node,
        symbols: &'a [Symbol],
    ) -> Option<&'a Symbol> {
        let tag_name = HTMLHelpers::extract_tag_name(base, node);
        let target_line = (node.start_position().row + 1) as u32;

        symbols.iter().find(|s| {
            s.name == tag_name
                && s.file_path == base.file_path
                && s.start_line.abs_diff(target_line) < 2
        })
    }
}
