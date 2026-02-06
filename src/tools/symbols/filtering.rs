//! Symbol filtering logic - Extract symbols by depth, target, and limit
//!
//! This module contains reusable functions for filtering symbols that are used
//! by both primary and reference workspace symbol retrieval. The filtering
//! happens in three stages: depth filtering, target filtering, and limit filtering.
//!
//! Internally, all filter stages operate on `Vec<usize>` indices into the original
//! symbol array. A single clone happens at the end of `apply_all_filters`. The
//! public per-stage functions retain their Vec<Symbol> signatures for backward
//! compatibility.

use std::collections::{HashMap, HashSet};
use tracing::debug;

use crate::extractors::base::Symbol;

// ============================================================================
// Shared hierarchy helpers
// ============================================================================

/// Build a parent_id -> children index for efficient hierarchy navigation
pub fn build_parent_to_children(symbols: &[Symbol]) -> HashMap<String, Vec<usize>> {
    let mut parent_to_children: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, symbol) in symbols.iter().enumerate() {
        if let Some(ref parent_id) = symbol.parent_id {
            parent_to_children
                .entry(parent_id.clone())
                .or_default()
                .push(idx);
        }
    }
    parent_to_children
}

/// Find all top-level symbols (those with no parent)
pub fn find_top_level_symbols(symbols: &[Symbol]) -> Vec<usize> {
    symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| s.parent_id.is_none())
        .map(|(idx, _)| idx)
        .collect()
}

/// Recursively collect symbols up to a maximum depth
///
/// This function traverses the symbol hierarchy and collects all symbols
/// that are within the specified maximum depth from their top-level parent.
pub fn collect_symbols_by_depth(
    indices: &[usize],
    depth: u32,
    max_depth: u32,
    all_symbols: &[Symbol],
    parent_to_children: &HashMap<String, Vec<usize>>,
    result: &mut Vec<usize>,
) {
    if depth > max_depth {
        return;
    }

    for &idx in indices {
        result.push(idx);
        if depth < max_depth {
            if let Some(children_indices) = parent_to_children.get(&all_symbols[idx].id) {
                collect_symbols_by_depth(
                    children_indices,
                    depth + 1,
                    max_depth,
                    all_symbols,
                    parent_to_children,
                    result,
                );
            }
        }
    }
}


// ============================================================================
// Index-returning filter internals (zero-clone pipeline)
// ============================================================================

/// Return indices of symbols within max_depth (into `all_symbols`).
fn max_depth_indices(all_symbols: &[Symbol], max_depth: u32) -> Vec<usize> {
    let parent_to_children = build_parent_to_children(all_symbols);
    let top_level_indices = find_top_level_symbols(all_symbols);

    debug!(
        "Symbol hierarchy: {} total, {} top-level",
        all_symbols.len(),
        top_level_indices.len()
    );

    let mut indices = Vec::new();
    collect_symbols_by_depth(
        &top_level_indices,
        0,
        max_depth,
        all_symbols,
        &parent_to_children,
        &mut indices,
    );

    debug!(
        "After max_depth={} filtering: {} -> {} symbols",
        max_depth,
        all_symbols.len(),
        indices.len()
    );

    indices
}

/// From the given `candidate_indices` (into `all_symbols`), return those whose
/// name matches `target` (case-insensitive partial) plus all their descendants
/// that also appear in `candidate_indices`.
fn target_indices(
    all_symbols: &[Symbol],
    candidate_indices: &[usize],
    target: &str,
) -> Vec<usize> {
    let target_lower = target.to_lowercase();
    let candidate_set: HashSet<usize> = candidate_indices.iter().copied().collect();

    // Find candidates whose name matches
    let matching: Vec<usize> = candidate_indices
        .iter()
        .copied()
        .filter(|&idx| all_symbols[idx].name.to_lowercase().contains(&target_lower))
        .collect();

    if matching.is_empty() {
        return Vec::new();
    }

    // Collect matched symbols + their descendants (restricted to candidate set)
    let mut result = Vec::new();
    for &idx in &matching {
        result.push(idx);
        add_descendants_within(
            &all_symbols[idx].id,
            all_symbols,
            &candidate_set,
            &mut result,
        );
    }

    debug!(
        "After target='{}' filtering: {} symbols",
        target,
        result.len()
    );

    result
}

/// Like `add_descendants` but only includes indices that are in `allowed`.
fn add_descendants_within(
    parent_id: &str,
    symbols: &[Symbol],
    allowed: &HashSet<usize>,
    result: &mut Vec<usize>,
) {
    for (idx, symbol) in symbols.iter().enumerate() {
        if !allowed.contains(&idx) {
            continue;
        }
        if let Some(ref pid) = symbol.parent_id {
            if pid == parent_id && !result.contains(&idx) {
                result.push(idx);
                add_descendants_within(&symbol.id, symbols, allowed, result);
            }
        }
    }
}

