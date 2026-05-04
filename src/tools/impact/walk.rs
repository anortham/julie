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

#[derive(Debug, Clone, Copy)]
pub struct WalkBudget {
    pub max_frontier_per_depth: usize,
    pub max_identifier_fanout_per_name: usize,
}

impl Default for WalkBudget {
    fn default() -> Self {
        Self {
            max_frontier_per_depth: 250,
            max_identifier_fanout_per_name: 100,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WalkStats {
    pub depths_visited: u32,
    pub capped_depths: u32,
    pub dropped_identifier_edges: usize,
    pub total_relationship_edges_considered: usize,
    pub total_identifier_edges_considered: usize,
}

pub fn walk_impacts(
    db: &SymbolDatabase,
    seed_symbols: &[Symbol],
    max_depth: u32,
) -> Result<Vec<ImpactCandidate>> {
    let (impacts, _stats) =
        walk_impacts_with_budget(db, seed_symbols, max_depth, WalkBudget::default())?;
    Ok(impacts)
}

pub fn walk_impacts_with_budget(
    db: &SymbolDatabase,
    seed_symbols: &[Symbol],
    max_depth: u32,
    budget: WalkBudget,
) -> Result<(Vec<ImpactCandidate>, WalkStats)> {
    if seed_symbols.is_empty() || max_depth == 0 {
        return Ok((Vec::new(), WalkStats::default()));
    }

    let max_frontier_per_depth = budget.max_frontier_per_depth.max(1);
    let max_identifier_fanout_per_name = budget.max_identifier_fanout_per_name.max(1);

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
    let mut stats = WalkStats::default();

    for distance in 1..=max_depth {
        if frontier_ids.is_empty() {
            break;
        }
        stats.depths_visited = distance;

        let relationships = db.get_relationships_to_symbols(&frontier_ids)?;
        stats.total_relationship_edges_considered += relationships.len();
        let mut best_by_source: HashMap<String, CandidateEdge> = HashMap::new();

        for rel in relationships {
            let Some(kind) = normalized_kind(&rel) else {
                continue;
            };
            if visited.contains(&rel.from_symbol_id) {
                continue;
            }

            let candidate = CandidateEdge {
                relationship_kind: kind,
                target_id: Some(rel.to_symbol_id.clone()),
                resolved_target: true,
            };
            let should_replace = best_by_source
                .get(&rel.from_symbol_id)
                .is_none_or(|current| relation_order(&candidate) < relation_order(current));
            if should_replace {
                best_by_source.insert(rel.from_symbol_id.clone(), candidate);
            }
        }

        // Identifier-based expansion fills in callers that only appear in the
        // identifiers table (TypeScript type usages, calls, imports). Pick the
        // strongest identifier edge per source, cap per-name fanout, then merge
        // after relationship rows so stored relationships keep priority.
        let identifier_edges = identifier_incoming_edges(db, &frontier_symbols, &visited)?;
        stats.total_identifier_edges_considered += identifier_edges.len();
        let fallback_target_id = if frontier_ids.len() == 1 {
            frontier_ids.first().cloned()
        } else {
            None
        };

        let mut best_identifier_by_source: HashMap<String, CandidateEdge> = HashMap::new();
        for edge in identifier_edges {
            if visited.contains(&edge.container_id) {
                continue;
            }
            let target_id = edge.target_symbol_id.or_else(|| fallback_target_id.clone());
            let candidate = CandidateEdge {
                relationship_kind: edge.relationship_kind,
                target_id: target_id.clone(),
                resolved_target: target_id.is_some(),
            };
            let should_replace = best_identifier_by_source
                .get(&edge.container_id)
                .is_none_or(|current| relation_order(&candidate) < relation_order(current));
            if should_replace {
                best_identifier_by_source.insert(edge.container_id, candidate);
            }
        }

        let mut identifier_candidates: Vec<(String, CandidateEdge)> =
            best_identifier_by_source.into_iter().collect();
        identifier_candidates.sort_by(|left, right| {
            relation_order(&left.1)
                .cmp(&relation_order(&right.1))
                .then_with(|| left.1.target_id.cmp(&right.1.target_id))
                .then_with(|| left.0.cmp(&right.0))
        });

        let mut identifier_name_counts: HashMap<String, usize> = HashMap::new();
        for (source_id, candidate) in identifier_candidates {
            if best_by_source.contains_key(&source_id) {
                continue;
            }
            let fanout_name = candidate
                .target_id
                .as_ref()
                .and_then(|target_id| frontier_names.get(target_id))
                .cloned()
                .unwrap_or_else(|| "__unresolved__".to_string());
            let fanout_count = identifier_name_counts.entry(fanout_name).or_insert(0);
            if *fanout_count >= max_identifier_fanout_per_name {
                stats.dropped_identifier_edges += 1;
                continue;
            }
            *fanout_count += 1;
            best_by_source.insert(source_id, candidate);
        }

        if best_by_source.is_empty() {
            break;
        }

        let mut source_ids: Vec<String> = best_by_source.keys().cloned().collect();
        source_ids.sort();
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
            let Some(candidate_edge) = best_by_source.remove(&source_id) else {
                continue;
            };
            visited.insert(source_id.clone());

            depth_impacts.push(ImpactWithResolution {
                resolved_target: candidate_edge.resolved_target,
                impact: ImpactCandidate {
                    symbol,
                    distance,
                    relationship_kind: candidate_edge.relationship_kind,
                    reference_score: reference_scores.get(&source_id).copied().unwrap_or(0.0),
                    via_symbol_name: candidate_edge
                        .target_id
                        .as_ref()
                        .and_then(|id| frontier_names.get(id))
                        .cloned()
                        .unwrap_or_else(|| "changed code".to_string()),
                },
            });
        }

        depth_impacts.sort_by(|left, right| impact_order(left).cmp(&impact_order(right)));
        if depth_impacts.len() > max_frontier_per_depth {
            stats.capped_depths += 1;
            depth_impacts.truncate(max_frontier_per_depth);
        }

        frontier_symbols = depth_impacts
            .iter()
            .map(|candidate| candidate.impact.symbol.clone())
            .collect();
        frontier_ids = frontier_symbols
            .iter()
            .map(|symbol| symbol.id.clone())
            .collect();
        frontier_names = frontier_symbols
            .iter()
            .map(|symbol| (symbol.id.clone(), symbol.name.clone()))
            .collect();
        impacts.extend(depth_impacts.into_iter().map(|candidate| candidate.impact));
    }

    Ok((impacts, stats))
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

#[derive(Debug, Clone)]
struct CandidateEdge {
    relationship_kind: RelationshipKind,
    target_id: Option<String>,
    resolved_target: bool,
}

#[derive(Debug, Clone)]
struct ImpactWithResolution {
    impact: ImpactCandidate,
    resolved_target: bool,
}

fn relation_order(candidate: &CandidateEdge) -> (u8, u8, &str) {
    (
        relationship_priority(&candidate.relationship_kind),
        if candidate.resolved_target { 0 } else { 1 },
        candidate.target_id.as_deref().unwrap_or(""),
    )
}

fn impact_order(
    candidate: &ImpactWithResolution,
) -> (u8, u8, std::cmp::Reverse<u64>, &str, u32, &str, &str) {
    (
        relationship_priority(&candidate.impact.relationship_kind),
        if candidate.resolved_target { 0 } else { 1 },
        std::cmp::Reverse(candidate.impact.reference_score.to_bits()),
        candidate.impact.symbol.file_path.as_str(),
        candidate.impact.symbol.start_line,
        candidate.impact.symbol.name.as_str(),
        candidate.impact.symbol.id.as_str(),
    )
}
