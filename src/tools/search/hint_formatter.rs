//! Zero-hit hint formatters for `fast_search`.
//!
//! When `search_target="content"` returns zero hits and no auto-promotion
//! fired, the formatter here produces an informative structured hint (per
//! plan §3.7) instead of the historical terse "no lines found" string. The
//! builders are pure functions so they are testable without a handler.
//!
//! Invoked from `FastSearchTool::execute_with_trace` (content zero-hit
//! branch). `hint_kind` on the trace is set by the caller.

use std::collections::HashSet;

use tantivy::tokenizer::{TokenStream, Tokenizer};

use crate::search::tokenizer::CodeTokenizer;
use crate::tools::search::query::line_match_strategy;
use crate::tools::search::trace::ZeroHitReason;
use crate::tools::search::types::LineMatchStrategy;

/// Whether `query` has two or more whitespace-separated tokens. This is the
/// gate for the multi-token zero-hit hint: single-token content zero-hits
/// flow through the auto-promotion path instead (Task 7).
pub fn is_multi_token_query(query: &str) -> bool {
    query.split_whitespace().count() >= 2
}

/// Tokenize `query` using the same `CodeTokenizer` that drives index-time
/// analysis, returning tokens in first-seen order with duplicates removed.
/// Surfaces the actual tokens the search index would see, which is the
/// information the agent needs when a multi-token query returns zero hits.
pub fn tokenize_query_for_hint(query: &str) -> Vec<String> {
    let mut tokenizer = CodeTokenizer::with_default_patterns();
    let mut stream = tokenizer.token_stream(query);
    let mut tokens = Vec::new();
    let mut seen = HashSet::new();
    while stream.advance() {
        let text = stream.token().text.clone();
        if seen.insert(text.clone()) {
            tokens.push(text);
        }
    }
    tokens
}

/// Display label for a `LineMatchStrategy` used by the hint template.
fn strategy_label(query: &str) -> &'static str {
    match line_match_strategy(query) {
        LineMatchStrategy::FileLevel { .. } => "FileLevel",
        LineMatchStrategy::Tokens { .. } => "Tokens",
        LineMatchStrategy::Substring(_) => "Substring",
    }
}

/// Render a `ZeroHitReason` as its snake_case telemetry string, falling back
/// to "unknown" when absent (Task 4 populates this; during Phase 2 the field
/// may still be `None`).
fn reason_label(reason: Option<&ZeroHitReason>) -> String {
    let Some(reason) = reason else {
        return "unknown".to_string();
    };
    match serde_json::to_value(reason) {
        Ok(serde_json::Value::String(s)) => s,
        _ => "unknown".to_string(),
    }
}

/// Build the structured multi-token content zero-hit hint (plan §3.7).
///
/// The template is verbose on purpose: it fires only on zero-hit responses
/// where the agent has already committed to a search, so the extra tokens
/// buy real diagnostic context. Callers set `trace.hint_kind =
/// Some(HintKind::MultiTokenHint)` independently.
pub fn build_multi_token_zero_hit_hint(
    query: &str,
    file_pattern: Option<&str>,
    language: Option<&str>,
    exclude_tests: Option<bool>,
    zero_hit_reason: Option<&ZeroHitReason>,
) -> String {
    let tokens = tokenize_query_for_hint(query);
    let first_token = tokens.first().map(String::as_str).unwrap_or(query);
    let pattern_display = file_pattern.unwrap_or("(none)");
    let language_display = language.unwrap_or("(none)");
    let exclude_display = match exclude_tests {
        Some(true) => "true",
        Some(false) => "false",
        None => "auto",
    };
    let reason_display = reason_label(zero_hit_reason);
    let strategy = strategy_label(query);
    let tokens_list = tokens.join(", ");

    format!(
        "0 content matches for \"{query}\" with file_pattern={pattern}.\n\
         \n\
         Content search requires all tokens on the same line (under Tokens strategy) or the same file (under FileLevel strategy). Multi-token zero-hits usually mean:\n\
         - Concept query → try: get_context(query=\"{query}\")\n\
         - Symbol lookup → try: fast_search(query=\"{first_token}\", search_target=\"definitions\")\n\
         - Literal phrase → drop to 1-2 key tokens\n\
         \n\
         Tokens: [{tokens_list}]\n\
         Strategy used: {strategy}\n\
         Filters: file_pattern={pattern}, language={language}, exclude_tests={exclude_tests}\n\
         Zero-hit reason: {reason}",
        query = query,
        pattern = pattern_display,
        first_token = first_token,
        tokens_list = tokens_list,
        strategy = strategy,
        language = language_display,
        exclude_tests = exclude_display,
        reason = reason_display,
    )
}
