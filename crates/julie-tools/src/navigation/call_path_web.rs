//! Web-mode call-path BFS, extracted from `call_path.rs` for file-size hygiene.
//!
//! Follows Calls/Instantiates/Overrides AND derived `http_call`/`sql_query` web
//! edges (so a frontend client-call traces through to its backend handler, and
//! a SQL routine traces through to its table). Reports external endpoints
//! reached by unmatched client calls even when no in-workspace symbol path is
//! found.

use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{Result, anyhow};
use julie_core::database::SymbolDatabase;
use julie_core::database::WebEdgeKind;
use julie_extractors::{RelationshipKind, Symbol};

use super::{CallPathHop, CallPathResponse, ResolvedEndpoints, edge_label, resolve_endpoints};

/// A uniform link in a web-mode path: either a stored relationship or a
/// derived web edge. Carries just what `build_web_hops` needs to render a hop.
#[derive(Clone)]
pub(super) struct EdgeLink {
    pub(super) from_id: String,
    pub(super) to_id: String,
    pub(super) label: &'static str,
    pub(super) file: String,
    pub(super) line: u32,
}

#[derive(Clone)]
pub(super) struct WebPathSearchResult {
    pub(super) target_id: Option<String>,
    pub(super) predecessor: HashMap<String, EdgeLink>,
    /// External endpoint labels reached by unmatched client calls during BFS
    /// (e.g. `"GET /api/foo"`), in sorted order for deterministic output.
    pub(super) external_endpoints: Vec<String>,
}

pub(super) fn web_edge_label(kind: WebEdgeKind) -> &'static str {
    match kind {
        WebEdgeKind::HttpCall => "http_call",
        WebEdgeKind::SqlQuery => "sql_query",
    }
}

/// Expand `frontier` one BFS level, returning outgoing edges as `EdgeLink`s
/// (relationships first, then web edges) and collecting any external
/// endpoints encountered (unmatched http_call edges with `to_external`).
pub(super) fn expand_web_frontier(
    db: &SymbolDatabase,
    frontier: &[String],
    visited: &HashSet<String>,
    external_endpoints: &mut Vec<String>,
) -> Result<Vec<EdgeLink>> {
    let mut links = Vec::new();

    // Stored relationships: Calls / Instantiates / Overrides (same filter as
    // the default BFS).
    let mut relationships = db.get_outgoing_relationships_for_symbols(frontier)?;
    relationships.retain(|rel| {
        matches!(
            rel.kind,
            RelationshipKind::Calls | RelationshipKind::Instantiates | RelationshipKind::Overrides
        )
    });
    for rel in relationships {
        links.push(EdgeLink {
            from_id: rel.from_symbol_id.clone(),
            to_id: rel.to_symbol_id.clone(),
            label: edge_label(&rel.kind),
            file: rel.file_path.clone(),
            line: rel.line_number,
        });
    }

    // Derived web edges (Phase 1: http_call; Phase 2: sql_query). Batch-load
    // for the whole frontier (mirrors relationships) to avoid N+1 queries.
    // Follow edges that resolve to an in-workspace symbol; record external
    // endpoints for unmatched calls.
    for edge in db.web_edges_from_symbols(frontier)? {
        match &edge.to_symbol_id {
            Some(to_id) => {
                links.push(EdgeLink {
                    from_id: edge.from_symbol_id.clone(),
                    to_id: to_id.clone(),
                    label: web_edge_label(edge.kind),
                    file: edge.file_path.clone(),
                    line: edge.line_number,
                });
            }
            None => {
                if let Some(external) = &edge.to_external {
                    external_endpoints.push(external.clone());
                }
            }
        }
    }

    // Deterministic order: by (from_id, label, line, to_id) so the BFS and the
    // resulting path are stable across runs.
    links.sort_by(|a, b| {
        a.from_id
            .cmp(&b.from_id)
            .then(a.label.cmp(b.label))
            .then(a.line.cmp(&b.line))
            .then(a.to_id.cmp(&b.to_id))
    });
    let _ = visited; // visited is enforced by the caller
    Ok(links)
}

pub(super) fn bfs_web_shortest_path(
    db: &SymbolDatabase,
    start_id: &str,
    targets: &HashSet<String>,
    max_hops: u32,
) -> Result<WebPathSearchResult> {
    let mut external_endpoints: Vec<String> = Vec::new();

    if targets.contains(start_id) {
        return Ok(WebPathSearchResult {
            target_id: Some(start_id.to_string()),
            predecessor: HashMap::new(),
            external_endpoints,
        });
    }

    let mut visited = HashSet::from([start_id.to_string()]);
    let mut frontier = vec![start_id.to_string()];
    let mut predecessor: HashMap<String, EdgeLink> = HashMap::new();

    for _depth in 0..max_hops {
        if frontier.is_empty() {
            break;
        }
        let links = expand_web_frontier(db, &frontier, &visited, &mut external_endpoints)?;
        let mut next_frontier = Vec::new();
        for link in links {
            if !visited.insert(link.to_id.clone()) {
                continue;
            }
            predecessor.insert(link.to_id.clone(), link.clone());
            if targets.contains(&link.to_id) {
                let mut ext = external_endpoints;
                ext.sort();
                ext.dedup();
                return Ok(WebPathSearchResult {
                    target_id: Some(link.to_id.clone()),
                    predecessor,
                    external_endpoints: ext,
                });
            }
            next_frontier.push(link.to_id);
        }
        frontier = next_frontier;
    }

    let mut ext = external_endpoints;
    ext.sort();
    ext.dedup();
    Ok(WebPathSearchResult {
        target_id: None,
        predecessor,
        external_endpoints: ext,
    })
}

