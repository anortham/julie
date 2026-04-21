use std::collections::{HashMap, HashSet};

use anyhow::Result;

use crate::database::SymbolDatabase;
use crate::extractors::{Relationship, RelationshipKind, Symbol};

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

    let mut frontier_ids: Vec<String> = seed_symbols
        .iter()
        .map(|symbol| symbol.id.clone())
        .collect();
    let mut frontier_names: HashMap<String, String> = seed_symbols
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
        let mut best_by_source: HashMap<String, (RelationshipKind, String)> = HashMap::new();

        for rel in relationships {
            let Some(kind) = normalized_kind(&rel) else {
                continue;
            };
            if visited.contains(&rel.from_symbol_id) {
                continue;
            }

            let candidate = (kind, rel.to_symbol_id.clone());
            let should_replace = best_by_source
                .get(&rel.from_symbol_id)
                .is_none_or(|current| relation_order(&candidate) < relation_order(current));

            if should_replace {
                best_by_source.insert(rel.from_symbol_id.clone(), candidate);
            }
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

        let mut next_frontier_ids = Vec::new();
        let mut next_frontier_names = HashMap::new();
        let mut depth_impacts = Vec::new();

        for source_id in source_ids {
            let Some(symbol) = symbol_map.get(&source_id).cloned() else {
                continue;
            };
            let Some((relationship_kind, target_id)) = best_by_source.remove(&source_id) else {
                continue;
            };

            visited.insert(source_id.clone());
            next_frontier_names.insert(source_id.clone(), symbol.name.clone());
            next_frontier_ids.push(source_id.clone());

            depth_impacts.push(ImpactCandidate {
                symbol,
                distance,
                relationship_kind,
                reference_score: reference_scores.get(&source_id).copied().unwrap_or(0.0),
                via_symbol_name: frontier_names
                    .get(&target_id)
                    .cloned()
                    .unwrap_or_else(|| "changed code".to_string()),
            });
        }

        depth_impacts.sort_by(|left, right| impact_order(left).cmp(&impact_order(right)));
        impacts.extend(depth_impacts);
        frontier_ids = next_frontier_ids;
        frontier_names = next_frontier_names;
    }

    Ok(impacts)
}

fn normalized_kind(relationship: &Relationship) -> Option<RelationshipKind> {
    match relationship.kind {
        RelationshipKind::Calls => Some(RelationshipKind::Calls),
        RelationshipKind::Overrides => Some(RelationshipKind::Overrides),
        RelationshipKind::Implements => Some(RelationshipKind::Implements),
        RelationshipKind::Instantiates => Some(RelationshipKind::Instantiates),
        RelationshipKind::References | RelationshipKind::Uses => Some(RelationshipKind::References),
        RelationshipKind::Imports => Some(RelationshipKind::Imports),
        _ => None,
    }
}

fn relation_order(candidate: &(RelationshipKind, String)) -> (u8, &str) {
    (relationship_priority(&candidate.0), candidate.1.as_str())
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

fn relationship_priority(kind: &RelationshipKind) -> u8 {
    match kind {
        RelationshipKind::Calls => 0,
        RelationshipKind::Overrides => 1,
        RelationshipKind::Implements => 2,
        RelationshipKind::Instantiates => 3,
        RelationshipKind::References => 4,
        RelationshipKind::Imports => 5,
        _ => 6,
    }
}
