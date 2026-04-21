//! Debug search infrastructure for the search playground.
//!
//! Wraps the normal search path and enriches each result with scoring
//! intermediates: raw BM25 score, centrality boost, pattern boost,
//! NL-path prior, and a human-readable explanation.

use std::collections::HashMap;

use serde::Serialize;

use crate::search::index::{ContentSearchResults, SearchFilter, SearchIndex, SymbolSearchResult};
use crate::search::scoring::{
    self, CENTRALITY_WEIGHT, NL_PATH_BOOST_SRC, NL_PATH_PENALTY_DOCS, NL_PATH_PENALTY_FIXTURES,
    NL_PATH_PENALTY_TESTS,
};

/// A symbol search result enriched with scoring breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct SymbolDebugResult {
    // -- original fields --
    pub id: String,
    pub name: String,
    pub signature: String,
    pub doc_comment: String,
    pub file_path: String,
    pub kind: String,
    pub language: String,
    pub start_line: u32,
    pub final_score: f32,

    // -- debug fields --
    pub bm25_score: f32,
    pub centrality_score: f64,
    pub centrality_boost: f32,
    pub pattern_boost: f32,
    pub nl_path_boost: f32,
    pub field_matches: Vec<String>,
    pub query_tokens: Vec<String>,
    pub relaxed: bool,
    pub boost_explanation: String,
}

/// A content search result enriched with scoring breakdown.
#[derive(Debug, Clone, Serialize)]
pub struct ContentDebugResult {
    pub file_path: String,
    pub language: String,
    pub final_score: f32,
    pub bm25_score: f32,
    pub query_tokens: Vec<String>,
    pub relaxed: bool,
}

/// Result container for debug symbol search.
#[derive(Debug, Serialize)]
pub struct SymbolDebugResults {
    pub results: Vec<SymbolDebugResult>,
    pub relaxed: bool,
    pub query_tokens: Vec<String>,
    pub total_candidates: usize,
}

/// Result container for debug content search.
#[derive(Debug, Serialize)]
pub struct ContentDebugResults {
    pub results: Vec<ContentDebugResult>,
    pub relaxed: bool,
    pub query_tokens: Vec<String>,
}

/// Run a content search with debug scoring breakdown.
pub fn search_content_debug(
    search_index: &SearchIndex,
    query_str: &str,
    filter: &SearchFilter,
    limit: usize,
) -> crate::search::Result<ContentDebugResults> {
    let query_tokens = search_index.tokenize_query_public(query_str);

    let ContentSearchResults {
        results, relaxed, ..
    } = search_index.search_content(query_str, filter, limit)?;

    let debug_results: Vec<ContentDebugResult> = results
        .into_iter()
        .map(|r| ContentDebugResult {
            file_path: r.file_path,
            language: r.language,
            final_score: r.score,
            // Content search has no post-Tantivy boosts, so BM25 == final
            bm25_score: r.score,
            query_tokens: query_tokens.clone(),
            relaxed,
        })
        .collect();

    Ok(ContentDebugResults {
        results: debug_results,
        relaxed,
        query_tokens,
    })
}

