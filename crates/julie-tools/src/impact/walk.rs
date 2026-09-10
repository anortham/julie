use std::collections::{HashMap, HashSet};

use anyhow::Result;
use julie_core::Symbol;
use julie_extractors::RelationshipKind;
use julie_facts::FactsReader;
use julie_facts::rows::StructuralFactQuery;
use julie_index::graph::{EdgeKind, Graph, SymbolId};
use julie_index::snapshot::Snapshot;

use crate::impact::ranking::relationship_priority;
use crate::snapshot_rows::to_symbol;

#[derive(Debug, Clone)]
pub struct ImpactCandidate {
    pub symbol: Symbol,
    pub distance: u32,
    pub relationship_kind: RelationshipKind,
    pub reference_score: f64,
    pub via_symbol_name: String,
}

/// A reverse web-edge caller of a seed symbol. For `WebRoute` edges the seed
/// is a route handler and the caller is the frontend symbol that issued the
/// client call; for `SqlQuery` edges the seed is a table symbol and the
/// caller is the routine that queries it. Populated only in `web` mode so
/// the default blast-radius output stays byte-identical.
#[derive(Debug, Clone)]
pub struct WebCaller {
    pub impact: ImpactCandidate,
    /// Endpoint or table label, e.g. `"GET /api/users/123"` or `"table:users"`.
    pub endpoint: String,
    /// The edge kind rendered as the `via` label (`http_call` or `sql_query`).
    pub via: &'static str,
}

const HTTP_CLIENT_CALL_PATTERN: &str = "http.client_request.v1";

/// `"VERB /path"` of the first HTTP client call inside `caller`, by line.
// ponytail: a caller with several client calls is labelled by its first one;
// per-call matching needs the handler's route template.
fn client_call_label(reader: &FactsReader<'_>, graph: &Graph, caller: SymbolId) -> Result<String> {
    let symbol = graph.symbol(caller);
    let mut rows = reader.structural_facts(&StructuralFactQuery {
        pattern_ids: vec![HTTP_CLIENT_CALL_PATTERN.to_string()],
        path_pattern: Some(symbol.path.clone()),
        language: None,
        limit: i64::MAX as usize,
    })?;
    rows.retain(|fact| fact.containing_ordinal == Some(symbol.ordinal));
    rows.sort_by_key(|fact| fact.span.start_line);
    Ok(rows
        .first()
        .map(|fact| {
            let meta = |key: &str| {
                fact.metadata
                    .as_ref()?
                    .get(key)?
                    .as_str()
                    .map(str::to_string)
            };
            let path = meta("target_path").unwrap_or_default();
            match meta("verb") {
                Some(verb) => format!("{verb} {path}"),
                None => path,
            }
        })
        .unwrap_or_default())
}

/// Reverse web-edge lookup: the symbols that reach `seeds` over `WebRoute`
/// or `SqlQuery` edges, one row per edge.
pub fn walk_web_callers(snapshot: &Snapshot, seeds: &[SymbolId]) -> Result<Vec<WebCaller>> {
    let graph = snapshot.graph();
    let facts = snapshot.facts()?;
    let reader = facts.reader();
    let mut callers = Vec::new();
    for &seed in seeds {
        for &(caller, kind) in graph.incoming(seed) {
            let (via, endpoint, relationship_kind) = match kind {
                EdgeKind::WebRoute => (
                    "http_call",
                    client_call_label(&reader, graph, caller)?,
                    RelationshipKind::Calls,
                ),
                EdgeKind::SqlQuery => (
                    "sql_query",
                    format!("table:{}", graph.symbol(seed).name),
                    RelationshipKind::References,
                ),
                _ => continue,
            };
            callers.push(WebCaller {
                impact: ImpactCandidate {
                    symbol: to_symbol(graph, caller),
                    distance: 1,
                    relationship_kind,
                    reference_score: graph.reference_score(caller),
                    via_symbol_name: endpoint.clone(),
                },
                endpoint,
                via,
            });
        }
    }
    callers.sort_by(|a, b| {
        a.impact
            .symbol
            .id
            .cmp(&b.impact.symbol.id)
            .then(a.via.cmp(b.via))
            .then(a.endpoint.cmp(&b.endpoint))
    });
    Ok(callers)
}

#[derive(Debug, Clone, Copy)]
pub struct WalkBudget {
    pub max_frontier_per_depth: usize,
}

