use std::collections::{HashMap, HashSet};

use super::scoring::Pivot;
use crate::snapshot_rows::to_symbol;
use julie_core::Symbol;
use julie_extractors::RelationshipKind;
use julie_index::graph::{EdgeKind, Graph, SymbolId};

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

/// Result of graph expansion, deduplicated neighbors sorted by reference score.
pub struct GraphExpansion {
    pub neighbors: Vec<Neighbor>,
}

/// Expand pivots into a graph of related neighbor symbols.
pub fn expand_graph(pivots: &[Pivot], graph: &Graph) -> GraphExpansion {
    let ids: Vec<SymbolId> = pivots
        .iter()
        .filter_map(|pivot| graph.symbol_by_row_id(&pivot.result.id))
        .collect();
    expand_graph_from_ids(&ids, graph)
}

pub fn expand_graph_from_symbols(symbols: &[Symbol], graph: &Graph) -> GraphExpansion {
    let ids: Vec<SymbolId> = symbols
        .iter()
        .filter_map(|symbol| graph.symbol_by_row_id(&symbol.id))
        .collect();
    expand_graph_from_ids(&ids, graph)
}

fn relationship_kind(kind: EdgeKind) -> RelationshipKind {
    match kind {
        EdgeKind::Calls => RelationshipKind::Calls,
        EdgeKind::Imports => RelationshipKind::Imports,
        EdgeKind::Implements => RelationshipKind::Implements,
        EdgeKind::Extends => RelationshipKind::Extends,
        EdgeKind::References | EdgeKind::Contains | EdgeKind::WebRoute | EdgeKind::SqlQuery => {
            RelationshipKind::References
        }
    }
}

fn expand_graph_from_ids(pivot_ids: &[SymbolId], graph: &Graph) -> GraphExpansion {
    let pivot_set: HashSet<SymbolId> = pivot_ids.iter().copied().collect();
    let mut neighbor_map: HashMap<SymbolId, (RelationshipKind, NeighborDirection)> = HashMap::new();

    for &pivot in pivot_ids {
        for &(from, kind) in graph.incoming(pivot) {
            if kind != EdgeKind::Contains && !pivot_set.contains(&from) {
                neighbor_map
                    .entry(from)
                    .or_insert_with(|| (relationship_kind(kind), NeighborDirection::Incoming));
            }
        }
    }
    for &pivot in pivot_ids {
        for &(to, kind) in graph.outgoing(pivot) {
            if kind != EdgeKind::Contains && !pivot_set.contains(&to) {
                neighbor_map
                    .entry(to)
                    .or_insert_with(|| (relationship_kind(kind), NeighborDirection::Outgoing));
            }
        }
    }

    let mut neighbors: Vec<Neighbor> = neighbor_map
        .into_iter()
        .map(|(id, (kind, direction))| Neighbor {
            symbol: to_symbol(graph, id),
            relationship_kind: kind,
            direction,
            reference_score: graph.reference_score(id),
        })
        .collect();

    neighbors.sort_by(|a, b| {
        b.reference_score
            .partial_cmp(&a.reference_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.file_path.cmp(&b.symbol.file_path))
            .then_with(|| a.symbol.start_line.cmp(&b.symbol.start_line))
            .then_with(|| a.symbol.name.cmp(&b.symbol.name))
            .then_with(|| a.symbol.id.cmp(&b.symbol.id))
    });

    GraphExpansion { neighbors }
}