/// Enrich a single symbol result with scoring breakdown.
///
/// We work backwards from the final score to reconstruct intermediates.
/// The boost pipeline in `search_symbols` is:
///   1. BM25 score (from Tantivy)
///   2. `apply_important_patterns_boost` — multiplies by 1.5 if matched
///   3. `apply_nl_path_prior` — multiplies by path-type factor if NL-like query
///
/// We reverse these to recover the raw BM25 score.
pub fn enrich_symbol_result(
    result: SymbolSearchResult,
    query_str: &str,
    reference_scores: &HashMap<String, f64>,
    query_tokens: &[String],
    relaxed: bool,
) -> SymbolDebugResult {
    let final_score = result.score;

    // Determine what nl_path_prior boost was applied
    let nl_path_boost = compute_nl_path_boost(query_str, &result.file_path);

    // Determine what important_patterns boost was applied
    let pattern_boost = compute_pattern_boost(&result);

    // Reverse the boosts to get raw BM25
    let bm25_score = final_score / (pattern_boost * nl_path_boost);

    // Look up centrality
    let centrality_score = reference_scores.get(&result.id).copied().unwrap_or(0.0);

    // Compute what centrality boost would be (note: centrality is NOT applied
    // inside search_symbols — it's applied separately by callers like text_search_impl).
    // We report it for informational purposes.
    let centrality_boost = if centrality_score > 0.0
        && !scoring::CENTRALITY_NOISE_NAMES.contains(&result.name.as_str())
    {
        1.0 + (1.0 + centrality_score as f32).ln() * CENTRALITY_WEIGHT
    } else {
        1.0
    };

    // Determine which fields matched (heuristic: check if any query token
    // appears in each field, case-insensitive)
    let field_matches = compute_field_matches(&result, query_tokens);

    // Build explanation
    let mut explanation = Vec::new();
    explanation.push(format!("BM25: {:.4}", bm25_score));

    if pattern_boost != 1.0 {
        explanation.push(format!("important_pattern: x{:.2}", pattern_boost));
    }
    if nl_path_boost != 1.0 {
        explanation.push(format!("nl_path_prior: x{:.2}", nl_path_boost));
    }
    if centrality_boost != 1.0 {
        explanation.push(format!(
            "centrality: {:.4} (boost x{:.4}, NOT applied in search_symbols)",
            centrality_score, centrality_boost
        ));
    }
    explanation.push(format!("final: {:.4}", final_score));

    SymbolDebugResult {
        id: result.id,
        name: result.name,
        signature: result.signature,
        doc_comment: result.doc_comment,
        file_path: result.file_path,
        kind: result.kind,
        language: result.language,
        start_line: result.start_line,
        final_score,
        bm25_score,
        centrality_score,
        centrality_boost,
        pattern_boost,
        nl_path_boost,
        field_matches,
        query_tokens: query_tokens.to_vec(),
        relaxed,
        boost_explanation: explanation.join(" → "),
    }
}

/// Compute what NL-path-prior boost factor was applied to this result.
fn compute_nl_path_boost(query_str: &str, file_path: &str) -> f32 {
    if !scoring::is_nl_like_query(query_str) {
        return 1.0;
    }

    if scoring::is_test_path(file_path) {
        NL_PATH_PENALTY_TESTS
    } else if scoring::is_docs_path(file_path) {
        NL_PATH_PENALTY_DOCS
    } else if scoring::is_fixture_path(file_path) {
        NL_PATH_PENALTY_FIXTURES
    } else {
        NL_PATH_BOOST_SRC
    }
}

/// Compute what important-patterns boost was applied.
///
/// We can't perfectly reconstruct this without the LanguageConfigs, but
/// the boost is always exactly 1.5 when it fires. If the final score
/// is a multiple of 1.5 relative to what BM25 would produce, we know
/// the pattern boost was applied. However, since we're dividing out,
/// we just report 1.5 as a constant — the caller can check if it's
/// meaningful by looking at the signature.
///
/// For now, we always report 1.0 (no boost) because we can't reliably
/// detect it without the configs. A future enhancement could accept
/// LanguageConfigs here.
fn compute_pattern_boost(_result: &SymbolSearchResult) -> f32 {
    // TODO: Accept LanguageConfigs to detect important_patterns match.
    // Without the configs, we can't know which patterns were checked.
    1.0
}

/// Determine which indexed fields matched the query tokens.
///
/// This is a heuristic: we check case-insensitively whether any query
/// token substring-matches the field value. Tantivy's actual matching
/// uses tokenized terms, but this gives a good approximation for the
/// debug UI.
fn compute_field_matches(result: &SymbolSearchResult, query_tokens: &[String]) -> Vec<String> {
    let mut matches = Vec::new();

    let name_lower = result.name.to_lowercase();
    let sig_lower = result.signature.to_lowercase();
    let doc_lower = result.doc_comment.to_lowercase();

    for token in query_tokens {
        let t = token.to_lowercase();
        if name_lower.contains(&t) && !matches.contains(&"name".to_string()) {
            matches.push("name".to_string());
        }
        if sig_lower.contains(&t) && !matches.contains(&"signature".to_string()) {
            matches.push("signature".to_string());
        }
        if doc_lower.contains(&t) && !matches.contains(&"doc_comment".to_string()) {
            matches.push("doc_comment".to_string());
        }
    }

    matches
}
