//! Web-mode call-path search: follows `Calls`, `WebRoute`, and `SqlQuery` edges so a
//! frontend client call traces through to its backend handler, and reports the
//! external endpoints of client calls that matched no handler.

use anyhow::Result;
use julie_facts::FactsReader;
use julie_facts::rows::{StructuralFactQuery, StructuralFactRow, SymbolRow};
use julie_index::graph::EdgeKind;
use julie_index::snapshot::Snapshot;

use super::{
    CallPathResponse, ResolvedEndpoints, bfs_shortest_path, build_hops, call_site_line,
    found_response, not_found_response, resolve_endpoints,
};

const HTTP_CLIENT_CALL_PATTERN: &str = "http.client_request.v1";

fn endpoint_label(fact: &StructuralFactRow) -> String {
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
}

/// `(line, "VERB /path")` of every HTTP client call inside `symbol`.
fn client_calls(reader: &FactsReader<'_>, symbol: &SymbolRow) -> Result<Vec<(u32, String)>> {
    let facts = reader.structural_facts(&StructuralFactQuery {
        pattern_ids: vec![HTTP_CLIENT_CALL_PATTERN.to_string()],
        path_pattern: Some(symbol.path.clone()),
        language: None,
        limit: i64::MAX as usize,
    })?;
    Ok(facts
        .iter()
        .filter(|fact| fact.containing_ordinal == Some(symbol.ordinal))
        .map(|fact| (fact.span.start_line, endpoint_label(fact)))
        .collect())
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

    let mut external_endpoints = Vec::new();
    let mut failure = None;
    let search = bfs_shortest_path(endpoints.from, &endpoints.targets, max_hops, |id| {
        let edges = graph.outgoing(id);
        // ponytail: a symbol with any matched route reports none of its client
        // calls as external; per-call matching needs the handlers' templates.
        if !edges.iter().any(|(_, kind)| *kind == EdgeKind::WebRoute) {
            match client_calls(&reader, graph.symbol(id)) {
                Ok(calls) => external_endpoints.extend(calls.into_iter().map(|(_, label)| label)),
                Err(error) => failure = Some(error),
            }
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
    if let Some(error) = failure {
        return Err(error);
    }
    external_endpoints.sort();
    external_endpoints.dedup();

    let Some(target) = search.target else {
        return Ok(not_found_response(from, to, max_hops, external_endpoints));
    };
    let site_line = |from, to, label: &str| {
        if label != "http_call" {
            return call_site_line(graph, from, to);
        }
        let symbol = graph.symbol(from);
        client_calls(&reader, symbol)
            .ok()
            .and_then(|calls| calls.first().map(|(line, _)| *line))
            .unwrap_or(symbol.span.start_line)
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
