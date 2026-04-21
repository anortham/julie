//! Second-hop graph expansion helpers.
//!
//! When a pivot has few code neighbors, we extend the graph one more hop from
//! the initial expansion. These helpers decide when to expand, select seeds,
//! and merge the new hop into the existing `GraphExpansion`.

use std::collections::HashSet;

use super::pipeline::GraphExpansion;
use super::task_signals::TaskSignals;
use crate::extractors::base::Symbol;
use crate::search::scoring::is_test_path;

/// Decide whether a second-hop expansion should run for this pivot set.
///
/// Only expand when the caller opted into `max_hops >= 2` and the first-hop
/// produced fewer than four non-test neighbors — otherwise the first hop is
/// already informative enough.
pub(super) fn should_expand_second_hop(signals: &TaskSignals, expansion: &GraphExpansion) -> bool {
    if signals.max_hops < 2 {
        return false;
    }

    let code_neighbors = expansion
        .neighbors
        .iter()
        .filter(|neighbor| !is_test_path(&neighbor.symbol.file_path))
        .count();

    code_neighbors < 4
}

/// Pick up to three first-hop neighbors to use as second-hop seeds.
///
/// Test files are only considered when the task signals ask for them;
/// otherwise we stick to code neighbors so we keep widening the implementation
/// graph rather than the test surface.
pub(super) fn select_second_hop_seeds(
    expansion: &GraphExpansion,
    prefer_tests: bool,
) -> Vec<Symbol> {
    expansion
        .neighbors
        .iter()
        .filter(|neighbor| prefer_tests || !is_test_path(&neighbor.symbol.file_path))
        .take(3)
        .map(|neighbor| neighbor.symbol.clone())
        .collect()
}

/// Merge two `GraphExpansion`s, keeping the first occurrence of each symbol id.
pub(super) fn merge_expansions(
    primary: GraphExpansion,
    secondary: GraphExpansion,
) -> GraphExpansion {
    let mut seen = HashSet::new();
    let mut neighbors = Vec::new();

    for neighbor in primary.neighbors.into_iter().chain(secondary.neighbors) {
        if seen.insert(neighbor.symbol.id.clone()) {
            neighbors.push(neighbor);
        }
    }

    GraphExpansion { neighbors }
}
