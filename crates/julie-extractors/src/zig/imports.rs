use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions, Visibility};
use regex::Regex;
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) fn extract_import_variable(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    is_public: bool,
    node_text: &str,
) -> Option<Symbol> {
    let name = base
        .find_child_by_type(&node, "identifier")
        .map(|name_node| base.get_node_text(&name_node))
        .or_else(|| extract_import_source(node_text).map(|source| format!("import:{source}")))
        .unwrap_or_else(|| "import".to_string());

    extract_import(base, node, name, parent_id, is_public, node_text, false)
}

pub(super) fn extract_usingnamespace(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&String>,
    is_public_fn: fn(&BaseExtractor, Node) -> bool,
) -> Option<Symbol> {
    let node_text = base.get_node_text(&node);
    let is_public = is_public_fn(base, node);
    let source = extract_import_source(&node_text);
    let namespace_target = extract_usingnamespace_target(&node_text);

    let name = match source.as_ref() {
        Some(path) => format!("usingnamespace:{path}"),
        None => format!(
            "usingnamespace:{}",
            namespace_target
                .as_deref()
                .unwrap_or("unknown")
                .split_whitespace()
                .collect::<String>()
        ),
    };

    if node_text.contains("@import(") {
        return extract_import(base, node, name, parent_id, is_public, &node_text, true);
    }

    let visibility = if is_public {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let mut metadata = HashMap::new();
    metadata.insert(
        "isUsingNamespace".to_string(),
        serde_json::Value::Bool(true),
    );
    if let Some(target) = namespace_target {
        metadata.insert("target".to_string(), serde_json::Value::String(target));
    }

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(node_text.trim().trim_end_matches(';').to_string()),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: Some(metadata),
            doc_comment: base.extract_documentation(&node),
            annotations: Vec::new(),
        },
    ))
}

fn extract_import(
    base: &mut BaseExtractor,
    node: Node,
    name: String,
    parent_id: Option<&String>,
    is_public: bool,
    node_text: &str,
    is_usingnamespace: bool,
) -> Option<Symbol> {
    let module_path = extract_import_source(node_text);
    let signature = node_text.trim().trim_end_matches(';').to_string();

    let visibility = if is_public {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let mut metadata = HashMap::new();
    if let Some(path) = module_path {
        metadata.insert("source".to_string(), serde_json::Value::String(path));
    }
    if is_usingnamespace {
        metadata.insert(
            "isUsingNamespace".to_string(),
            serde_json::Value::Bool(true),
        );
    }

    Some(base.create_symbol(
        &node,
        name,
        SymbolKind::Import,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.cloned(),
            metadata: if metadata.is_empty() {
                None
            } else {
                Some(metadata)
            },
            doc_comment: base.extract_documentation(&node),
            annotations: Vec::new(),
        },
    ))
}

fn extract_import_source(node_text: &str) -> Option<String> {
    let captures = Regex::new(r#"@import\(([^)]*)\)"#)
        .unwrap()
        .captures(node_text)?;
    let raw_source = captures.get(1)?.as_str().trim();
    Some(
        raw_source
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string(),
    )
}

fn extract_usingnamespace_target(node_text: &str) -> Option<String> {
    Regex::new(r#"usingnamespace\s+(.+?)\s*;?\s*$"#)
        .unwrap()
        .captures(node_text)
        .and_then(|caps| caps.get(1).map(|target| target.as_str().trim().to_string()))
}
