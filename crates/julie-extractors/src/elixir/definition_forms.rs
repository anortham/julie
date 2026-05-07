use super::{ElixirExtractor, attributes, helpers};
use crate::base::{Symbol, SymbolKind, SymbolOptions, Visibility, normalize_annotations};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::Node;

pub(super) fn extract_defguard(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
    visibility: Visibility,
) -> Option<(Symbol, bool)> {
    let (fn_name, params) = helpers::extract_function_head(&extractor.base, node)?;
    let keyword = if visibility == Visibility::Private {
        "defguardp"
    } else {
        "defguard"
    };
    let signature = match &params {
        Some(p) => format!("{} {}{}", keyword, fn_name, p),
        None => format!("{} {}", keyword, fn_name),
    };
    let annotations = normalize_annotations(
        &attributes::collect_preceding_annotations(&extractor.base, node, &["doc", "spec", "impl"]),
        "elixir",
    );
    let mut metadata = HashMap::new();
    metadata.insert("guard".to_string(), Value::Bool(true));

    let symbol = extractor.base.create_symbol(
        node,
        fn_name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(visibility),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: extractor.base.find_doc_comment(node),
            annotations,
        },
    );
    Some((symbol, false))
}

pub(super) fn extract_defdelegate(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let (fn_name, params) = helpers::extract_function_head(&extractor.base, node)?;
    let signature = match &params {
        Some(p) => format!("defdelegate {}{}", fn_name, p),
        None => format!("defdelegate {}", fn_name),
    };
    let annotations = normalize_annotations(
        &attributes::collect_preceding_annotations(&extractor.base, node, &["doc", "spec"]),
        "elixir",
    );
    let mut metadata = HashMap::new();
    metadata.insert("delegate".to_string(), Value::Bool(true));

    let symbol = extractor.base.create_symbol(
        node,
        fn_name,
        SymbolKind::Delegate,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: extractor.base.find_doc_comment(node),
            annotations,
        },
    );
    Some((symbol, false))
}

pub(super) fn extract_defexception(
    extractor: &mut ElixirExtractor,
    node: &Node,
    symbols: &mut Vec<Symbol>,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let fields = helpers::extract_struct_fields(&extractor.base, node);
    let struct_name = extractor
        .module_stack
        .last()
        .cloned()
        .unwrap_or_else(|| "Exception".to_string());
    let field_names: Vec<&str> = fields.iter().map(|(n, _, _)| n.as_str()).collect();
    let signature = format!("defexception [{}]", field_names.join(", "));
    let annotations = normalize_annotations(
        &attributes::collect_preceding_annotations(&extractor.base, node, &["doc"]),
        "elixir",
    );
    let mut metadata = HashMap::new();
    metadata.insert("exception".to_string(), Value::Bool(true));

    let symbol = extractor.base.create_symbol(
        node,
        struct_name,
        SymbolKind::Struct,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: extractor.base.find_doc_comment(node),
            annotations,
        },
    );

    let sym_id = symbol.id.clone();
    for (field_name, _start_byte, _end_byte) in &fields {
        let field_sym = extractor.base.create_symbol(
            node,
            field_name.clone(),
            SymbolKind::Field,
            SymbolOptions {
                signature: Some(format!(":{}", field_name)),
                visibility: Some(Visibility::Public),
                parent_id: Some(sym_id.clone()),
                metadata: None,
                doc_comment: None,
                annotations: Vec::new(),
            },
        );
        symbols.push(field_sym);
    }

    Some((symbol, true))
}

pub(super) fn extract_defoverridable(
    extractor: &mut ElixirExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<(Symbol, bool)> {
    let name = extract_overridable_name(&extractor.base.get_node_text(node))?;
    let mut metadata = HashMap::new();
    metadata.insert("overridable".to_string(), Value::Bool(true));
    let signature = format!("defoverridable {}", name.replace('/', ": "));
    let symbol = extractor.base.create_symbol(
        node,
        name,
        SymbolKind::Method,
        SymbolOptions {
            signature: Some(signature),
            visibility: Some(Visibility::Public),
            parent_id: parent_id.map(String::from),
            metadata: Some(metadata),
            doc_comment: extractor.base.find_doc_comment(node),
            annotations: Vec::new(),
        },
    );
    Some((symbol, false))
}

fn extract_overridable_name(text: &str) -> Option<String> {
    let body = text.trim().strip_prefix("defoverridable")?.trim();
    let (name, arity) = body.split_once(':')?;
    let arity = arity
        .trim()
        .split(|ch: char| !ch.is_ascii_digit())
        .next()
        .unwrap_or_default();
    if name.trim().is_empty() || arity.is_empty() {
        None
    } else {
        Some(format!("{}/{}", name.trim(), arity))
    }
}
