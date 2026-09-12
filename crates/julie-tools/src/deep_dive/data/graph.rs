//! Neighbor sections of a symbol read from the snapshot graph: the referencing
//! and referenced symbols with the source lines that name the other side.

use std::collections::HashSet;

use julie_core::shared::NOISE_CALLEE_NAMES;
use julie_extractors::RelationshipKind;
use julie_index::analysis::test_roles::is_test_related;
use julie_index::graph::{EdgeKind, Graph, SymbolId};
use julie_index::search::scoring::is_test_path;

use super::types::RefEntry;
use crate::navigation::sites::{Site, reference_sites};
use crate::snapshot_rows::to_symbol;

const TEST_REF_CAP: usize = 10;

/// Non-structural edges of `id`, one per neighbor, keeping the first edge kind.
fn neighbors(edges: &[(SymbolId, EdgeKind)]) -> Vec<(SymbolId, EdgeKind)> {
    let mut seen = HashSet::new();
    edges
        .iter()
        .filter(|(_, kind)| *kind != EdgeKind::Contains)
        .filter(|(id, _)| seen.insert(*id))
        .copied()
        .collect()
}

fn edge_relationship_kind(kind: EdgeKind) -> RelationshipKind {
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

/// One entry per line in `from` that names `to`, showing `shown`; the edge
/// itself stands in when no row names the line.
fn ref_entries(
    graph: &Graph,
    from: SymbolId,
    to: SymbolId,
    edge: EdgeKind,
    shown: SymbolId,
) -> Vec<RefEntry> {
    let from_row = graph.symbol(from);
    let mut sites = reference_sites(graph, from, to);
    if sites.is_empty() {
        sites.push(Site {
            id: format!("graph_{}_{}", from_row.id, graph.symbol(to).id),
            line: from_row.span.start_line,
            span: None,
            exact: false,
            kind: edge_relationship_kind(edge),
            identifier_kind: None,
            confidence: 1.0,
            metadata: None,
        });
    }
    sites
        .into_iter()
        .map(|site| RefEntry {
            kind: site.kind,
            file_path: from_row.path.clone(),
            line_number: site.line,
            symbol: Some(to_symbol(graph, shown)),
        })
        .collect()
}

/// Every site that references `id`, sorted by file then line.
pub(crate) fn incoming_refs(graph: &Graph, id: SymbolId) -> Vec<RefEntry> {
    let mut entries: Vec<RefEntry> = neighbors(graph.incoming(id))
        .into_iter()
        .flat_map(|(from, edge)| ref_entries(graph, from, id, edge, from))
        .collect();
    entries.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then(a.line_number.cmp(&b.line_number))
    });
    entries
}

/// Every site in `id` that references another symbol, sorted by line, without
/// the noise callees (`new`, `len`, `from`, ...) that resolve too loosely.
pub(crate) fn outgoing_refs(graph: &Graph, id: SymbolId) -> Vec<RefEntry> {
    let mut entries: Vec<RefEntry> = neighbors(graph.outgoing(id))
        .into_iter()
        .flat_map(|(to, edge)| ref_entries(graph, id, to, edge, to))
        .filter(|entry| {
            let name = entry.symbol.as_ref().map_or("", |s| s.name.as_str());
            !NOISE_CALLEE_NAMES.contains(&name)
        })
        .collect();
    entries.sort_by_key(|entry| entry.line_number);
    entries
}

/// Sites in test symbols or test files that reference `id`, one per
/// (file, symbol), at most ten.
pub(crate) fn test_refs(graph: &Graph, id: SymbolId) -> Vec<RefEntry> {
    let mut seen = HashSet::new();
    let mut refs: Vec<RefEntry> = neighbors(graph.incoming(id))
        .into_iter()
        .filter(|(from, _)| {
            is_test_path(&graph.symbol(*from).path) || is_test_related(&to_symbol(graph, *from))
        })
        .flat_map(|(from, edge)| ref_entries(graph, from, id, edge, from))
        .filter(|entry| {
            let name = entry.symbol.as_ref().map_or("", |s| s.name.as_str());
            seen.insert((entry.file_path.clone(), name.to_string()))
        })
        .collect();
    refs.truncate(TEST_REF_CAP);
    refs
}
