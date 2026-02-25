//! Post-search scoring and reranking.
//!
//! Applies language-specific score boosts based on important_patterns
//! from language configurations. Results whose signatures match patterns
//! like "pub fn", "public class" etc. get a 1.5x score multiplier.

use std::collections::HashMap;

use crate::search::index::SymbolSearchResult;
use crate::search::language_config::LanguageConfigs;

/// Score multiplier for results matching an important pattern.
const IMPORTANT_PATTERN_BOOST: f32 = 1.5;

/// Weight for graph centrality boost (logarithmic scaling).
pub const CENTRALITY_WEIGHT: f32 = 0.3;

/// Symbol names that are too ubiquitous to benefit from centrality scoring.
///
/// These are standard trait impls and common short names that accumulate
/// thousands of references across any codebase. Without filtering, `to_string`
/// (3702 refs) or `clone` (1665 refs) would get massive centrality boosts that
/// warp search rankings. Their high ref counts reflect language mechanics, not
/// actual importance.
///
/// NOTE: Intentionally separate from `NOISE_NEIGHBOR_NAMES` in get_context pipeline,
/// which serves a different purpose (neighbor expansion filtering) and has a different
/// membership set.
pub(crate) const CENTRALITY_NOISE_NAMES: &[&str] = &[
    "clone", "to_string", "fmt", "eq", "ne", "cmp", "partial_cmp",
    "hash", "drop", "deref", "deref_mut", "new", "default", "from", "into",
    "is_empty", "len", "as_ref", "as_mut", "borrow", "borrow_mut",
];

/// Apply important_patterns boost to search results, then re-sort by score.
///
/// For each result, if its signature contains any important_pattern from
/// the result's language config, its score is multiplied by `IMPORTANT_PATTERN_BOOST`.
/// Only one boost is applied per result regardless of how many patterns match.
///
/// After boosting, results are re-sorted by score descending.
pub fn apply_important_patterns_boost(
    results: &mut Vec<SymbolSearchResult>,
    configs: &LanguageConfigs,
) {
    for result in results.iter_mut() {
        if let Some(config) = configs.get(&result.language) {
            for pattern in &config.scoring.important_patterns {
                if result.signature.contains(pattern.as_str()) {
                    result.score *= IMPORTANT_PATTERN_BOOST;
                    break; // Only boost once per result
                }
            }
        }
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
}

/// Apply graph centrality boost to search results, then re-sort.
///
/// Symbols that are referenced more frequently across the codebase get a
/// logarithmic score boost. This promotes well-connected, "important"
/// symbols (e.g. core interfaces, heavily-used utilities) in search rankings.
///
/// Formula: `boosted = score * (1.0 + ln(1 + reference_score) * CENTRALITY_WEIGHT)`
///
/// The logarithmic scaling ensures diminishing returns — a symbol with 100
/// references doesn't dominate 10x more than one with 10 references.
pub fn apply_centrality_boost(
    results: &mut Vec<SymbolSearchResult>,
    reference_scores: &HashMap<String, f64>,
) {
    for result in results.iter_mut() {
        if CENTRALITY_NOISE_NAMES.contains(&result.name.as_str()) {
            continue; // Skip noise — ubiquitous trait impls shouldn't benefit from centrality
        }
        if let Some(&ref_score) = reference_scores.get(&result.id) {
            if ref_score > 0.0 {
                let boost = 1.0 + (1.0 + ref_score as f32).ln() * CENTRALITY_WEIGHT;
                result.score *= boost;
            }
        }
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
}
