use tree_sitter::Node;

use super::extractor::BaseExtractor;

/// Decode a string-literal node's contents for capture.
pub(crate) fn decode_string_literal(base: &BaseExtractor, node: &Node) -> Option<String> {
    let kind = node.kind();
    if !(kind.contains("string") || kind.contains("char")) {
        return None;
    }
    let mut out = String::new();
    decode_string_children(base, node, &mut out);
    if out.is_empty() {
        out = strip_string_delimiters(&base.get_node_text(node));
    }
    Some(out)
}

fn strip_string_delimiters(raw: &str) -> String {
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

fn decode_string_children(base: &BaseExtractor, node: &Node, out: &mut String) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let ck = child.kind();
        if ck.contains("content")
            || ck.contains("fragment")
            || ck.contains("text")
            || ck.contains("template_chars")
            || ck == "escape_sequence"
        {
            out.push_str(&base.get_node_text(&child));
        } else if is_interpolation_hole(ck) {
            out.push_str("{}");
        } else if child.named_child_count() > 0 {
            decode_string_children(base, &child, out);
        }
    }
}

fn is_interpolation_hole(ck: &str) -> bool {
    if matches!(
        ck,
        "simple_expansion" | "expansion" | "arithmetic_expansion"
    ) {
        return true;
    }
    (ck.contains("interpolat") || ck.contains("substitution"))
        && !ck.ends_with("_start")
        && !ck.ends_with("_end")
        && !ck.ends_with("_quote")
        && !ck.ends_with("_brace")
        && !ck.ends_with("_open")
        && !ck.ends_with("_close")
}
