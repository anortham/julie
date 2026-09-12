//! Web-mode call-path search: follows `Calls`, `WebRoute`, and `SqlQuery` edges so a
//! frontend client call traces through to its backend handler, and reports the
//! external endpoints of client calls that matched no handler.

use std::collections::HashMap;

use anyhow::Result;
use julie_facts::FactsReader;
use julie_facts::rows::{StructuralFactQuery, StructuralFactRow};
use julie_index::graph::{
    EdgeKind, HTTP_CLIENT_CALL_PATTERN_IDS, ROUTE_HANDLER_PATTERN_IDS, SymbolId,
};
use julie_index::snapshot::Snapshot;

use super::{
    CallPathResponse, ResolvedEndpoints, bfs_shortest_path, build_hops, call_site_line,
    found_response, not_found_response, resolve_endpoints,
};

fn meta(fact: &StructuralFactRow, key: &str) -> Option<String> {
    fact.metadata
        .as_ref()?
        .get(key)?
        .as_str()
        .map(str::to_string)
}

fn endpoint_label(fact: &StructuralFactRow) -> String {
    let path = meta(fact, "target_path").unwrap_or_else(|| "<unresolved target>".to_string());
    match meta(fact, "verb") {
        Some(verb) => format!("{verb} {path}"),
        None => path,
    }
}

fn web_facts(reader: &FactsReader<'_>) -> Result<Vec<StructuralFactRow>> {
    reader.structural_facts(&StructuralFactQuery {
        pattern_ids: HTTP_CLIENT_CALL_PATTERN_IDS
            .iter()
            .chain(ROUTE_HANDLER_PATTERN_IDS)
            .map(|pattern| (*pattern).to_string())
            .collect(),
        path_pattern: None,
        language: None,
        limit: i64::MAX as usize,
    })
}

/// Run a web-mode `call_path` traversal over `Calls`, `WebRoute`, and `SqlQuery` edges.
///
/// External endpoints are collected from every frontier symbol the BFS
/// explores, not only from symbols on the final path.
pub(super) fn run_web_call_path(
    snapshot: &Snapshot,
    endpoints: &ResolvedEndpoints,
    max_hops: u32,
    from: &str,
    to: &str,
) -> Result<CallPathResponse> {
    if endpoints.targets.contains(&endpoints.from) {
        return Ok(found_response(Vec::new(), Vec::new()));
    }
    let graph = snapshot.graph();
    let facts = snapshot.facts()?;
    let reader = facts.reader();
    let web_facts = web_facts(&reader)?;
    let mut calls_by_symbol: HashMap<SymbolId, Vec<(Option<SymbolId>, &StructuralFactRow)>> =
        HashMap::new();
    for route in graph.resolved_web_routes(&web_facts) {
        calls_by_symbol
            .entry(route.from)
            .or_default()
            .push((route.to, &web_facts[route.client_index]));
    }

    let mut external_endpoints = Vec::new();
    let search = bfs_shortest_path(endpoints.from, &endpoints.targets, max_hops, |id| {
        let edges = graph.outgoing(id);
        if let Some(calls) = calls_by_symbol.get(&id) {
            external_endpoints.extend(
                calls
                    .iter()
                    .filter(|(to, _)| to.is_none())
                    .map(|(_, call)| endpoint_label(call)),
            );
        }
        edges
            .iter()
            .filter_map(|(to, kind)| match kind {
                EdgeKind::Calls => Some((*to, "call")),
                EdgeKind::WebRoute => Some((*to, "http_call")),
                EdgeKind::SqlQuery => Some((*to, "sql_query")),
                _ => None,
            })
            .collect()
    });
    external_endpoints.sort();
    external_endpoints.dedup();

    let Some(target) = search.target else {
        return Ok(not_found_response(from, to, max_hops, external_endpoints));
    };
    let site_line = |from, to, label: &str| {
        if label != "http_call" {
            return call_site_line(graph, from, to);
        }
        calls_by_symbol
            .get(&from)
            .and_then(|calls| {
                calls
                    .iter()
                    .filter(|(target, _)| *target == Some(to))
                    .map(|(_, call)| call.span.start_line)
                    .min()
            })
            .unwrap_or(graph.symbol(from).span.start_line)
    };
    let hops = build_hops(
        graph,
        endpoints.from,
        target,
        &search.predecessors,
        &site_line,
    )?;
    Ok(found_response(hops, external_endpoints))
}

/// Programmatic (non-MCP) entry point for a web-mode call-path search: resolve
/// `from`/`to` by name (with optional file hints) and run the web-mode BFS.
pub fn web_call_path_by_name(
    snapshot: &Snapshot,
    from: &str,
    to: &str,
    from_file_path: Option<&str>,
    to_file_path: Option<&str>,
    max_hops: u32,
) -> Result<CallPathResponse> {
    let endpoints = resolve_endpoints(snapshot.graph(), from, to, from_file_path, to_file_path)?;
    run_web_call_path(snapshot, &endpoints, max_hops, from, to)
}
