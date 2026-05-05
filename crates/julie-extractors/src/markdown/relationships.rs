use crate::base::{BaseExtractor, Relationship, RelationshipKind, Symbol};
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static LOCAL_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[^\]]+\]\(#([^)]+)\)").unwrap());

pub(super) fn extract_relationships(base: &BaseExtractor, symbols: &[Symbol]) -> Vec<Relationship> {
    let headings = heading_symbols_by_slug(symbols);
    let mut relationships = Vec::new();
    let mut seen = HashSet::new();

    for (line_index, line) in base.content.lines().enumerate() {
        let line_number = line_index as u32 + 1;
        for captures in LOCAL_LINK_RE.captures_iter(line) {
            let Some(raw_anchor) = captures.get(1).map(|matched| matched.as_str()) else {
                continue;
            };
            let slug = normalize_anchor(raw_anchor);
            let Some(target) = headings.get(&slug) else {
                continue;
            };
            let Some(source) = containing_symbol(symbols, line_number) else {
                continue;
            };
            push_relationship(
                base,
                source,
                target,
                line_number,
                raw_anchor,
                &mut seen,
                &mut relationships,
            );
        }
    }

    relationships
}

fn heading_symbols_by_slug(symbols: &[Symbol]) -> HashMap<String, &Symbol> {
    symbols
        .iter()
        .map(|symbol| (slugify_heading(&symbol.name), symbol))
        .collect()
}

fn push_relationship(
    base: &BaseExtractor,
    source: &Symbol,
    target: &Symbol,
    line_number: u32,
    anchor: &str,
    seen: &mut HashSet<(String, String, u32, String)>,
    relationships: &mut Vec<Relationship>,
) {
    let key = (
        source.id.clone(),
        target.id.clone(),
        line_number,
        anchor.to_string(),
    );
    if !seen.insert(key) {
        return;
    }

    let mut metadata = HashMap::new();
    metadata.insert("anchor".to_string(), Value::String(anchor.to_string()));

    relationships.push(Relationship {
        id: format!(
            "{}_{}_{:?}_{}_{}",
            source.id,
            target.id,
            RelationshipKind::References,
            line_number,
            anchor
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

fn containing_symbol(symbols: &[Symbol], line_number: u32) -> Option<&Symbol> {
    symbols
        .iter()
        .filter(|symbol| symbol.start_line <= line_number && symbol.end_line >= line_number)
        .min_by_key(|symbol| symbol.end_line.saturating_sub(symbol.start_line))
}

fn normalize_anchor(anchor: &str) -> String {
    slugify_heading(&anchor.replace('-', " "))
}

fn slugify_heading(text: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_dash = false;
        } else if ch.is_whitespace() || ch == '-' {
            if !last_was_dash && !slug.is_empty() {
                slug.push('-');
                last_was_dash = true;
            }
        }
    }

    slug.trim_matches('-').to_string()
}
