use super::ElixirExtractor;
use super::helpers;
use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Extract an ExUnit `test "description" do ... end` block as a Function symbol.
pub(super) fn extract_test(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let description = helpers::extract_first_string_arg(&extractor.base, node)?;
    let signature = format!("test \"{}\"", description);

    let mut metadata = HashMap::new();
    metadata.insert("is_test".to_string(), Value::Bool(true));

    let symbol = extractor.base.create_symbol(
        node,
        description,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Private),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: None,
            annotations: Vec::new(),
        },
    );

    Some((symbol, false))
}

/// Extract an ExUnit `describe "context" do ... end` block as a Namespace symbol.
/// Traverses child nodes to extract nested test definitions.
pub(super) fn extract_describe(
    extractor: &mut ElixirExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let description = helpers::extract_first_string_arg(&extractor.base, node)?;
    let signature = format!("describe \"{}\"", description);

    let mut metadata = HashMap::new();
    metadata.insert("test_container".to_string(), Value::Bool(true));

    let symbol = extractor.base.create_symbol(
        node,
        description,
        SymbolKind::Namespace,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Private),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: None,
            annotations: Vec::new(),
        },
    );

    let sym_id = symbol.id.clone();
    if let Some(do_block) = helpers::extract_do_block(node) {
        extractor.traverse_children(&do_block, symbols, Some(&sym_id));
    }

    Some((symbol, true))
}

/// Extract an ExUnit `setup` / `setup_all` lifecycle hook as a Function symbol.
pub(super) fn extract_setup(
    extractor: &mut ElixirExtractor,
    node: &Node,
    name: &str,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let signature = format!("{}()", name);

    let mut metadata = HashMap::new();
    metadata.insert("is_test".to_string(), Value::Bool(true));
    metadata.insert("test_lifecycle".to_string(), Value::Bool(true));

    let symbol = extractor.base.create_symbol(
        node,
        name.to_string(),
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Private),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: None,
            annotations: Vec::new(),
        },
    );

    Some((symbol, false))
}
