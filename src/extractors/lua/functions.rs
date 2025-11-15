use super::helpers;
/// Function and method definition extraction
///
/// Handles extraction of:
/// - Regular functions: `function name() end`
/// - Local functions: `local function name() end`
/// - Methods with colon syntax: `function obj:method() end`
/// - Methods with dot syntax: `function obj.method() end`
use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use tree_sitter::Node;

/// Extract regular function definition statement
/// Handles both `function name()` and method definitions
pub(super) fn extract_function_definition_statement(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    // Handle both regular functions and colon syntax methods
    let mut name_node = helpers::find_child_by_type(node, "identifier");
    let name: String;
    let mut kind = SymbolKind::Function;
    let mut method_parent_id = None;

    if let Some(name_n) = name_node {
        name = base.get_node_text(&name_n);
    } else {
        // Check for colon syntax: function obj:method() or dot syntax: function obj.method()
        if let Some(variable_node) = helpers::find_child_by_type(node, "variable")
            .or_else(|| helpers::find_child_by_type(node, "dot_index_expression"))
            .or_else(|| helpers::find_child_by_type(node, "method_index_expression"))
        {
            let full_name = base.get_node_text(&variable_node);

            // Handle colon syntax: function obj:method()
            if full_name.contains(':') {
                let parts: Vec<&str> = full_name.split(':').collect();
                if parts.len() == 2 {
                    let object_name = parts[0];
                    let method_name = parts[1];
                    name = method_name.to_string();
                    name_node = Some(variable_node);
                    kind = SymbolKind::Method;

                    // Try to find the object this method belongs to
                    if let Some(object_symbol) = symbols.iter().find(|s| s.name == object_name) {
                        method_parent_id = Some(object_symbol.id.clone());
                    }
                } else {
                    return None;
                }
            }
            // Handle dot syntax: function obj.method()
            else if full_name.contains('.') {
                let parts: Vec<&str> = full_name.split('.').collect();
                if parts.len() == 2 {
                    let object_name = parts[0];
                    let method_name = parts[1];
                    name = method_name.to_string();
                    name_node = Some(variable_node);
                    kind = SymbolKind::Method;

                    // Try to find the object this method belongs to
                    if let Some(object_symbol) = symbols.iter().find(|s| s.name == object_name) {
                        method_parent_id = Some(object_symbol.id.clone());
                    }
                } else {
                    return None;
                }
            } else {
                return None;
            }
        } else {
            return None;
        }
    }

    let signature = base.get_node_text(&node);

    // Determine visibility: check if function is local (contains "local" keyword) or uses underscore prefix
    let node_text = base.get_node_text(&node);
    let is_local = node_text.trim_start().starts_with("local function");
    let has_underscore = name.starts_with('_');
    let visibility = if is_local || has_underscore {
        Visibility::Private
    } else {
        Visibility::Public
    };

    // Extract LuaDoc comment
    let doc_comment = base.find_doc_comment(&node);

    let options = SymbolOptions {
        signature: Some(signature),
        parent_id: method_parent_id.or_else(|| parent_id.map(|s| s.to_string())),
        visibility: Some(visibility),
        doc_comment,
        ..Default::default()
    };

    let symbol = base.create_symbol(&name_node.unwrap_or(node), name, kind, options);
    symbols.push(symbol.clone());
    Some(symbol)
}

/// Extract local function definition statement
/// Local functions are always private
pub(super) fn extract_local_function_definition_statement(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    let name_node = helpers::find_child_by_type(node, "identifier")?;
    let name = base.get_node_text(&name_node);
    let signature = base.get_node_text(&node);

    // Extract LuaDoc comment
    let doc_comment = base.find_doc_comment(&node);

    // Local functions are always private (regardless of underscore prefix)
    let options = SymbolOptions {
        signature: Some(signature),
        parent_id: parent_id.map(|s| s.to_string()),
        visibility: Some(Visibility::Private),
        doc_comment,
        ..Default::default()
    };

    let symbol = base.create_symbol(&name_node, name, SymbolKind::Function, options);
    symbols.push(symbol.clone());
    Some(symbol)
}
