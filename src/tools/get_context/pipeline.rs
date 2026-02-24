//! Main pipeline: search -> rank -> expand -> allocate -> format

use std::collections::HashMap;

use anyhow::Result;

use super::GetContextTool;
use crate::handler::JulieServerHandler;
use crate::search::index::SymbolSearchResult;
use crate::search::scoring::CENTRALITY_WEIGHT;

/// A pivot symbol selected from search results, with its combined score.
pub struct Pivot {
    pub result: SymbolSearchResult,
    pub combined_score: f32,
}

/// Select pivot symbols from search results using centrality-weighted scoring.
///
/// Applies centrality boost to each result's text relevance score, then selects
/// an adaptive number of pivots based on score distribution:
/// - Top result 2x+ above second -> 1 pivot (clear winner)
/// - Top 3 within 30% of each other -> 3 pivots (cluster)
/// - Otherwise -> 2 pivots (default)
pub fn select_pivots(
    results: Vec<SymbolSearchResult>,
    reference_scores: &HashMap<String, f64>,
) -> Vec<Pivot> {
    if results.is_empty() {
        return Vec::new();
    }

    // Compute combined scores: text_relevance * centrality_boost
    let mut scored: Vec<Pivot> = results
        .into_iter()
        .map(|r| {
            let ref_score = reference_scores.get(&r.id).copied().unwrap_or(0.0);
            let boost = if ref_score > 0.0 {
                1.0 + (1.0 + ref_score as f32).ln() * CENTRALITY_WEIGHT
            } else {
                1.0
            };
            let combined = r.score * boost;
            Pivot {
                result: r,
                combined_score: combined,
            }
        })
        .collect();

    // Sort by combined score descending
    scored.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Determine pivot count from score distribution
    let top_score = scored[0].combined_score;
    let pivot_count = if scored.len() == 1 {
        1
    } else if top_score > scored[1].combined_score * 2.0 {
        1 // Clear winner — top result dominates
    } else if scored.len() >= 3 && scored[2].combined_score >= top_score * 0.7 {
        3 // Cluster — top 3 are close
    } else {
        2 // Default — top 2
    };

    scored.into_iter().take(pivot_count).collect()
}

pub async fn run(tool: &GetContextTool, _handler: &JulieServerHandler) -> Result<String> {
    // Will be implemented in subsequent tasks
    Ok(format!(
        "get_context not yet implemented for query: {}",
        tool.query
    ))
}
