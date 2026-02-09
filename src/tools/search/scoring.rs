//! Search result scoring and intelligence
//!
//! Provides scoring functions for confidence calculation, insights generation,
//! and next action suggestions based on search results.

use std::collections::HashMap;

use crate::extractors::{Symbol, SymbolKind};

/// Calculate confidence score based on search quality and result relevance
pub fn calculate_search_confidence(query: &str, symbols: &[Symbol]) -> f32 {
    if symbols.is_empty() {
        return 0.0;
    }

    let mut confidence: f32 = 0.5; // Base confidence

    // PERF FIX: Allocate lowercase query once instead of in every iteration
    let query_lowercase = query.to_lowercase();

    // Exact name matches boost confidence
    // PERF FIX: Use eq_ignore_ascii_case to avoid allocations on ASCII-only comparisons
    let exact_matches = symbols
        .iter()
        .filter(|s| s.name.eq_ignore_ascii_case(query))
        .count();
    if exact_matches > 0 {
        confidence += 0.3;
    }

    // Partial matches are medium confidence
    // PERF FIX: Use pre-allocated query_lowercase reference instead of repeated allocations
    let partial_matches = symbols
        .iter()
        .filter(|s| s.name.to_lowercase().contains(&query_lowercase))
        .count();
    if partial_matches > exact_matches {
        confidence += 0.2;
    }

    // More results can indicate ambiguity (lower confidence)
    if symbols.len() > 20 {
        confidence -= 0.1;
    } else if symbols.len() < 5 {
        confidence += 0.1;
    }

    confidence.clamp(0.0, 1.0)
}

/// Generate intelligent insights about search patterns
pub fn generate_search_insights(symbols: &[Symbol], confidence: f32) -> Option<String> {
    if symbols.is_empty() {
        return None;
    }

    let mut insights = Vec::new();

    // Add hint about .julieignore for low-quality results
    if confidence < 0.5 && !symbols.is_empty() {
        insights.push("💡 Getting low-quality results? Consider adding unwanted directories to .julieignore in your project root".to_string());
    }

    // Language distribution
    let mut lang_counts = HashMap::new();
    for symbol in symbols {
        *lang_counts.entry(&symbol.language).or_insert(0) += 1;
    }

    if lang_counts.len() > 1 {
        // Safe: We checked lang_counts.len() > 1, so max_by_key will find a value
        let main_lang = lang_counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .expect("lang_counts must have entries since len > 1");
        insights.push(format!(
            "Found across {} languages (mainly {})",
            lang_counts.len(),
            main_lang.0
        ));
    }

    // Kind distribution
    let mut kind_counts = HashMap::new();
    for symbol in symbols {
        *kind_counts.entry(&symbol.kind).or_insert(0) += 1;
    }

    if let Some((dominant_kind, count)) = kind_counts.iter().max_by_key(|(_, count)| *count) {
        if *count > symbols.len() / 2 {
            insights.push(format!(
                "Mostly {:?}s ({} of {})",
                dominant_kind,
                count,
                symbols.len()
            ));
        }
    }

    if insights.is_empty() {
        None
    } else {
        Some(insights.join(", "))
    }
}

/// Suggest intelligent next actions based on search results
pub fn suggest_next_actions(query: &str, symbols: &[Symbol]) -> Vec<String> {
    let mut actions = Vec::new();

    if symbols.len() == 1 {
        actions.push("Use deep_dive to understand this symbol".to_string());
        actions.push("Use fast_refs to see all usages".to_string());
    } else if symbols.len() > 1 {
        actions.push("Narrow search with language filter".to_string());
        actions.push("Use fast_refs on specific symbols".to_string());
    }

    // Check if we have functions that might be entry points
    if symbols
        .iter()
        .any(|s| matches!(s.kind, SymbolKind::Function) && s.name.contains("main"))
    {
        actions.push("Use fast_explore to understand architecture".to_string());
    }

    // PERF FIX: Allocate lowercase query once instead of in every iteration
    let query_lowercase = query.to_lowercase();
    if symbols
        .iter()
        .any(|s| s.name.to_lowercase().contains(&query_lowercase))
    {
        actions.push("Consider exact name match for precision".to_string());
    }

    actions
}
