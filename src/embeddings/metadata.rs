//! Symbol metadata formatting for embedding generation.
//!
//! Converts Symbol structs into natural language strings suitable for embedding.
//! Only "structural" symbol kinds are embedded — leaf nodes like variables,
//! fields, and imports are too granular for semantic search.

use crate::extractors::{Symbol, SymbolKind};

/// Maximum characters for the embedding input text.
/// BGE-small handles up to 512 tokens (~2000 chars), but shorter is better
/// for embedding quality. 400 chars ≈ 80-100 tokens.
const MAX_METADATA_CHARS: usize = 400;

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
pub fn prepare_batch_for_embedding(symbols: &[Symbol]) -> Vec<(String, String)> {
    symbols
        .iter()
        .filter(|s| is_embeddable_kind(&s.kind))
        .map(|s| (s.id.clone(), format_symbol_metadata(s)))
        .collect()
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
/// Strips leading `///`, `//!`, `#`, `*` markers and trims.
fn first_sentence(doc: &str) -> String {
    let cleaned: String = doc
        .lines()
        .map(|line| {
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
                .unwrap_or(trimmed);
            stripped.trim()
        })
        .next()
        .unwrap_or("")
        .to_string();

    // Take up to the first sentence boundary
    if let Some(pos) = cleaned.find(". ") {
        cleaned[..=pos].to_string()
    } else {
        cleaned
    }
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
