use super::functions;
use super::tables;
use super::variables;
/// Core symbol extraction and tree traversal
///
/// Handles the main tree traversal logic and dispatches to appropriate
/// extraction functions based on node types.
use crate::base::{
    BaseExtractor, RelationshipKind, Symbol, SymbolKind, SymbolOptions, UnresolvedTarget,
    Visibility,
};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

/// Recursively traverse the tree and extract symbols
pub(super) fn traverse_tree(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<String>,
) {
    let mut symbol: Option<Symbol> = None;

    match node.kind() {
        "function_call" => {
            symbol = extract_bare_require_import(symbols, base, node, parent_id.as_deref());
        }
        "function_definition_statement" | "function_declaration" => {
            symbol = functions::extract_function_definition_statement(
                symbols,
                base,
                node,
                parent_id.as_deref(),
            );
        }
        "local_function_definition_statement" | "local_function_declaration" => {
            symbol = functions::extract_local_function_definition_statement(
                symbols,
                base,
                node,
                parent_id.as_deref(),
            );
        }
        "local_variable_declaration" | "variable_declaration" => {
            symbol = variables::extract_local_variable_declaration(
                symbols,
                base,
                node,
                parent_id.as_deref(),
            );
        }
        "assignment_statement" => {
            symbol =
                variables::extract_assignment_statement(symbols, base, node, parent_id.as_deref());
        }
        "variable_assignment" => {
            symbol =
                variables::extract_variable_assignment(symbols, base, node, parent_id.as_deref());
        }
        "table_constructor" | "table" => {
            // Table constructors can contain fields that should be extracted as child symbols
            tables::extract_table_fields(symbols, base, node, parent_id.as_deref());
            return; // Table constructor itself doesn't create a symbol, just its fields
        }
        _ => {}
    }

    // Traverse children with current symbol as parent (if extracted) or keep same parent
    let current_parent_id = symbol.as_ref().map(|s| s.id.clone()).or(parent_id);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_tree(symbols, base, child, current_parent_id.clone());
    }
}

fn extract_bare_require_import(
    symbols: &mut Vec<Symbol>,
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if !is_bare_require_statement(base, node) {
        return None;
    }

    let require_call = parse_require_call(base, node)?;
    let import_context = base.get_node_text(&node);
    let module_path = require_call.module_path.clone();
    let terminal_name = require_call.terminal_name.clone();

    let mut metadata = HashMap::new();
    metadata.insert("source".to_string(), Value::String(module_path.clone()));
    metadata.insert(
        "importContext".to_string(),
        Value::String(import_context.clone()),
    );

    let options = SymbolOptions {
        signature: Some(format!("require({:?})", module_path)),
        visibility: Some(Visibility::Public),
        parent_id: parent_id.map(|s| s.to_string()),
        metadata: Some(metadata),
        ..Default::default()
    };

    let symbol = base.create_symbol(&node, terminal_name, SymbolKind::Import, options);

    let pending = base.create_pending_relationship(
        symbol.id.clone(),
        UnresolvedTarget {
            display_name: module_path,
            terminal_name: require_call.terminal_name,
            receiver: None,
            namespace_path: Vec::new(),
            import_context: Some(import_context),
        },
        RelationshipKind::Imports,
        &node,
        parent_id.map(|s| s.to_string()),
        Some(0.7),
    );
    base.add_structured_pending_relationship(pending);

    symbols.push(symbol.clone());
    Some(symbol)
}

fn is_bare_require_statement(base: &BaseExtractor, node: Node) -> bool {
    let Some(name_node) = node.child_by_field_name("name") else {
        return false;
    };

    if base.get_node_text(&name_node) != "require" {
        return false;
    }

    !node
        .parent()
        .map(|parent| matches!(parent.kind(), "expression_list" | "arguments"))
        .unwrap_or(false)
}

struct RequireCall {
    module_path: String,
    terminal_name: String,
}

fn parse_require_call(base: &BaseExtractor, node: Node) -> Option<RequireCall> {
    let arguments = node.child_by_field_name("arguments")?;
    let module_path = extract_first_string_argument(base, arguments)?;
    let terminal_name = terminal_require_name(&module_path);

    if terminal_name.is_empty() {
        return None;
    }

    Some(RequireCall {
        module_path,
        terminal_name,
    })
}

fn extract_first_string_argument(base: &BaseExtractor, arguments: Node) -> Option<String> {
    let mut cursor = arguments.walk();
    let string_node = arguments
        .children(&mut cursor)
        .find(|child| child.kind() == "string")?;
    normalize_lua_string_literal(&base.get_node_text(&string_node))
}

fn normalize_lua_string_literal(raw: &str) -> Option<String> {
    let trimmed = raw.trim();

    if let Some(stripped) = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return Some(stripped.to_string());
    }

    if let Some(stripped) = trimmed
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        return Some(stripped.to_string());
    }

    if let Some(stripped) = trimmed
        .strip_prefix("[[")
        .and_then(|value| value.strip_suffix("]]"))
    {
        return Some(stripped.to_string());
    }

    None
}

fn terminal_require_name(module_path: &str) -> String {
    module_path
        .split(|ch| ch == '/' || ch == '.')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .last()
        .unwrap_or(module_path)
        .to_string()
}
