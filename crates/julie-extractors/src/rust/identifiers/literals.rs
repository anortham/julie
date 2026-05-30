use crate::base::BaseExtractor;
use crate::rust::RustExtractor;

use super::containing_symbols::ContainingSymbolIndex;

/// Capture string-literal arguments of a Rust `call_expression` as `Literal`
/// records.
///
/// Config-free: `carrier` is the verbatim callee text; the URL/SQL
/// classification and the carrier gate run later in the `src/` pipeline.
/// Records one literal per string-like argument, with `arg_position` counted
/// over the full `arguments` list. The turbofish wrapper (`generic_function`)
/// is unwrapped to its inner callee first.
///
/// NOTE: This arm targets `call_expression` only. sqlx's `query!`/`query_as!`
/// macros are `macro_invocation` nodes — captured separately by
/// [`record_rust_macro_arg_literals`].
pub(super) fn record_rust_call_arg_literals(
    extractor: &mut RustExtractor,
    call_node: tree_sitter::Node,
    containing_symbols: &ContainingSymbolIndex<'_>,
) {
    let Some(func_node) = call_node.child_by_field_name("function") else {
        return;
    };
    let Some(args_node) = call_node.child_by_field_name("arguments") else {
        return;
    };
    let callee = if func_node.kind() == "generic_function" {
        func_node
            .child_by_field_name("function")
            .unwrap_or(func_node)
    } else {
        func_node
    };
    let containing_symbol_id = containing_symbols
        .find(call_node)
        .map(|symbol| symbol.id.clone());
    let carrier = rust_carrier(extractor.get_base_mut(), callee);

    let mut cursor = args_node.walk();
    for (pos, arg) in args_node.named_children(&mut cursor).enumerate() {
        let base = extractor.get_base_mut();
        if let Some(text) = base.decode_string_literal(&arg) {
            base.record_literal(
                &arg,
                text,
                carrier.clone(),
                pos as u32,
                containing_symbol_id.clone(),
            );
        }
    }
}

/// Capture string-literal tokens inside a Rust `macro_invocation` as `Literal`
/// records — primarily sqlx's `query!`/`query_as!`/`query_scalar!`, the
/// compile-time-checked SQL form that dominates real Rust DB code.
pub(super) fn record_rust_macro_arg_literals(
    extractor: &mut RustExtractor,
    macro_node: tree_sitter::Node,
    containing_symbols: &ContainingSymbolIndex<'_>,
) {
    let Some(macro_name_node) = macro_node.child_by_field_name("macro") else {
        return;
    };
    let token_tree = {
        let mut cursor = macro_node.walk();
        macro_node
            .children(&mut cursor)
            .find(|c| c.kind() == "token_tree")
    };
    let Some(token_tree) = token_tree else {
        return;
    };
    let carrier = rust_macro_carrier(extractor.get_base_mut(), macro_name_node);
    let containing_symbol_id = containing_symbols
        .find(macro_node)
        .map(|symbol| symbol.id.clone());

    let tokens: Vec<tree_sitter::Node> = {
        let mut cursor = token_tree.walk();
        token_tree.named_children(&mut cursor).collect()
    };
    for (pos, token) in tokens.into_iter().enumerate() {
        let base = extractor.get_base_mut();
        if let Some(text) = base.decode_string_literal(&token) {
            base.record_literal(
                &token,
                text,
                carrier.clone(),
                pos as u32,
                containing_symbol_id.clone(),
            );
        }
    }
}

/// Derive a Rust call's carrier from its (turbofish-unwrapped) callee.
fn rust_carrier(base: &BaseExtractor, callee: tree_sitter::Node) -> Option<String> {
    match callee.kind() {
        "identifier" => Some(base.get_node_text(&callee)),
        "field_expression" => {
            let value = callee
                .child_by_field_name("value")
                .map(|n| base.get_node_text(&n));
            let field = callee
                .child_by_field_name("field")
                .map(|n| base.get_node_text(&n));
            match (value, field) {
                (Some(v), Some(f)) => Some(format!("{v}.{f}")),
                (None, Some(f)) => Some(f),
                _ => None,
            }
        }
        "scoped_identifier" => {
            let name = callee
                .child_by_field_name("name")
                .map(|n| base.get_node_text(&n));
            let qualifier = callee.child_by_field_name("path").map(|p| {
                if p.kind() == "scoped_identifier" {
                    p.child_by_field_name("name")
                        .map(|n| base.get_node_text(&n))
                        .unwrap_or_else(|| base.get_node_text(&p))
                } else {
                    base.get_node_text(&p)
                }
            });
            match (qualifier, name) {
                (Some(q), Some(n)) => Some(format!("{q}.{n}")),
                (None, Some(n)) => Some(n),
                _ => None,
            }
        }
        _ => {
            let text = base.get_node_text(&callee);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

/// Derive a Rust macro's carrier from its `macro` field: the last path segment,
/// no `!`. `query!` → `query`; `sqlx::query!` → `query`; `query_as!` →
/// `query_as`.
fn rust_macro_carrier(base: &BaseExtractor, macro_name_node: tree_sitter::Node) -> Option<String> {
    match macro_name_node.kind() {
        "identifier" => Some(base.get_node_text(&macro_name_node)),
        "scoped_identifier" => macro_name_node
            .child_by_field_name("name")
            .map(|n| base.get_node_text(&n))
            .or_else(|| Some(base.get_node_text(&macro_name_node))),
        _ => {
            let text = base.get_node_text(&macro_name_node);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}
