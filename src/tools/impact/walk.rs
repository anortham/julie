use std::collections::{HashMap, HashSet};

use anyhow::Result;

use crate::database::SymbolDatabase;
use crate::database::impact_graph::identifier_incoming_edges;
use crate::extractors::{Relationship, RelationshipKind, Symbol};
use crate::tools::impact::ranking::relationship_priority;

#[derive(Debug, Clone)]
pub struct ImpactCandidate {
    pub symbol: Symbol,
    pub distance: u32,
    pub relationship_kind: RelationshipKind,
    pub reference_score: f64,
    pub via_symbol_name: String,
}

pub fn walk_impacts(
    db: &SymbolDatabase,
    seed_symbols: &[Symbol],
    max_depth: u32,
) -> Result<Vec<ImpactCandidate>> {
    if seed_symbols.is_empty() || max_depth == 0 {
        return Ok(Vec::new());
    }

    let mut frontier_symbols: Vec<Symbol> = seed_symbols.to_vec();
    let mut frontier_ids: Vec<String> = frontier_symbols
        .iter()
        .map(|symbol| symbol.id.clone())
        .collect();
    let mut frontier_names: HashMap<String, String> = frontier_symbols
        .iter()
        .map(|symbol| (symbol.id.clone(), symbol.name.clone()))
        .collect();
    let mut visited: HashSet<String> = frontier_ids.iter().cloned().collect();
    let mut impacts = Vec::new();

    for distance in 1..=max_depth {
        if frontier_ids.is_empty() {
            break;
        }

        let relationships = db.get_relationships_to_symbols(&frontier_ids)?;
        let mut best_by_source: HashMap<String, (RelationshipKind, Option<String>)> =
            HashMap::new();

        for rel in relationships {
            let Some(kind) = normalized_kind(&rel) else {
                continue;
            };
            if visited.contains(&rel.from_symbol_id) {
                continue;
            }

            let candidate = (kind, Some(rel.to_symbol_id.clone()));
            let should_replace = best_by_source
                .get(&rel.from_symbol_id)
                .is_none_or(|current| relation_order(&candidate) < relation_order(current));

            if should_replace {
                best_by_source.insert(rel.from_symbol_id.clone(), candidate);
            }
        }

        // Identifier-based expansion fills in callers that only appear in the
        // identifiers table (TypeScript type usages, calls, imports). Pick the
        // strongest identifier edge per source, then merge after relationship
        // rows so stored relationships keep priority over fallback edges.
        let identifier_edges = identifier_incoming_edges(db, &frontier_symbols, &visited)?;
        let fallback_target_id = if frontier_ids.len() == 1 {
            frontier_ids.first().cloned()
        } else {
            None
        };
        let mut best_identifier_by_source: HashMap<String, (RelationshipKind, Option<String>)> =
            HashMap::new();
        for edge in identifier_edges {
            if visited.contains(&edge.container_id) {
                continue;
            }
            let candidate = (
                edge.relationship_kind,
                edge.target_symbol_id.or_else(|| fallback_target_id.clone()),
            );
            let should_replace = best_identifier_by_source
                .get(&edge.container_id)
                .is_none_or(|current| relation_order(&candidate) < relation_order(current));

            if should_replace {
                best_identifier_by_source.insert(edge.container_id, candidate);
            }
        }
        for (source_id, candidate) in best_identifier_by_source {
            best_by_source.entry(source_id).or_insert(candidate);
        }

        if best_by_source.is_empty() {
            break;
        }

        let source_ids: Vec<String> = best_by_source.keys().cloned().collect();
        let symbols = db.get_symbols_by_ids(&source_ids)?;
        let symbol_map: HashMap<String, Symbol> = symbols
            .into_iter()
            .map(|symbol| (symbol.id.clone(), symbol))
            .collect();
        let source_id_refs: Vec<&str> = source_ids.iter().map(|id| id.as_str()).collect();
        let reference_scores = db.get_reference_scores(&source_id_refs)?;

        let mut depth_impacts = Vec::new();

        for source_id in source_ids {
            let Some(symbol) = symbol_map.get(&source_id).cloned() else {
                continue;
            };
            let Some((relationship_kind, target_id)) = best_by_source.remove(&source_id) else {
                continue;
            };

            visited.insert(source_id.clone());

            depth_impacts.push(ImpactCandidate {
                symbol,
                distance,
                relationship_kind,
                reference_score: reference_scores.get(&source_id).copied().unwrap_or(0.0),
                via_symbol_name: target_id
                    .as_ref()
                    .and_then(|id| frontier_names.get(id))
                    .cloned()
                    .unwrap_or_else(|| "changed code".to_string()),
            });
        }

        depth_impacts.sort_by(|left, right| impact_order(left).cmp(&impact_order(right)));
        frontier_symbols = depth_impacts
            .iter()
            .map(|candidate| candidate.symbol.clone())
            .collect();
        frontier_ids = frontier_symbols
            .iter()
            .map(|symbol| symbol.id.clone())
            .collect();
        frontier_names = frontier_symbols
            .iter()
            .map(|symbol| (symbol.id.clone(), symbol.name.clone()))
            .collect();
        impacts.extend(depth_impacts);
    }

    Ok(impacts)
}

fn normalized_kind(relationship: &Relationship) -> Option<RelationshipKind> {
    match relationship.kind {
        RelationshipKind::Calls => Some(RelationshipKind::Calls),
        RelationshipKind::Extends => Some(RelationshipKind::Extends),
        RelationshipKind::Overrides => Some(RelationshipKind::Overrides),
        RelationshipKind::Implements => Some(RelationshipKind::Implements),
        RelationshipKind::Instantiates => Some(RelationshipKind::Instantiates),
        RelationshipKind::References | RelationshipKind::Uses => Some(RelationshipKind::References),
        RelationshipKind::Imports => Some(RelationshipKind::Imports),
        _ => None,
    }
}

fn relation_order(candidate: &(RelationshipKind, Option<String>)) -> (u8, &str) {
    (
        relationship_priority(&candidate.0),
        candidate.1.as_deref().unwrap_or(""),
    )
}

fn impact_order(candidate: &ImpactCandidate) -> (u8, std::cmp::Reverse<u64>, &str, u32, &str) {
    (
        relationship_priority(&candidate.relationship_kind),
        std::cmp::Reverse(candidate.reference_score.to_bits()),
        candidate.symbol.file_path.as_str(),
        candidate.symbol.start_line,
        candidate.symbol.name.as_str(),
    )
}
