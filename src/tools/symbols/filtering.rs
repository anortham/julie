//! Symbol filtering logic - Extract symbols by depth, target, and limit
//!
//! This module contains reusable functions for filtering symbols that are used
//! by both primary and reference workspace symbol retrieval. The filtering
//! happens in three stages: depth filtering, target filtering, and limit filtering.

use std::collections::{HashMap, HashSet};
use tracing::debug;

use crate::extractors::base::Symbol;

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

/// Apply max_depth filtering to symbols
///
/// Returns filtered symbols in original order, keeping only those within
/// the maximum depth from top-level symbols.
pub fn apply_max_depth_filter(
    all_symbols: &[Symbol],
    max_depth: u32,
) -> Vec<Symbol> {
    let parent_to_children = build_parent_to_children(all_symbols);
    let top_level_indices = find_top_level_symbols(all_symbols);

    debug!(
        "📊 Symbol hierarchy: {} total, {} top-level",
        all_symbols.len(),
        top_level_indices.len()
    );

    let mut indices_to_include = Vec::new();
    collect_symbols_by_depth(
        &top_level_indices,
        0,
        max_depth,
        all_symbols,
        &parent_to_children,
        &mut indices_to_include,
    );

    debug!(
        "🔍 After max_depth={} filtering: {} -> {} symbols",
        max_depth,
        all_symbols.len(),
        indices_to_include.len()
    );

    indices_to_include
        .into_iter()
        .map(|idx| all_symbols[idx].clone())
        .collect()
}

/// Recursively add all descendants of a symbol
fn add_descendants(
    parent_id: &str,
    symbols: &[Symbol],
    result: &mut Vec<usize>,
) {
    for (idx, symbol) in symbols.iter().enumerate() {
        if let Some(ref pid) = symbol.parent_id {
            if pid == parent_id {
                result.push(idx);
                add_descendants(&symbol.id, symbols, result);
            }
        }
    }
}

/// Apply target filtering to symbols
///
/// Returns symbols matching the target name (case-insensitive partial match)
/// and all their descendants. Returns empty vec if no matches found.
pub fn apply_target_filter(
    symbols: &[Symbol],
    target: &str,
) -> Vec<Symbol> {
    let target_lower = target.to_lowercase();

    // Find symbols matching the target
    let matching_indices: Vec<usize> = symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| s.name.to_lowercase().contains(&target_lower))
        .map(|(idx, _)| idx)
        .collect();

    if matching_indices.is_empty() {
        return Vec::new();
    }

    // For each matching symbol, include it and all its descendants
    let mut final_indices = Vec::new();
    for &match_idx in &matching_indices {
        final_indices.push(match_idx);
        let matched_id = &symbols[match_idx].id;
        add_descendants(matched_id, symbols, &mut final_indices);
    }

    debug!(
        "🎯 After target='{}' filtering: {} symbols",
        target,
        final_indices.len()
    );

    final_indices
        .into_iter()
        .map(|idx| symbols[idx].clone())
        .collect()
}

/// Recursively add all descendants of symbols in a set
fn add_all_descendants(
    parent_ids: &HashSet<String>,
    symbols: &[Symbol],
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
            if let Some(ref pid) = symbol.parent_id {
                if pid == &parent_id && !result.contains(&idx) {
                    result.push(idx);
                    to_process.push(symbol.id.clone());
                }
            }
        }
    }
}

/// Apply limit filtering to symbols
///
/// The limit applies to top-level symbols. Returns all their children as well.
/// Returns (filtered_symbols, was_truncated).
pub fn apply_limit_filter(
    symbols: &[Symbol],
    limit: u32,
) -> (Vec<Symbol>, bool) {
    let limit_usize = limit as usize;

    // Count top-level symbols
    let top_level_in_list: Vec<usize> = symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| s.parent_id.is_none())
        .map(|(idx, _)| idx)
        .collect();

    if top_level_in_list.len() <= limit_usize {
        return (symbols.to_vec(), false);
    }

    // Limit applies to top-level symbols; include all their children
    let mut result = Vec::new();
    let mut top_level_count = 0;

    for (idx, symbol) in symbols.iter().enumerate() {
        if symbol.parent_id.is_none() {
            if top_level_count >= limit_usize {
                break;
            }
            top_level_count += 1;
            result.push(idx);
        }
    }

    // Add all children of included top-level symbols
    let top_level_ids: HashSet<String> = result
        .iter()
        .map(|&idx| symbols[idx].id.clone())
        .collect();

    add_all_descendants(&top_level_ids, symbols, &mut result);

    tracing::info!(
        "⚠️  Truncating to {} top-level symbols (total {} with children)",
        limit_usize,
        result.len()
    );

    result.sort();
    let filtered_symbols: Vec<Symbol> = result
        .into_iter()
        .map(|idx| symbols[idx].clone())
        .collect();

    (filtered_symbols, true)
}

/// Apply all filtering steps in sequence
///
/// 1. Apply max_depth filter
/// 2. Apply target filter (if specified)
/// 3. Apply limit filter (if specified)
pub fn apply_all_filters(
    symbols: Vec<Symbol>,
    max_depth: u32,
    target: Option<&str>,
    limit: Option<u32>,
) -> (Vec<Symbol>, bool, usize) {
    let total_symbols = symbols.len();

    // Step 1: Apply max_depth filtering
    let mut filtered = apply_max_depth_filter(&symbols, max_depth);

    // Step 2: Apply target filtering if specified
    if let Some(target) = target {
        filtered = apply_target_filter(&filtered, target);
        if filtered.is_empty() {
            return (Vec::new(), false, total_symbols);
        }
    }

    // Step 3: Apply limit filtering if specified
    let was_truncated = if let Some(limit) = limit {
        let (limited, truncated) = apply_limit_filter(&filtered, limit);
        filtered = limited;
        truncated
    } else {
        false
    };

    (filtered, was_truncated, total_symbols)
}
