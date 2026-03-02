//! Symbol metadata formatting for embedding generation.
//!
//! Converts Symbol structs into natural language strings suitable for embedding.
//! Only "structural" symbol kinds are embedded — leaf nodes like variables,
//! fields, and imports are too granular for semantic search.

use std::collections::HashMap;

use crate::extractors::{Symbol, SymbolKind};

/// Maximum characters for the embedding input text.
/// BGE-small handles up to 512 tokens (~2000 chars). With enrichment (methods,
/// properties, variants), containers need more room to fit their semantic
/// fingerprint. 600 chars ≈ 120-150 tokens — still well within model capacity.
const MAX_METADATA_CHARS: usize = 600;

/// Symbol kinds worth embedding — structural definitions that carry semantic meaning.
const EMBEDDABLE_KINDS: &[SymbolKind] = &[
    SymbolKind::Function,
    SymbolKind::Method,
    SymbolKind::Class,
    SymbolKind::Struct,
    SymbolKind::Interface,
    SymbolKind::Trait,
    SymbolKind::Enum,
    SymbolKind::Type,
    SymbolKind::Module,
    SymbolKind::Namespace,
    SymbolKind::Union,
];

/// Returns true if this symbol kind is worth embedding.
pub fn is_embeddable_kind(kind: &SymbolKind) -> bool {
    EMBEDDABLE_KINDS.contains(kind)
}

/// Languages that are structural/configuration rather than code logic.
/// These shouldn't compete with code symbols in the semantic vector space.
pub const NON_EMBEDDABLE_LANGUAGES: &[&str] = &[
    "markdown", "json", "jsonl", "toml", "yaml", "css", "html", "regex", "sql",
];

/// Policy for including variable symbols in embedding batches.
#[derive(Debug, Clone, Copy)]
pub struct VariableEmbeddingPolicy {
    pub enabled: bool,
    pub max_ratio: f64,
}

/// Returns true if symbols from this language are worth embedding.
/// Non-code languages (markdown, config files, etc.) produce embeddings
/// that dominate NL queries due to their natural-language headings.
pub fn is_embeddable_language(language: &str) -> bool {
    !NON_EMBEDDABLE_LANGUAGES.contains(&language)
}

/// Format a symbol's metadata into a natural language string for embedding.
///
/// Format: `"{kind} {name} {signature_excerpt} {doc_comment_excerpt}"`
/// Truncated to `MAX_METADATA_CHARS` on a word boundary.
///
/// Examples:
/// - `"function process_payment(amount: f64) -> Result<Receipt>"`
/// - `"struct DatabaseConnection Manages pooled database connections"`
/// - `"trait EmbeddingProvider Trait abstracting vector embedding generation"`
pub fn format_symbol_metadata(symbol: &Symbol) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(4);

    // Kind as lowercase word
    let kind_str = kind_to_str(&symbol.kind);
    parts.push(kind_str);

    // Symbol name
    parts.push(&symbol.name);

    // Signature excerpt (first line only, trimmed)
    let sig_excerpt;
    if let Some(ref sig) = symbol.signature {
        sig_excerpt = first_line_trimmed(sig);
        if !sig_excerpt.is_empty() {
            parts.push(&sig_excerpt);
        }
    }

    // Doc comment excerpt (first sentence or first line)
    let doc_excerpt;
    if let Some(ref doc) = symbol.doc_comment {
        doc_excerpt = first_sentence(doc);
        if !doc_excerpt.is_empty() {
            parts.push(&doc_excerpt);
        }
    }

    let joined = parts.join(" ");
    truncate_on_word_boundary(&joined, MAX_METADATA_CHARS)
}

/// Filter symbols to embeddable ones and format their metadata.
///
/// Returns `(symbol_id, formatted_text)` pairs ready for `embed_batch`.
/// Container kinds whose embeddings benefit from child method names.
const CONTAINER_KINDS: &[SymbolKind] = &[
    SymbolKind::Class,
    SymbolKind::Struct,
    SymbolKind::Interface,
    SymbolKind::Trait,
    SymbolKind::Enum,
];

