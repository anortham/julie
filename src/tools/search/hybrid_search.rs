//! Hybrid search combining text and semantic results
//!
//! Implements result fusion combining text search and semantic search
//! with intelligent scoring and ranking.

use std::collections::HashMap;

use anyhow::Result;
use tracing::debug;

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::utils::{exact_match_boost::ExactMatchBoost, path_relevance::PathRelevanceScorer};

/// Hybrid search combining text and semantic methods
///
/// Runs both text and semantic searches in parallel and fuses results
/// with intelligent scoring that boosts symbols appearing in both searches.
pub async fn hybrid_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    debug!("🔄 Hybrid search mode (text + semantic fusion)");

    // Run both searches in parallel for optimal performance
    // Both searches now respect workspace filtering
    let (text_results, semantic_results) = tokio::join!(
        crate::tools::search::text_search::text_search_impl(
            query,
            language,
            file_pattern,
            limit,
            workspace_ids.clone(),
            "symbols", // Hybrid search is for finding symbols
            None,      // context_lines: use default
            handler
        ),
        crate::tools::search::semantic_search::semantic_search_impl(
            query,
            language,
            file_pattern,
            limit,
            workspace_ids.clone(),
            handler
        )
    );

    // Handle errors gracefully - if one fails, use the other
    let text_symbols = match text_results {
        Ok(symbols) => symbols,
        Err(e) => {
            debug!("Text search failed in hybrid mode: {}", e);
            Vec::new()
        }
    };

    let semantic_symbols = match semantic_results {
        Ok(symbols) => symbols,
        Err(e) => {
            debug!("Semantic search failed in hybrid mode: {}", e);
            Vec::new()
        }
    };

    // If both searches failed, return an error
    if text_symbols.is_empty() && semantic_symbols.is_empty() {
        return Ok(Vec::new());
    }

    // Create a scoring map for fusion
    // Key: symbol ID, Value: (symbol, text_rank, semantic_rank, combined_score)
    let mut fusion_map: HashMap<String, (Symbol, Option<f32>, Option<f32>, f32)> = HashMap::new();

    // Add text search results with normalized scores
    for (rank, symbol) in text_symbols.iter().enumerate() {
        // Normalize rank to score (earlier results get higher scores)
        let text_score = 1.0 - (rank as f32 / text_symbols.len().max(1) as f32);
        fusion_map.insert(
            symbol.id.clone(),
            (symbol.clone(), Some(text_score), None, text_score * 0.6), // 60% weight for text
        );
    }

    // Add semantic search results with normalized scores
    for (rank, symbol) in semantic_symbols.iter().enumerate() {
        // Normalize rank to score (earlier results get higher scores)
        let semantic_score = 1.0 - (rank as f32 / semantic_symbols.len().max(1) as f32);

        fusion_map
            .entry(symbol.id.clone())
            .and_modify(|(existing_symbol, text_score, sem_score, combined)| {
                // Symbol appears in both results - boost the score!
                *sem_score = Some(semantic_score);

                // Calculate weighted fusion score with overlap bonus
                let text_weight = text_score.unwrap_or(0.0) * 0.6; // 60% weight for text
                let sem_weight = semantic_score * 0.4; // 40% weight for semantic
                let overlap_bonus = 0.2; // Bonus for appearing in both

                *combined = text_weight + sem_weight + overlap_bonus;
                *combined = combined.min(1.0); // Cap at 1.0

                debug!(
                    "Symbol '{}' found in both searches - boosted score to {:.2}",
                    existing_symbol.name, *combined
                );
            })
            .or_insert((
                symbol.clone(),
                None,
                Some(semantic_score),
                semantic_score * 0.4, // 40% weight for semantic-only
            ));
    }

    // Sort by combined score (descending)
    let mut ranked_results: Vec<(Symbol, f32)> = fusion_map
        .into_values()
        .map(|(symbol, _text, _sem, score)| (symbol, score))
        .collect();

    ranked_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Apply exact match boost and path relevance scoring (same as text search)
    let path_scorer = PathRelevanceScorer::new(query);
    let exact_match_booster = ExactMatchBoost::new(query);

    // Re-rank with additional scoring factors
    ranked_results.sort_by(|a, b| {
        // Combine fusion score with exact match and path relevance
        let final_score_a = a.1
            * exact_match_booster.calculate_boost(&a.0.name)
            * path_scorer.calculate_score(&a.0.file_path);

        let final_score_b = b.1
            * exact_match_booster.calculate_boost(&b.0.name)
            * path_scorer.calculate_score(&b.0.file_path);

        final_score_b
            .partial_cmp(&final_score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Extract symbols and limit to requested count
    let final_results: Vec<Symbol> = ranked_results
        .into_iter()
        .take(limit as usize)
        .map(|(symbol, _score)| symbol)
        .collect();

    debug!(
        "🎯 Hybrid search complete: {} text + {} semantic = {} unique results (showing {})",
        text_symbols.len(),
        semantic_symbols.len(),
        final_results.len(),
        final_results.len().min(limit as usize)
    );

    Ok(final_results)
}