/// From the given `candidate_indices` (into `all_symbols`), keep the first
/// `limit` top-level symbols plus all their descendants. Returns
/// (indices, was_truncated).
fn limit_indices(
    all_symbols: &[Symbol],
    candidate_indices: &[usize],
    limit: u32,
) -> (Vec<usize>, bool) {
    let limit_usize = limit as usize;

    // Count top-level among candidates
    let top_level: Vec<usize> = candidate_indices
        .iter()
        .copied()
        .filter(|&idx| all_symbols[idx].parent_id.is_none())
        .collect();

    if top_level.len() <= limit_usize {
        return (candidate_indices.to_vec(), false);
    }

    // Keep first `limit` top-level symbols
    let kept_top: &[usize] = &top_level[..limit_usize];
    let mut result: Vec<usize> = kept_top.to_vec();

    // Add descendants of those top-level symbols (only from candidate set)
    let candidate_set: HashSet<usize> = candidate_indices.iter().copied().collect();
    let top_ids: HashSet<String> = kept_top
        .iter()
        .map(|&idx| all_symbols[idx].id.clone())
        .collect();

    add_all_descendants_within(&top_ids, all_symbols, &candidate_set, &mut result);

    tracing::info!(
        "Truncating to {} top-level symbols (total {} with children)",
        limit_usize,
        result.len()
    );

    result.sort();
    (result, true)
}

/// Like `add_all_descendants` but only includes indices in `allowed`.
fn add_all_descendants_within(
    parent_ids: &HashSet<String>,
    symbols: &[Symbol],
    allowed: &HashSet<usize>,
    result: &mut Vec<usize>,
) {
    let mut to_process: Vec<String> = parent_ids.iter().cloned().collect();
    let mut processed = HashSet::new();

    while let Some(parent_id) = to_process.pop() {
        if processed.contains(&parent_id) {
            continue;
        }
        processed.insert(parent_id.clone());

        for (idx, symbol) in symbols.iter().enumerate() {
            if !allowed.contains(&idx) {
                continue;
            }
            if let Some(ref pid) = symbol.parent_id {
                if pid == &parent_id && !result.contains(&idx) {
                    result.push(idx);
                    to_process.push(symbol.id.clone());
                }
            }
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Per-stage filter functions — used by tests to validate individual stages.
/// Production code uses `apply_all_filters` which runs the zero-clone pipeline.
#[cfg(test)]
pub fn apply_max_depth_filter(all_symbols: &[Symbol], max_depth: u32) -> Vec<Symbol> {
    max_depth_indices(all_symbols, max_depth)
        .into_iter()
        .map(|idx| all_symbols[idx].clone())
        .collect()
}

#[cfg(test)]
pub fn apply_target_filter(symbols: &[Symbol], target: &str) -> Vec<Symbol> {
    let all_indices: Vec<usize> = (0..symbols.len()).collect();
    target_indices(symbols, &all_indices, target)
        .into_iter()
        .map(|idx| symbols[idx].clone())
        .collect()
}

#[cfg(test)]
pub fn apply_limit_filter(symbols: &[Symbol], limit: u32) -> (Vec<Symbol>, bool) {
    let all_indices: Vec<usize> = (0..symbols.len()).collect();
    let (indices, was_truncated) = limit_indices(symbols, &all_indices, limit);
    let result = indices
        .into_iter()
        .map(|idx| symbols[idx].clone())
        .collect();
    (result, was_truncated)
}

/// Apply all filtering steps in sequence (single-clone pipeline)
///
/// 1. Apply max_depth filter → indices
/// 2. Apply target filter (if specified) → narrowed indices
/// 3. Apply limit filter (if specified) → final indices
/// 4. Clone symbols once from the original array
pub fn apply_all_filters(
    symbols: Vec<Symbol>,
    max_depth: u32,
    target: Option<&str>,
    limit: Option<u32>,
) -> (Vec<Symbol>, bool, usize) {
    let total_symbols = symbols.len();

    // Step 1: max_depth → indices into `symbols`
    let mut indices = max_depth_indices(&symbols, max_depth);

    // Step 2: target → narrowed indices
    if let Some(target) = target {
        indices = target_indices(&symbols, &indices, target);
        if indices.is_empty() {
            return (Vec::new(), false, total_symbols);
        }
    }

    // Step 3: limit → final indices + truncation flag
    let was_truncated = if let Some(limit) = limit {
        let (limited, truncated) = limit_indices(&symbols, &indices, limit);
        indices = limited;
        truncated
    } else {
        false
    };

    // Single clone at the end
    let result = indices
        .into_iter()
        .map(|idx| symbols[idx].clone())
        .collect();

    (result, was_truncated, total_symbols)
}
