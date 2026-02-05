//! Post-search scoring and reranking.
//!
//! Applies language-specific score boosts based on important_patterns
//! from language configurations. Results whose signatures match patterns
//! like "pub fn", "public class" etc. get a 1.5x score multiplier.

use crate::search::index::SymbolSearchResult;
use crate::search::language_config::LanguageConfigs;

/// Score multiplier for results matching an important pattern.
const IMPORTANT_PATTERN_BOOST: f32 = 1.5;

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
