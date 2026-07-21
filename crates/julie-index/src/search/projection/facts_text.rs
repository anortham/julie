//! Web structural-facts text extraction for the search projection.
//!
//! Builds searchable text blobs from web structural facts (route templates, HTTP
//! paths, SQL table names) and merges them into the per-symbol
//! `relationship_text` blob so `fast_search` can find a handler by route path
//! and a routine by table name. Extracted from `apply.rs` for file-size hygiene.
//!
//! Parity: symbols without web facts produce an empty facts-text map, so the
//! merge into `relationship_text` is a no-op and search behavior is
//! byte-identical to the pre-web-facts projection.

use std::collections::HashMap;

use anyhow::Result;
use julie_core::database::SymbolDatabase;
use julie_extractors::base::StructuralFact;

use crate::search::index::truncate_utf8_bytes;

/// Truncate `s` to at most `max_bytes` bytes on a whitespace boundary.
///
/// If `s` fits within `max_bytes`, returns `s` unchanged. Otherwise truncates
/// to `max_bytes` bytes (respecting UTF-8 char boundaries via
/// [`truncate_utf8_bytes`]) then backtracks to the last whitespace so partial
/// identifiers are never left in the index.
pub(super) fn truncate_to_whitespace_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let truncated = truncate_utf8_bytes(s, max_bytes);
    if let Some(idx) = truncated.rfind(char::is_whitespace) {
        &truncated[..idx]
    } else {
        truncated
    }
}

/// Build a searchable text blob for a single web structural fact.
///
/// Pulls the route template / HTTP path / SQL table name out of the fact's
/// metadata and prefixes a kind tag (`route` / `http` / `sql`) so the symbol's
/// `relationship_text` carries tokens a user can search for ("route users",
/// "sql accounts", "api v2"). The `code` tokenizer used for
/// `relationship_text` splits path templates and `table:<name>` forms into
/// their constituent tokens.
fn fact_text_blob(fact: &StructuralFact) -> String {
    let meta = fact.metadata.as_ref();
    let get = |k: &str| -> &str {
        meta.and_then(|m| m.get(k))
            .and_then(|v| v.as_str())
            .unwrap_or("")
    };

    let kind_tag = if fact.pattern_id.contains("route") {
        "route"
    } else if fact.pattern_id.starts_with("sql.") {
        "sql"
    } else if fact.pattern_id.contains("http") {
        "http"
    } else {
        ""
    };

    let verb = {
        let v = get("verb");
        if !v.is_empty() {
            v
        } else {
            get("method")
        }
    };

    let template = [
        "normalized_route_template",
        "route_template",
        "path",
        "url_template",
        "uri_template",
    ]
    .iter()
    .map(|k| get(k))
    .find(|s| !s.is_empty())
    .unwrap_or("");

    let table = get("table_name");

    let mut parts: Vec<String> = Vec::new();
    if !kind_tag.is_empty() {
        parts.push(kind_tag.to_string());
    }
    if !verb.is_empty() {
        parts.push(verb.to_string());
    }
    if !template.is_empty() {
        parts.push(template.to_string());
    }
    if !table.is_empty() {
        parts.push(format!("table:{table}"));
    }
    parts.join(" ")
}

/// Collect web structural-fact text blobs for a batch of symbol IDs.
///
/// For each symbol, loads the structural facts whose `containing_symbol_id`
/// matches (routes, HTTP client calls, SQL statements/tables), builds a text
/// blob per fact via [`fact_text_blob`], joins them, and truncates the per-symbol
/// blob at `max_bytes_per` on the last whitespace boundary.
///
/// Returns a `HashMap<symbol_id, facts_text_blob>`. Symbols with no facts (or
/// facts with no usable metadata) are omitted — callers treat missing keys as
/// empty string, preserving byte-identical parity for symbols without web facts.
pub fn collect_structural_facts_text_bounded(
    db: &SymbolDatabase,
    symbol_ids: &[String],
    max_bytes_per: usize,
) -> Result<HashMap<String, String>> {
    if symbol_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let facts = match db.load_structural_facts_for_symbols(symbol_ids) {
        Ok(f) => f,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("no such table") {
                return Ok(HashMap::new());
            }
            return Err(e);
        }
    };

    let mut result: HashMap<String, String> = HashMap::new();
    for fact in facts {
        let Some(sym_id) = fact.containing_symbol_id.as_ref() else {
            continue;
        };
        let blob = fact_text_blob(&fact);
        if blob.is_empty() {
            continue;
        }
        let entry = result.entry(sym_id.clone()).or_default();
        if !entry.is_empty() {
            entry.push(' ');
        }
        entry.push_str(&blob);
    }

    for blob in result.values_mut() {
        if blob.len() > max_bytes_per {
            *blob = truncate_to_whitespace_boundary(blob, max_bytes_per).to_string();
        }
    }

    Ok(result)
}

/// Merge structural-facts text into an existing `relationship_text` blob,
/// respecting the per-symbol byte cap. When `facts_text` is empty the original
/// `rel_text` is returned unchanged (parity); when `rel_text` is empty the
/// facts blob (truncated to the cap) is returned alone.
pub(super) fn merge_structural_facts_text(
    rel_text: String,
    facts_text: &str,
    max_bytes: usize,
) -> String {
    if facts_text.is_empty() {
        return rel_text;
    }
    let remaining = max_bytes.saturating_sub(rel_text.len());
    if remaining == 0 {
        return rel_text;
    }
    let facts_blob = truncate_to_whitespace_boundary(facts_text, remaining);
    if facts_blob.is_empty() {
        return rel_text;
    }
    if rel_text.is_empty() {
        facts_blob.to_string()
    } else {
        format!("{rel_text} {facts_blob}")
    }
}
