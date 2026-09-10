use std::collections::HashMap;

use julie_core::Symbol;
use julie_extractors::{IdentifierKind, SymbolKind};
use julie_index::graph::{Graph, SymbolId};

fn sorted_names(graph: &Graph, ids: impl Iterator<Item = SymbolId>) -> Vec<String> {
    let mut names: Vec<String> = ids.map(|id| graph.symbol(id).name.clone()).collect();
    names.sort();
    names.dedup();
    names
}

/// symbol id -> names of the functions it calls, from `Calls` edges only.
pub(crate) fn build_callee_map(
    graph: &Graph,
    symbols: &[(SymbolId, Symbol)],
) -> HashMap<String, Vec<String>> {
    symbols
        .iter()
        .filter(|(_, s)| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .filter_map(|(id, s)| {
            let names = sorted_names(graph, graph.callees(*id));
            (!names.is_empty()).then(|| (s.id.clone(), names))
        })
        .collect()
}

/// symbol id -> field names it accesses (`self.session_metrics`, `this.db`),
/// from member-access identifiers of every current file.
pub(crate) fn build_field_access_map(graph: &Graph) -> HashMap<String, Vec<String>> {
    let mut fields: HashMap<String, Vec<String>> = HashMap::new();
    for path in graph.paths() {
        let Some(rows) = graph.file_rows(path) else {
            continue;
        };
        for identifier in rows.identifiers.iter() {
            if identifier.kind != IdentifierKind::MemberAccess {
                continue;
            }
            if let Some(ordinal) = identifier.containing_ordinal {
                fields
                    .entry(format!("{}:{ordinal}", identifier.blob_hash))
                    .or_default()
                    .push(identifier.name.clone());
            }
        }
    }
    for names in fields.values_mut() {
        names.sort();
        names.dedup();
    }
    fields
}

/// trait or interface id -> up to eight implementor names.
pub(crate) fn build_implementor_map(
    graph: &Graph,
    symbols: &[(SymbolId, Symbol)],
) -> HashMap<String, Vec<String>> {
    symbols
        .iter()
        .filter(|(_, s)| matches!(s.kind, SymbolKind::Trait | SymbolKind::Interface))
        .filter_map(|(id, s)| {
            let mut names = sorted_names(graph, graph.implementations(*id));
            names.truncate(8);
            (!names.is_empty()).then(|| (s.id.clone(), names))
        })
        .collect()
}
