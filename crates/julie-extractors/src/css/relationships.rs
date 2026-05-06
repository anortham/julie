use crate::base::{
    BaseExtractor, Relationship, RelationshipKind, Symbol, containing_symbol_at_line,
};
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static CUSTOM_PROPERTY_USE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"var\(\s*(--[A-Za-z0-9_-]+)").unwrap());
static ANIMATION_NAME_DECL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\banimation-name\s*:\s*([^;]+)").unwrap());
static CSS_IDENTIFIER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_-]*$").unwrap());

pub(super) fn extract_relationships(base: &BaseExtractor, symbols: &[Symbol]) -> Vec<Relationship> {
    let custom_properties = symbols_by_metadata(symbols, "property");
    let keyframes = symbols_by_metadata(symbols, "animationName");
    let mut relationships = Vec::new();
    let mut seen = HashSet::new();
    let mut in_block_comment = false;

    for (line_index, line) in base.content.lines().enumerate() {
        let line_number = line_index as u32 + 1;
        let scan_line = strip_css_comments(line, &mut in_block_comment);

        for captures in CUSTOM_PROPERTY_USE_RE.captures_iter(&scan_line) {
            if let Some(name) = captures.get(1).map(|matched| matched.as_str()) {
                if let Some(target) = custom_properties.get(name) {
                    push_relationship(
                        base,
                        symbols,
                        target,
                        line_number,
                        name,
                        "custom-property",
                        &mut seen,
                        &mut relationships,
                    );
                }
            }
        }

        for captures in ANIMATION_NAME_DECL_RE.captures_iter(&scan_line) {
            let Some(value) = captures.get(1).map(|matched| matched.as_str()) else {
                continue;
            };
            for name in parse_animation_names(value) {
                if let Some(target) = keyframes.get(name) {
                    push_relationship(
                        base,
                        symbols,
                        target,
                        line_number,
                        name,
                        "keyframes",
                        &mut seen,
                        &mut relationships,
                    );
                }
            }
        }
    }

    relationships
}

fn symbols_by_metadata<'a>(symbols: &'a [Symbol], key: &str) -> HashMap<String, &'a Symbol> {
    symbols
        .iter()
        .filter_map(|symbol| {
            let value = symbol
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get(key))
                .and_then(Value::as_str)?;
            Some((value.to_string(), symbol))
        })
        .collect()
}

fn push_relationship(
    base: &BaseExtractor,
    symbols: &[Symbol],
    target: &Symbol,
    line_number: u32,
    reference_name: &str,
    reference_type: &str,
    seen: &mut HashSet<(String, String, u32, String)>,
    relationships: &mut Vec<Relationship>,
) {
    let Some(source) =
        containing_symbol_at_line(symbols, line_number).filter(|source| source.id != target.id)
    else {
        return;
    };
    let key = (
        source.id.clone(),
        target.id.clone(),
        line_number,
        reference_name.to_string(),
    );
    if !seen.insert(key) {
        return;
    }

    let mut metadata = HashMap::new();
    metadata.insert(
        "referenceName".to_string(),
        Value::String(reference_name.to_string()),
    );
    metadata.insert(
        "referenceType".to_string(),
        Value::String(reference_type.to_string()),
    );

    relationships.push(Relationship {
        id: format!(
            "{}_{}_{:?}_{}_{}",
            source.id,
            target.id,
            RelationshipKind::References,
            line_number,
            reference_name
        ),
        from_symbol_id: source.id.clone(),
        to_symbol_id: target.id.clone(),
        kind: RelationshipKind::References,
        file_path: base.file_path.clone(),
        line_number,
        confidence: 1.0,
        metadata: Some(metadata),
    });
}

fn parse_animation_names(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(',')
        .map(str::trim)
        .filter(|name| CSS_IDENTIFIER_RE.is_match(name))
}

fn strip_css_comments(line: &str, in_block_comment: &mut bool) -> String {
    let mut output = String::with_capacity(line.len());
    let mut remaining = line;

    loop {
        if *in_block_comment {
            if let Some(end_index) = remaining.find("*/") {
                remaining = &remaining[end_index + 2..];
                *in_block_comment = false;
                continue;
            }
            return output;
        }

        if let Some(start_index) = remaining.find("/*") {
            output.push_str(&remaining[..start_index]);
            remaining = &remaining[start_index + 2..];
            *in_block_comment = true;
            continue;
        }

        output.push_str(remaining);
        return output;
    }
}
