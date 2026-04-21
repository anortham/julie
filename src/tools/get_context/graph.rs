use std::collections::{HashMap, HashSet};

use anyhow::Result;

use super::scoring::Pivot;
use crate::database::SymbolDatabase;
use crate::extractors::base::{RelationshipKind, Symbol};

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
pub fn expand_graph(pivots: &[Pivot], db: &SymbolDatabase) -> Result<GraphExpansion> {
    let pivot_symbols: Vec<Symbol> = pivots
        .iter()
        .map(|pivot| Symbol {
            id: pivot.result.id.clone(),
            name: pivot.result.name.clone(),
            kind: crate::extractors::base::SymbolKind::from_string(&pivot.result.kind),
            language: pivot.result.language.clone(),
            file_path: pivot.result.file_path.clone(),
            start_line: pivot.result.start_line,
            end_line: pivot.result.start_line,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            parent_id: None,
            signature: Some(pivot.result.signature.clone()),
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        })
        .collect();
    expand_graph_from_symbols(&pivot_symbols, db)
}

pub fn expand_graph_from_symbols(
    symbols: &[Symbol],
    db: &SymbolDatabase,
) -> Result<GraphExpansion> {
    if symbols.is_empty() {
        return Ok(GraphExpansion {
            neighbors: Vec::new(),
        });
    }

    let pivot_ids_vec: Vec<String> = symbols.iter().map(|symbol| symbol.id.clone()).collect();
    expand_graph_from_ids(symbols, &pivot_ids_vec, db)
}

fn expand_graph_from_ids(
    symbols: &[Symbol],
    pivot_ids_vec: &[String],
    db: &SymbolDatabase,
) -> Result<GraphExpansion> {
    if symbols.is_empty() {
        return Ok(GraphExpansion {
            neighbors: Vec::new(),
        });
    }

    let pivot_ids: HashSet<String> = pivot_ids_vec.iter().cloned().collect();
    let mut neighbor_map: HashMap<String, (RelationshipKind, NeighborDirection)> = HashMap::new();

    let incoming = db.get_relationships_to_symbols(pivot_ids_vec)?;
    for rel in incoming {
        let neighbor_id = &rel.from_symbol_id;
        if !pivot_ids.contains(neighbor_id) {
            neighbor_map
                .entry(neighbor_id.clone())
                .or_insert_with(|| (rel.kind, NeighborDirection::Incoming));
        }
    }

    let outgoing = db.get_outgoing_relationships_for_symbols(pivot_ids_vec)?;
    for rel in outgoing {
        let neighbor_id = &rel.to_symbol_id;
        if !pivot_ids.contains(neighbor_id) {
            neighbor_map
                .entry(neighbor_id.clone())
                .or_insert_with(|| (rel.kind, NeighborDirection::Outgoing));
        }
    }

    // Identifier-based neighbor expansion fills in languages whose calls and
    // type usages live in the identifiers table instead of relationships.
    if let Ok(identifier_edges) =
        crate::database::impact_graph::identifier_incoming_edges(db, symbols, &pivot_ids)
    {
        for edge in identifier_edges {
            neighbor_map
                .entry(edge.container_id)
                .or_insert((edge.relationship_kind, NeighborDirection::Incoming));
        }
    }

    if neighbor_map.is_empty() {
        return Ok(GraphExpansion {
            neighbors: Vec::new(),
        });
    }

    let neighbor_ids: Vec<String> = neighbor_map.keys().cloned().collect();
    let symbols = db.get_symbols_by_ids(&neighbor_ids)?;
    let id_refs: Vec<&str> = neighbor_ids.iter().map(|s| s.as_str()).collect();
    let ref_scores = db.get_reference_scores(&id_refs)?;

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

    neighbors.sort_by(|a, b| {
        b.reference_score
            .partial_cmp(&a.reference_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(GraphExpansion { neighbors })
}