pub(super) fn build_web_hops(
    db: &SymbolDatabase,
    start_symbol: &Symbol,
    target_id: &str,
    predecessor: &HashMap<String, EdgeLink>,
) -> Result<Vec<CallPathHop>> {
    let mut chain: VecDeque<EdgeLink> = VecDeque::new();
    let mut current_id = target_id.to_string();
    while current_id != start_symbol.id {
        let link = predecessor
            .get(&current_id)
            .ok_or_else(|| anyhow!("Path reconstruction failed at '{}'", current_id))?;
        chain.push_front(link.clone());
        current_id = link.from_id.clone();
    }

    let mut symbol_ids = vec![start_symbol.id.clone()];
    for link in &chain {
        symbol_ids.push(link.to_id.clone());
        symbol_ids.push(link.from_id.clone());
    }
    symbol_ids.sort();
    symbol_ids.dedup();

    let symbol_map = db
        .get_symbols_by_ids(&symbol_ids)?
        .into_iter()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect::<HashMap<_, _>>();

    let mut hops = Vec::new();
    for link in chain {
        let from_symbol = symbol_map
            .get(&link.from_id)
            .ok_or_else(|| anyhow!("Missing symbol '{}'", link.from_id))?;
        let to_symbol = symbol_map
            .get(&link.to_id)
            .ok_or_else(|| anyhow!("Missing symbol '{}'", link.to_id))?;
        hops.push(CallPathHop {
            from: from_symbol.name.clone(),
            to: to_symbol.name.clone(),
            edge: link.label.to_string(),
            file: format!("{}:{}", link.file, link.line),
            target_file: to_symbol.file_path.clone(),
            target_start_line: to_symbol.start_line,
        });
    }
    Ok(hops)
}

/// Run a web-mode `call_path` traversal. Follows Calls/Instantiates/Overrides
/// AND derived `http_call`/`sql_query` edges. Reports external endpoints
/// reached by unmatched client calls even when no symbol path is found.
///
/// Scope note: external endpoints are collected from every frontier node the
/// BFS explores that has an unmatched `http_call` edge, not only from nodes on
/// the final shortest path. This surfaces all external services reachable from
/// the `from` symbol's call cone, at the cost of some noise when a path is
/// also found. Tightening to on-path-only is a tracked follow-up.
pub(super) fn run_web_call_path(
    db: &SymbolDatabase,
    endpoints: &ResolvedEndpoints,
    max_hops: u32,
    from: &str,
    to: &str,
) -> Result<CallPathResponse> {
    if endpoints.targets.contains(&endpoints.from.id) {
        return Ok(CallPathResponse {
            found: true,
            hops: 0,
            path: Vec::new(),
            diagnostic: None,
            external_endpoints: Vec::new(),
        });
    }
    let search = bfs_web_shortest_path(db, &endpoints.from.id, &endpoints.targets, max_hops)?;
    if let Some(target_id) = search.target_id.as_deref() {
        let hops = build_web_hops(db, &endpoints.from, target_id, &search.predecessor)?;
        return Ok(CallPathResponse {
            found: true,
            hops: hops.len() as u32,
            path: hops,
            diagnostic: None,
            external_endpoints: search.external_endpoints,
        });
    }
    Ok(CallPathResponse {
        found: false,
        hops: 0,
        path: Vec::new(),
        diagnostic: Some(format!(
            "No path found from '{}' to '{}' within {} hops.",
            from, to, max_hops
        )),
        external_endpoints: search.external_endpoints,
    })
}

/// Programmatic (non-MCP) entry point for a web-mode call-path search: resolve
/// `from`/`to` by name (with optional file hints) and run the web-mode BFS.
/// Exposed for callers and tests that want the trace without the async MCP
/// layer; the MCP `call_tool` path wraps the same logic.
pub fn web_call_path_by_name(
    db: &SymbolDatabase,
    from: &str,
    to: &str,
    from_file_path: Option<&str>,
    to_file_path: Option<&str>,
    max_hops: u32,
) -> Result<CallPathResponse> {
    let endpoints = resolve_endpoints(db, from, to, from_file_path, to_file_path)?;
    run_web_call_path(db, &endpoints, max_hops, from, to)
}