impl Default for WalkBudget {
    fn default() -> Self {
        Self {
            max_frontier_per_depth: 250,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImpactTraversalPolicy {
    Default,
    Web,
}

#[derive(Debug, Clone, Default)]
pub struct WalkStats {
    pub depths_visited: u32,
    pub capped_depths: u32,
    pub total_edges_considered: usize,
}

pub fn walk_impacts(graph: &Graph, seeds: &[SymbolId], max_depth: u32) -> Vec<ImpactCandidate> {
    walk_impacts_with_budget(graph, seeds, max_depth, WalkBudget::default()).0
}

pub fn walk_impacts_with_budget(
    graph: &Graph,
    seeds: &[SymbolId],
    max_depth: u32,
    budget: WalkBudget,
) -> (Vec<ImpactCandidate>, WalkStats) {
    walk_impacts_with_policy(
        graph,
        seeds,
        max_depth,
        budget,
        ImpactTraversalPolicy::Default,
    )
}

fn relationship_kind(kind: EdgeKind, policy: ImpactTraversalPolicy) -> Option<RelationshipKind> {
    let web = policy == ImpactTraversalPolicy::Web;
    match kind {
        EdgeKind::Calls => Some(RelationshipKind::Calls),
        EdgeKind::References => Some(RelationshipKind::References),
        EdgeKind::Imports => Some(RelationshipKind::Imports),
        EdgeKind::Implements => Some(RelationshipKind::Implements),
        EdgeKind::Extends => Some(RelationshipKind::Extends),
        EdgeKind::Contains => None,
        EdgeKind::WebRoute => web.then_some(RelationshipKind::Calls),
        EdgeKind::SqlQuery => web.then_some(RelationshipKind::References),
    }
}

/// Bounded breadth-first walk over incoming edges. Each depth keeps one edge
/// per caller (strongest relationship kind, then lowest target id), sorts the
/// callers, and clips the frontier to `budget.max_frontier_per_depth`.
pub(super) fn walk_impacts_with_policy(
    graph: &Graph,
    seeds: &[SymbolId],
    max_depth: u32,
    budget: WalkBudget,
    policy: ImpactTraversalPolicy,
) -> (Vec<ImpactCandidate>, WalkStats) {
    let mut stats = WalkStats::default();
    if seeds.is_empty() || max_depth == 0 {
        return (Vec::new(), stats);
    }
    let max_frontier_per_depth = budget.max_frontier_per_depth.max(1);
    let mut frontier: Vec<SymbolId> = seeds.to_vec();
    let mut visited: HashSet<SymbolId> = frontier.iter().copied().collect();
    let mut impacts = Vec::new();

    for distance in 1..=max_depth {
        if frontier.is_empty() {
            break;
        }
        stats.depths_visited = distance;

        let mut best_by_source: HashMap<SymbolId, (RelationshipKind, SymbolId)> = HashMap::new();
        for &target in &frontier {
            for &(source, kind) in graph.incoming(target) {
                let Some(kind) = relationship_kind(kind, policy) else {
                    continue;
                };
                stats.total_edges_considered += 1;
                if visited.contains(&source) {
                    continue;
                }
                let candidate = (kind, target);
                let replace = best_by_source
                    .get(&source)
                    .is_none_or(|current| edge_order(&candidate) < edge_order(current));
                if replace {
                    best_by_source.insert(source, candidate);
                }
            }
        }
        if best_by_source.is_empty() {
            break;
        }

        let mut depth_impacts: Vec<(SymbolId, ImpactCandidate)> = best_by_source
            .into_iter()
            .map(|(source, (kind, target))| {
                visited.insert(source);
                let impact = ImpactCandidate {
                    symbol: to_symbol(graph, source),
                    distance,
                    relationship_kind: kind,
                    reference_score: graph.reference_score(source),
                    via_symbol_name: graph.symbol(target).name.clone(),
                };
                (source, impact)
            })
            .collect();
        depth_impacts.sort_by(|left, right| impact_order(&left.1).cmp(&impact_order(&right.1)));
        if depth_impacts.len() > max_frontier_per_depth {
            stats.capped_depths += 1;
            depth_impacts.truncate(max_frontier_per_depth);
        }

        frontier = depth_impacts.iter().map(|(source, _)| *source).collect();
        impacts.extend(depth_impacts.into_iter().map(|(_, impact)| impact));
    }

    (impacts, stats)
}

fn edge_order(candidate: &(RelationshipKind, SymbolId)) -> (u8, SymbolId) {
    (relationship_priority(&candidate.0), candidate.1)
}

fn impact_order(
    candidate: &ImpactCandidate,
) -> (u8, std::cmp::Reverse<u64>, &str, u32, &str, &str) {
    (
        relationship_priority(&candidate.relationship_kind),
        std::cmp::Reverse(candidate.reference_score.to_bits()),
        candidate.symbol.file_path.as_str(),
        candidate.symbol.start_line,
        candidate.symbol.name.as_str(),
        candidate.symbol.id.as_str(),
    )
}