pub fn prepare_batch_for_embedding(symbols: &[Symbol]) -> Vec<(String, String)> {
    // Build parent_id → child method names mapping for container enrichment.
    let mut methods_by_parent: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut properties_by_parent: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut variants_by_parent: HashMap<&str, Vec<&str>> = HashMap::new();
    for sym in symbols {
        if let Some(ref parent_id) = sym.parent_id {
            match sym.kind {
                SymbolKind::Method | SymbolKind::Function => {
                    methods_by_parent
                        .entry(parent_id.as_str())
                        .or_default()
                        .push(&sym.name);
                }
                SymbolKind::Property | SymbolKind::Field => {
                    properties_by_parent
                        .entry(parent_id.as_str())
                        .or_default()
                        .push(&sym.name);
                }
                SymbolKind::EnumMember => {
                    variants_by_parent
                        .entry(parent_id.as_str())
                        .or_default()
                        .push(&sym.name);
                }
                _ => {}
            }
        }
    }

    symbols
        .iter()
        .filter(|s| is_embeddable_kind(&s.kind) && is_embeddable_language(&s.language))
        .map(|s| {
            let mut text = format_symbol_metadata(s);

            // Enrich container symbols with child method and property/field names.
            // Properties/fields are the semantic fingerprint of DTOs and data types,
            // enabling cross-language matching (e.g., C# UserDto ↔ TS UserDto).
            if CONTAINER_KINDS.contains(&s.kind) {
                if let Some(methods) = methods_by_parent.get(s.id.as_str()) {
                    let suffix = format!(" methods: {}", methods.join(", "));
                    text.push_str(&suffix);
                }
                if let Some(properties) = properties_by_parent.get(s.id.as_str()) {
                    let suffix = format!(" properties: {}", properties.join(", "));
                    text.push_str(&suffix);
                }
                if let Some(variants) = variants_by_parent.get(s.id.as_str()) {
                    let suffix = format!(" variants: {}", variants.join(", "));
                    text.push_str(&suffix);
                }
                text = truncate_on_word_boundary(&text, MAX_METADATA_CHARS);
            }

            (s.id.clone(), text)
        })
        .collect()
}

/// Select variable symbols under a configurable embedding budget.
pub fn select_budgeted_variables(
    symbols: &[Symbol],
    reference_scores: &HashMap<String, f64>,
    base_count: usize,
    policy: &VariableEmbeddingPolicy,
) -> Vec<(String, String)> {
    if !policy.enabled {
        return Vec::new();
    }

    let cap = ((base_count as f64) * policy.max_ratio).floor() as usize;
    if cap == 0 {
        return Vec::new();
    }

    let mut ranked: Vec<(&Symbol, f64)> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| {
            let reference_score = *reference_scores.get(&s.id).unwrap_or(&0.0);
            let score = reference_score + variable_signal_boost(s) - variable_noise_penalty(s);
            (s, score)
        })
        .collect();

    ranked.sort_by(|(a_sym, a_score), (b_sym, b_score)| {
        b_score
            .total_cmp(a_score)
            .then_with(|| a_sym.id.cmp(&b_sym.id))
    });

    ranked
        .into_iter()
        .take(cap)
        .map(|(s, _)| (s.id.clone(), format_symbol_metadata(s)))
        .collect()
}

fn variable_signal_boost(symbol: &Symbol) -> f64 {
    // Descriptive names (snake_case or long identifiers) get a small boost.
    // Language-agnostic: works for any project, unlike domain-specific token lists.
    if symbol.name.contains('_') || symbol.name.len() >= 12 {
        0.15
    } else {
        0.0
    }
}

