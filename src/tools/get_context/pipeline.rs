//! Main pipeline: search -> rank -> expand -> allocate -> format

use std::collections::{HashMap, HashSet};

use anyhow::Result;

use super::GetContextTool;
use crate::database::SymbolDatabase;
use crate::extractors::base::{RelationshipKind, Symbol};
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

/// Direction of a neighbor relative to the pivot symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeighborDirection {
    /// Symbol calls/uses/imports the pivot (incoming relationship).
    Incoming,
    /// Pivot calls/uses/imports this symbol (outgoing relationship).
    Outgoing,
}

/// A neighbor symbol discovered through graph expansion from a pivot.
pub struct Neighbor {
    pub symbol: Symbol,
    pub relationship_kind: RelationshipKind,
    pub direction: NeighborDirection,
    pub reference_score: f64,
}

/// Result of graph expansion — deduplicated neighbors sorted by reference_score.
pub struct GraphExpansion {
    pub neighbors: Vec<Neighbor>,
}

/// Expand pivots into a graph of related neighbor symbols.
///
/// For each pivot:
/// 1. Fetch incoming relationships (callers, implementors, importers)
/// 2. Fetch outgoing relationships (callees, types used, modules imported)
/// 3. Deduplicate neighbors across all pivots (each symbol appears once)
/// 4. Exclude pivot symbols themselves from the neighbor list
/// 5. Look up neighbor metadata and reference_scores
/// 6. Sort by reference_score descending (most important first)
pub fn expand_graph(pivots: &[Pivot], db: &SymbolDatabase) -> Result<GraphExpansion> {
    if pivots.is_empty() {
        return Ok(GraphExpansion {
            neighbors: Vec::new(),
        });
    }

    // Collect pivot IDs for exclusion
    let pivot_ids: HashSet<&str> = pivots.iter().map(|p| p.result.id.as_str()).collect();

    // For each neighbor, track: (relationship_kind, direction) — first seen wins
    let mut neighbor_map: HashMap<String, (RelationshipKind, NeighborDirection)> = HashMap::new();

    for pivot in pivots {
        let symbol_id = &pivot.result.id;

        // Incoming: other symbols that reference this pivot
        let incoming = db.get_relationships_to_symbol(symbol_id)?;
        for rel in incoming {
            let neighbor_id = &rel.from_symbol_id;
            if !pivot_ids.contains(neighbor_id.as_str()) {
                neighbor_map
                    .entry(neighbor_id.clone())
                    .or_insert_with(|| (rel.kind, NeighborDirection::Incoming));
            }
        }

        // Outgoing: symbols that this pivot references
        let outgoing = db.get_outgoing_relationships(symbol_id)?;
        for rel in outgoing {
            let neighbor_id = &rel.to_symbol_id;
            if !pivot_ids.contains(neighbor_id.as_str()) {
                neighbor_map
                    .entry(neighbor_id.clone())
                    .or_insert_with(|| (rel.kind, NeighborDirection::Outgoing));
            }
        }
    }

    if neighbor_map.is_empty() {
        return Ok(GraphExpansion {
            neighbors: Vec::new(),
        });
    }

    // Batch-fetch symbol metadata
    let neighbor_ids: Vec<String> = neighbor_map.keys().cloned().collect();
    let symbols = db.get_symbols_by_ids(&neighbor_ids)?;

    // Batch-fetch reference scores
    let id_refs: Vec<&str> = neighbor_ids.iter().map(|s| s.as_str()).collect();
    let ref_scores = db.get_reference_scores(&id_refs)?;

    // Build neighbors with metadata
    let mut neighbors: Vec<Neighbor> = symbols
        .into_iter()
        .filter_map(|sym| {
            let (kind, direction) = neighbor_map.remove(&sym.id)?;
            let reference_score = ref_scores.get(&sym.id).copied().unwrap_or(0.0);
            Some(Neighbor {
                symbol: sym,
                relationship_kind: kind,
                direction,
                reference_score,
            })
        })
        .collect();

    // Sort by reference_score descending
    neighbors.sort_by(|a, b| {
        b.reference_score
            .partial_cmp(&a.reference_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(GraphExpansion { neighbors })
}

pub async fn run(tool: &GetContextTool, _handler: &JulieServerHandler) -> Result<String> {
    // Will be implemented in subsequent tasks
    Ok(format!(
        "get_context not yet implemented for query: {}",
        tool.query
    ))
}
