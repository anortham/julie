//! Deterministic query-term expansion primitives.

use std::collections::HashSet;

/// Maximum number of additional (non-original) terms.
pub const MAX_ADDED_TERMS: usize = 8;

/// Grouped query terms for later weighted query building.
#[derive(Debug, Clone, Default)]
pub struct ExpandedQueryTerms {
    pub original_terms: Vec<String>,
    pub alias_terms: Vec<String>,
    pub normalized_terms: Vec<String>,
}

/// Expand natural-language query terms into deterministic groups.
pub fn expand_query_terms(query: &str) -> ExpandedQueryTerms {
    let original_terms = tokenize_query(query);
    if original_terms.is_empty() {
        return ExpandedQueryTerms::default();
    }

    let mut seen: HashSet<String> = original_terms.iter().cloned().collect();
    let mut alias_terms = Vec::new();
    let mut normalized_terms = Vec::new();

    if is_nl_like_query(query) {
        apply_aliases(&original_terms, &mut seen, &mut alias_terms);
    }

    apply_normalization(&original_terms, &mut seen, &mut normalized_terms);

    let added_count = alias_terms.len() + normalized_terms.len();
    if added_count > MAX_ADDED_TERMS {
        let keep_alias = alias_terms.len().min(MAX_ADDED_TERMS);
        alias_terms.truncate(keep_alias);
        normalized_terms.truncate(MAX_ADDED_TERMS - keep_alias);
    }

    ExpandedQueryTerms {
        original_terms,
        alias_terms,
        normalized_terms,
    }
}

fn tokenize_query(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut terms = Vec::new();

    for raw in query.split_whitespace() {
        let cleaned = raw
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .to_ascii_lowercase();

        if cleaned.is_empty() {
            continue;
        }

        if seen.insert(cleaned.clone()) {
            terms.push(cleaned);
        }
    }

    terms
}

fn is_nl_like_query(query: &str) -> bool {
    let words: Vec<&str> = query.split_whitespace().collect();
    words.len() > 1
        && words
            .iter()
            .all(|word| word.chars().any(|c| c.is_ascii_alphabetic()))
}

fn apply_aliases(
    original_terms: &[String],
    seen: &mut HashSet<String>,
    alias_terms: &mut Vec<String>,
) {
    for phrase in original_terms.windows(2) {
        let first = phrase[0].as_str();
        let second = phrase[1].as_str();

        for alias in phrase_aliases(first, second) {
            if alias_terms.len() >= MAX_ADDED_TERMS {
                return;
            }

            if seen.insert(alias.to_string()) {
                alias_terms.push(alias.to_string());
            }
        }
    }
}

fn apply_normalization(
    original_terms: &[String],
    seen: &mut HashSet<String>,
    normalized_terms: &mut Vec<String>,
) {
    for term in original_terms {
        if let Some(normalized) = normalize_term(term) {
            if seen.insert(normalized.clone()) {
                normalized_terms.push(normalized);
            }
        }
    }
}

fn phrase_aliases(first: &str, second: &str) -> &'static [&'static str] {
    match (first, second) {
        ("workspace", "routing") => &["router", "registry"],
        ("symbol", "extraction") => &["symbols", "extractor", "indexer"],
        ("dependency", "graph") => &["references", "imports", "edges"],
        ("call", "trace") => &["callers", "callees", "stack"],
        ("index", "refresh") => &["reindex", "rebuild", "backfill"],
        ("semantic", "search") => &["relevance", "ranking", "retrieval"],
        ("reference", "lookup") => &["xref", "findrefs", "navigation"],
        _ => &[],
    }
}

fn normalize_term(term: &str) -> Option<String> {
    if let Some(stem) = term.strip_suffix("ing") {
        if stem.len() >= 5 {
            return Some(stem.to_string());
        }
    }

    if let Some(stem) = term.strip_suffix('s') {
        if stem.len() >= 4 && !term.ends_with("ss") {
            return Some(stem.to_string());
        }
    }

    None
}
