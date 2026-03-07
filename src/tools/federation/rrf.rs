//! Generalized Reciprocal Rank Fusion for N ranked lists.
//!
//! RRF formula: `score(d) = sum over all lists L: 1 / (k + rank_L(d))`
//! where `rank_L(d)` is the 1-based rank of document `d` in list `L`,
//! and the term is 0 if `d` is absent from list `L`.
//!
//! This is a generalization of the 2-list RRF in `search/hybrid.rs` to
//! support merging results from N workspaces in federated search.

use std::collections::HashMap;

/// Default RRF smoothing constant (standard value from the original RRF paper).
pub const RRF_K: u32 = 60;

/// A trait for items that can participate in RRF merging.
///
/// Each item needs a unique ID for deduplication and a mutable score
/// that gets replaced with the computed RRF score.
pub trait RrfItem {
    /// Globally unique identifier for dedup across lists.
    fn rrf_id(&self) -> &str;

    /// Set the RRF score on this item (replaces any existing score).
    fn set_score(&mut self, score: f32);

    /// Get the current score.
    fn score(&self) -> f32;
}

/// Merge N ranked lists using Reciprocal Rank Fusion.
///
/// Each input list is assumed to be pre-sorted by relevance (best first).
/// Items are deduplicated by `RrfItem::rrf_id()` — first occurrence wins
/// for the representative item, but scores accumulate from all lists.
///
/// # Arguments
/// - `lists`: N ranked lists of items
/// - `k`: RRF smoothing constant (typically 60)
/// - `limit`: Maximum number of results to return
///
/// # Returns
/// Merged results sorted by RRF score descending, truncated to `limit`.
pub fn multi_rrf_merge<T: RrfItem>(lists: Vec<Vec<T>>, k: u32, limit: usize) -> Vec<T> {
    // Fast path: no lists or all empty
    if lists.is_empty() {
        return Vec::new();
    }

    // Fast path: single list — normalize to RRF scores for consistency
    // (without this, single-list results keep BM25 scores while multi-list
    // results get much smaller RRF scores, confusing downstream consumers)
    if lists.len() == 1 {
        let mut single = lists.into_iter().next().unwrap();
        let k_f32 = k as f32;
        for (i, item) in single.iter_mut().enumerate() {
            item.set_score(1.0 / (k_f32 + (i + 1) as f32));
        }
        single.truncate(limit);
        return single;
    }

    let k_f32 = k as f32;

    // Accumulate RRF scores keyed by item ID
    let mut scores: HashMap<String, f32> = HashMap::new();
    // Keep one representative item per ID (first seen wins)
    let mut items_by_id: HashMap<String, T> = HashMap::new();

    for list in lists {
        for (i, item) in list.into_iter().enumerate() {
            let rank = (i + 1) as f32;
            let rrf_score = 1.0 / (k_f32 + rank);
            let id = item.rrf_id().to_string();
            *scores.entry(id.clone()).or_insert(0.0) += rrf_score;
            items_by_id.entry(id).or_insert(item);
        }
    }

    // Collect, apply scores, sort descending, truncate
    let mut merged: Vec<T> = items_by_id
        .into_values()
        .map(|mut item| {
            let id = item.rrf_id().to_string();
            item.set_score(scores[&id]);
            item
        })
        .collect();

    merged.sort_by(|a, b| {
        b.score()
            .partial_cmp(&a.score())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    merged.truncate(limit);

    merged
}
