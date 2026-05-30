use crate::base::{
    BaseExtractor, EmbeddedSpanOffset, Literal, LiteralKind, NormalizedSpan, Symbol,
};
use std::collections::HashMap;
use tree_sitter::Node;

use super::{find_containing_symbol_id_for_span, get_node_text_from_content};

/// Capture string-literal arguments of a Vue `<script>` `call_expression` as
/// `Literal` records. Config-free: `carrier` is the verbatim callee text; the
/// URL/SQL classification and the carrier gate run later in the `src/` pipeline.
/// `arg_position` is counted over the full named argument list.
pub(super) fn record_vue_call_arg_literals(
    base: &mut BaseExtractor,
    call_node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    script_content: &str,
    offset: EmbeddedSpanOffset,
) {
    let Some(function_node) = call_node.child_by_field_name("function") else {
        return;
    };
    let Some(args_node) = call_node.child_by_field_name("arguments") else {
        return;
    };
    let carrier = vue_callee_text(function_node, script_content);
    let containing_span = offset.apply(NormalizedSpan::from_node(&call_node));
    let containing_symbol_id =
        find_containing_symbol_id_for_span(base, containing_span, symbol_map);

    let mut cursor = args_node.walk();
    for (pos, arg) in args_node.named_children(&mut cursor).enumerate() {
        let Some(text) = decode_vue_string_literal(&arg, script_content) else {
            continue;
        };
        let span = offset.apply(NormalizedSpan::from_node(&arg));
        let literal = Literal {
            id: base.generate_id_for_span(&text, &span),
            literal_text: text,
            kind: LiteralKind::Other,
            carrier: carrier.clone(),
            arg_position: pos as u32,
            language: base.language.clone(),
            file_path: base.file_path.clone(),
            start_line: span.start_line,
            start_column: span.start_column,
            end_line: span.end_line,
            end_column: span.end_column,
            start_byte: span.start_byte,
            end_byte: span.end_byte,
            containing_symbol_id: containing_symbol_id.clone(),
            confidence: 1.0,
        };
        base.literals.push(literal);
    }
}

fn vue_callee_text(function_node: Node, script_content: &str) -> Option<String> {
    match function_node.kind() {
        "identifier" => Some(get_node_text_from_content(&function_node, script_content)),
        "member_expression" => {
            let object = function_node
                .child_by_field_name("object")
                .map(|n| get_node_text_from_content(&n, script_content));
            let property = function_node
                .child_by_field_name("property")
                .map(|n| get_node_text_from_content(&n, script_content));
            match (object, property) {
                (Some(o), Some(p)) => Some(format!("{o}.{p}")),
                (None, Some(p)) => Some(p),
                _ => None,
            }
        }
        _ => {
            let text = get_node_text_from_content(&function_node, script_content);
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

fn decode_vue_string_literal(node: &Node, content: &str) -> Option<String> {
    let kind = node.kind();
    if !(kind.contains("string") || kind.contains("char")) {
        return None;
    }
    let mut out = String::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let ck = child.kind();
        if ck == "interpolation" || ck == "template_substitution" || ck.ends_with("_substitution") {
            out.push_str("{}");
        } else if ck.contains("content")
            || ck.contains("fragment")
            || ck.contains("text")
            || ck == "escape_sequence"
        {
            out.push_str(&get_node_text_from_content(&child, content));
        }
    }
    if out.is_empty() {
        out = strip_vue_string_delimiters(&get_node_text_from_content(node, content));
    }
    Some(out)
}

fn strip_vue_string_delimiters(raw: &str) -> String {
    let s = raw.trim();
    let Some(qpos) = s.find(['"', '\'', '`']) else {
        return s.to_string();
    };
    let s = &s[qpos..];
    for q in ["\"\"\"", "'''"] {
        if s.len() >= 2 * q.len() && s.starts_with(q) && s.ends_with(q) {
            return s[q.len()..s.len() - q.len()].to_string();
        }
    }
    let count = s.chars().count();
    if count >= 2 {
        let first = s.chars().next();
        let last = s.chars().last();
        if let (Some(f), Some(l)) = (first, last) {
            if f == l && matches!(f, '"' | '\'' | '`') {
                return s.chars().skip(1).take(count - 2).collect();
            }
        }
    }
    s.to_string()
}
