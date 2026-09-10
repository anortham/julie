//! Name resolution: rows in, edges out. Rules kept from the SQL-era tools:
//! `IMPACT_IDENTIFIER_KINDS` (call, type_usage, import), exact match then
//! qualified suffix, definition priority, and ambiguity drops the edge.

use julie_extractors::{IdentifierKind, RelationshipKind, SymbolKind};
use julie_facts::rows::{IdentifierRow, RelationshipRow};

use super::{Edge, EdgeKind, SymbolId, SymbolTable};

/// Resolve every identifier and relationship row against `symbols`, plus the
/// `Contains` edges implied by `parent_ordinal`. Self edges are dropped.
pub fn resolve<'a>(
    symbols: &SymbolTable,
    identifiers: impl IntoIterator<Item = &'a IdentifierRow>,
    relationships: impl IntoIterator<Item = &'a RelationshipRow>,
) -> Vec<Edge> {
    let mut edges = Vec::new();
    for (file, rows) in symbols.files().iter().enumerate() {
        let base = symbols.symbols_in_file(file as u32).start;
        for (local, row) in rows.symbols.iter().enumerate() {
            let Some(parent) = row
                .parent_ordinal
                .and_then(|p| symbols.id_at(file as u32, p))
            else {
                continue;
            };
            push(
                &mut edges,
                parent,
                SymbolId(base + local as u32),
                EdgeKind::Contains,
            );
        }
    }
    for row in relationships {
        let Some(file) = symbols.file_index(&row.path) else {
            continue;
        };
        let Some(from) = row.from_ordinal.and_then(|o| symbols.id_at(file, o)) else {
            continue;
        };
        let to = match row.to_ordinal {
            Some(ordinal) => symbols.id_at(file, ordinal),
            None => resolve_target(symbols, &row.to_name, file),
        };
        if let Some(to) = to {
            push(&mut edges, from, to, relationship_edge_kind(&row.kind));
        }
    }
    for row in identifiers {
        let Some(kind) = identifier_edge_kind(&row.kind) else {
            continue;
        };
        let Some(file) = symbols.file_index(&row.path) else {
            continue;
        };
        let Some(from) = row.containing_ordinal.and_then(|o| symbols.id_at(file, o)) else {
            continue;
        };
        if let Some(to) = resolve_target(symbols, &row.name, file) {
            push(&mut edges, from, to, kind);
        }
    }
    edges
}

fn push(edges: &mut Vec<Edge>, from: SymbolId, to: SymbolId, kind: EdgeKind) {
    if from != to {
        edges.push(Edge { from, to, kind });
    }
}

/// `IMPACT_IDENTIFIER_KINDS` mapping: call -> Calls, type_usage -> References.
/// The extractor has no `import` identifier kind, so that mapping has no row.
pub fn identifier_edge_kind(kind: &IdentifierKind) -> Option<EdgeKind> {
    match kind {
        IdentifierKind::Call => Some(EdgeKind::Calls),
        IdentifierKind::TypeUsage => Some(EdgeKind::References),
        IdentifierKind::VariableRef | IdentifierKind::MemberAccess => None,
    }
}

pub fn relationship_edge_kind(kind: &RelationshipKind) -> EdgeKind {
    match kind {
        RelationshipKind::Calls | RelationshipKind::Instantiates => EdgeKind::Calls,
        RelationshipKind::Imports => EdgeKind::Imports,
        RelationshipKind::Implements | RelationshipKind::Overrides => EdgeKind::Implements,
        RelationshipKind::Extends => EdgeKind::Extends,
        RelationshipKind::Contains => EdgeKind::Contains,
        RelationshipKind::Uses
        | RelationshipKind::Returns
        | RelationshipKind::Parameter
        | RelationshipKind::References
        | RelationshipKind::Defines
        | RelationshipKind::Joins
        | RelationshipKind::Composition => EdgeKind::References,
    }
}

/// Classes and interfaces first, then functions, methods, types, values.
pub fn definition_priority(kind: &SymbolKind) -> u8 {
    match kind {
        SymbolKind::Class | SymbolKind::Interface => 1,
        SymbolKind::Function => 2,
        SymbolKind::Method | SymbolKind::Constructor => 3,
        SymbolKind::Type | SymbolKind::Enum => 4,
        SymbolKind::Variable | SymbolKind::Constant => 5,
        _ => 10,
    }
}

fn is_definition(kind: &SymbolKind) -> bool {
    !matches!(kind, SymbolKind::Import | SymbolKind::Export)
}

/// Exact name, else qualified suffix; same-file definitions win; then the best
/// definition priority. A tie at the best priority is ambiguous: no target.
pub fn resolve_target(symbols: &SymbolTable, name: &str, from_file: u32) -> Option<SymbolId> {
    let exact = symbols.find_by_name(name);
    let found = if exact.is_empty() {
        symbols.find_by_name_suffix(name)
    } else {
        exact.to_vec()
    };
    let mut candidates: Vec<SymbolId> = found
        .into_iter()
        .filter(|id| is_definition(&symbols.symbol(*id).kind))
        .collect();
    if candidates
        .iter()
        .any(|id| symbols.file_of(*id) == from_file)
    {
        candidates.retain(|id| symbols.file_of(*id) == from_file);
    }
    let best = candidates
        .iter()
        .map(|id| definition_priority(&symbols.symbol(*id).kind))
        .min()?;
    let mut top = candidates
        .iter()
        .filter(|id| definition_priority(&symbols.symbol(**id).kind) == best);
    let first = *top.next()?;
    top.next().is_none().then_some(first)
}