fn variable_noise_penalty(symbol: &Symbol) -> f64 {
    let mut penalty = 0.0;
    let lower = symbol.name.to_lowercase();

    if lower.len() <= 2 {
        penalty += 0.30;
    }

    if [
        "i", "j", "k", "x", "y", "z", "n", "tmp", "temp", "var", "val", "obj", "data", "res", "req",
    ]
    .contains(&lower.as_str())
    {
        penalty += 0.35;
    }

    if let Some(signature) = &symbol.signature {
        if has_simple_default_literal(signature) {
            penalty += 0.10;
        }
    }

    penalty
}

pub(crate) fn has_simple_default_literal(signature: &str) -> bool {
    const LITERALS: &[&str] = &[
        "0", "1", "true", "false", "none", "null", "nil", "\"\"", "''", "{}", "[]",
    ];
    let bytes = signature.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        // Skip comparison operators: ==, !=, >=, <=
        if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
            continue;
        }
        if i > 0 && matches!(bytes[i - 1], b'=' | b'!' | b'<' | b'>') {
            continue;
        }
        let rhs = signature[i + 1..].trim_start();
        let rhs_lower = rhs.to_lowercase();
        if LITERALS.iter().any(|lit| {
            rhs_lower.starts_with(lit)
                && !rhs_lower[lit.len()..]
                    .starts_with(|c: char| c.is_alphanumeric() || c == '_')
        }) {
            return true;
        }
    }
    false
}

/// Convert SymbolKind to a lowercase embedding-friendly string.
fn kind_to_str(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Class => "class",
        SymbolKind::Struct => "struct",
        SymbolKind::Interface => "interface",
        SymbolKind::Trait => "trait",
        SymbolKind::Enum => "enum",
        SymbolKind::Type => "type",
        SymbolKind::Module => "module",
        SymbolKind::Namespace => "namespace",
        SymbolKind::Union => "union",
        // Non-embeddable kinds — shouldn't reach here, but handle gracefully
        _ => "symbol",
    }
}

/// Extract the first line of text, trimmed.
fn first_line_trimmed(text: &str) -> String {
    text.lines()
        .next()
        .map(|l| l.trim().to_string())
        .unwrap_or_default()
}

/// Extract the first sentence from a doc comment.
/// Strips leading `///`, `//!`, `#`, `*` markers and XML tags, then takes
/// the first line with actual content (skipping tag-only lines like `<summary>`).
pub fn first_sentence(doc: &str) -> String {
    let cleaned: String = doc
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            // Strip common doc comment prefixes
            let stripped = trimmed
                .strip_prefix("///")
                .or_else(|| trimmed.strip_prefix("//!"))
                .or_else(|| trimmed.strip_prefix("/**"))
                .or_else(|| trimmed.strip_prefix("*/"))
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("*"))
                .or_else(|| trimmed.strip_prefix("# "))
                .or_else(|| trimmed.strip_prefix("## "))
                .or_else(|| trimmed.strip_prefix("### "))
                .unwrap_or(trimmed)
                .trim();

            // Strip XML tags (e.g. <summary>, </remarks>, <see cref="..."/>)
            let without_tags = strip_xml_tags(stripped);
            let content = without_tags.trim();

            if content.is_empty() {
                None
            } else {
                Some(content.to_string())
            }
        })
        .next()
        .unwrap_or_default();

    // Take up to the first sentence boundary
    if let Some(pos) = cleaned.find(". ") {
        cleaned[..=pos].to_string()
    } else {
        cleaned
    }
}

/// Strip XML tags from text, preserving content between tags.
/// E.g. `"<see cref=\"Foo\"/>bar"` → `"bar"`, `"<summary>"` → `""`.
fn strip_xml_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

/// Truncate a string on a word boundary, appending no ellipsis.
fn truncate_on_word_boundary(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }

    // Find the last space before the limit
    let truncated = &text[..max_chars];
    if let Some(pos) = truncated.rfind(' ') {
        truncated[..pos].to_string()
    } else {
        // No space found — hard truncate (rare for natural text)
        truncated.to_string()
    }
}
