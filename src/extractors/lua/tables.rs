use super::helpers;
/// Table field extraction and handling
///
/// Handles extraction of table fields from table constructors:
/// - Field assignments: `field = value`
/// - Method definitions: `method = function() end`
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract fields from a table constructor
///
/// Processes all field nodes within a table and creates symbols for them.
pub(super) fn extract_table_fields(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) {
    // Extract fields from table constructor: { field = value, method = function() end }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "field" {
            if let Some(field_symbol) = extract_table_field_symbol(symbols, base, child, parent_id)
            {
                symbols.push(field_symbol);
            }
        }
    }
}

/// Extract a single table field and create a symbol for it
///
/// Handles field definitions like:
/// - `field = value`
/// - `field = function() end`
fn extract_table_field_symbol(
    _symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Handle field definitions like: field = value or field = function() end
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    if children.len() < 3 {
        return None;
    }
    let name_node = children[0]; // field name
    let equal_node = children[1]; // '=' operator
    let value_node = children[2]; // field value
    if equal_node.kind() != "=" || name_node.kind() != "identifier" {
        return None;
    }
    let name = base.get_node_text(&name_node);
    let signature = base.get_node_text(&node);
    // Determine if this is a method (function) or field (value)
    let mut kind = SymbolKind::Field;
    let data_type = if value_node.kind() == "function_definition" {
        kind = SymbolKind::Method;
        "function".to_string()
    } else {
        helpers::infer_type_from_expression(base, value_node)
    };
    let mut metadata = HashMap::new();
    metadata.insert("dataType".to_string(), data_type.clone().into());
    let options = SymbolOptions {
        signature: Some(signature),
        parent_id: parent_id.map(|s| s.to_string()),
        visibility: Some(Visibility::Public),
        metadata: Some(metadata),
        ..Default::default()
    };

    Some(base.create_symbol(&name_node, name, kind, options))
}
