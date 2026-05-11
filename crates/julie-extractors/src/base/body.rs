use super::span::NormalizedSpan;
use tree_sitter::Node;

pub type BodySpan = NormalizedSpan;

const BODY_FIELD_NAMES: &[&str] = &["body"];

const BODY_NODE_KINDS: &[&str] = &[
    "block",
    "statement_block",
    "class_body",
    "declaration_list",
    "field_declaration_list",
    "enum_body",
    "trait_body",
    "interface_body",
    "object_body",
    "suite",
    "do_block",
];

pub(crate) fn infer_body_span(
    node: &Node,
    content: &str,
    declaration_span: NormalizedSpan,
) -> Option<BodySpan> {
    for field_name in BODY_FIELD_NAMES {
        if let Some(body) = node.child_by_field_name(field_name) {
            return Some(NormalizedSpan::from_node(&body));
        }
    }

    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| BODY_NODE_KINDS.contains(&child.kind()))
        .map(|child| NormalizedSpan::from_node(&child))
        .or_else(|| infer_body_span_from_span(content, declaration_span))
}

pub(crate) fn body_hash(content: &str, span: BodySpan) -> Option<String> {
    let source = content.get(span.start_byte as usize..span.end_byte as usize)?;
    let tokens = normalized_body_tokens(source);
    let input = tokens.join("\u{1f}");
    Some(format!("{:x}", md5::compute(input.as_bytes())))
}

fn normalized_body_tokens(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }

        if ch == '"' || ch == '\'' || ch == '`' {
            let (token, next_index) = quoted_token(&chars, index, ch);
            tokens.push(token);
            index = next_index;
            continue;
        }

        if is_word_char(ch) {
            let start = index;
            index += 1;
            while index < chars.len() && is_word_char(chars[index]) {
                index += 1;
            }
            tokens.push(chars[start..index].iter().collect());
            continue;
        }

        tokens.push(ch.to_string());
        index += 1;
    }

    tokens
}

fn quoted_token(chars: &[char], start: usize, quote: char) -> (String, usize) {
    let mut index = start + 1;
    let mut escaped = false;

    while index < chars.len() {
        let ch = chars[index];
        index += 1;

        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            break;
        }
    }

    (chars[start..index].iter().collect(), index)
}

fn is_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

pub(crate) fn infer_body_span_from_span(
    content: &str,
    declaration_span: NormalizedSpan,
) -> Option<BodySpan> {
    let start = declaration_span.start_byte as usize;
    let end = declaration_span.end_byte as usize;
    let source = content.get(start..end)?;

    brace_body_span(content, declaration_span, source)
        .or_else(|| html_body_span(content, declaration_span, source))
        .or_else(|| parenthesized_body_span(content, declaration_span, source))
        .or_else(|| sql_as_body_span(content, declaration_span, source))
        .or_else(|| keyword_body_span(content, declaration_span, source))
}

fn brace_body_span(
    content: &str,
    declaration_span: NormalizedSpan,
    source: &str,
) -> Option<BodySpan> {
    let open = source.find('{')?;
    let close = source.rfind('}')?;
    if open >= close {
        return None;
    }
    span_for_relative_range(content, declaration_span, open, close + 1)
}

fn html_body_span(
    content: &str,
    declaration_span: NormalizedSpan,
    source: &str,
) -> Option<BodySpan> {
    if !source.trim_start().starts_with('<') || !source.contains("</") {
        return None;
    }
    let open_end = source.find('>')? + 1;
    let close_start = source.rfind("</")?;
    if open_end > close_start {
        return None;
    }
    span_for_relative_range(content, declaration_span, open_end, close_start)
}

fn parenthesized_body_span(
    content: &str,
    declaration_span: NormalizedSpan,
    source: &str,
) -> Option<BodySpan> {
    let open = source.find('(')?;
    let close = source.rfind(')')?;
    if open >= close {
        return None;
    }
    span_for_relative_range(content, declaration_span, open, close + 1)
}

fn sql_as_body_span(
    content: &str,
    declaration_span: NormalizedSpan,
    source: &str,
) -> Option<BodySpan> {
    let lower = source.to_ascii_lowercase();
    let as_index = lower.find(" as ")?;
    let body_start = as_index + " as ".len();
    if body_start >= source.len() {
        return None;
    }
    span_for_relative_range(content, declaration_span, body_start, source.len())
}

fn keyword_body_span(
    content: &str,
    declaration_span: NormalizedSpan,
    source: &str,
) -> Option<BodySpan> {
    let lower = source.to_ascii_lowercase();
    if !lower.contains(" end")
        && !lower.ends_with("end")
        && !lower.contains("end ")
        && !lower.contains("end;")
    {
        return None;
    }

    let body_start = source.find('\n').map(|index| index + 1)?;
    if body_start >= source.len() {
        return None;
    }
    span_for_relative_range(content, declaration_span, body_start, source.len())
}

fn span_for_relative_range(
    content: &str,
    declaration_span: NormalizedSpan,
    relative_start: usize,
    relative_end: usize,
) -> Option<BodySpan> {
    let absolute_start = declaration_span.start_byte as usize + relative_start;
    let absolute_end = declaration_span.start_byte as usize + relative_end;
    if absolute_start > absolute_end {
        return None;
    }
    NormalizedSpan::from_content_range(content, absolute_start, absolute_end)
}
