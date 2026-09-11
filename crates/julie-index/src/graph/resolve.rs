//! Name resolution: rows in, edges out. Rules kept from the SQL-era tools:
//! `IMPACT_IDENTIFIER_KINDS` (call, type_usage, import), exact match then
//! qualified suffix, definition priority, and ambiguity drops the edge.

use std::collections::HashSet;

use julie_extractors::{IdentifierKind, RelationshipKind, SymbolKind};
use julie_facts::rows::{IdentifierRow, RelationshipRow};

use super::reexports::ReexportIndex;
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
    let reexports = ReexportIndex::build(symbols);
    let mut qualified_sites: HashSet<(SymbolId, u32, &str)> = HashSet::new();
    for row in relationships {
        let Some(file) = symbols.file_index(&row.path) else {
            continue;
        };
        let Some(from) = row.from_ordinal.and_then(|o| symbols.id_at(file, o)) else {
            continue;
        };
        let to = match row.to_ordinal {
            Some(ordinal) => symbols.id_at(file, ordinal),
            None => {
                let (leaf, qualifier) = split_qualified(&row.to_name);
                if !qualifier.is_empty() {
                    qualified_sites.insert((from, row.line_number, leaf));
                }
                resolve_target(symbols, &reexports, &row.to_name, file)
            }
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
        if qualified_sites.contains(&(from, row.span.start_line, row.name.as_str())) {
            continue;
        }
        if let Some(to) = resolve_target(symbols, &reexports, &row.name, file) {
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

pub(super) fn is_definition(kind: &SymbolKind) -> bool {
    !matches!(kind, SymbolKind::Import | SymbolKind::Export)
}

/// `a::b::leaf` or `a.b.leaf` -> `("leaf", ["a", "b"])`. Leading `crate`,
/// `self`, `Self`, and `super` say nothing about the target and are dropped.
pub(super) fn split_qualified(name: &str) -> (&str, Vec<&str>) {
    let mut segments: Vec<&str> = name
        .split("::")
        .flat_map(|s| s.split('.'))
        .filter(|s| !s.is_empty())
        .collect();
    let Some(leaf) = segments.pop() else {
        return (name, Vec::new());
    };
    segments.retain(|s| !matches!(*s, "crate" | "self" | "Self" | "super"));
    (leaf, segments)
}

/// `search::hybrid` matches `src/search/hybrid.rs` and `src/search/hybrid/mod.rs`.
fn path_matches_qualifier(path: &str, qualifier: &[&str]) -> bool {
    let stem = path.rsplit_once('.').map_or(path, |(stem, _)| stem);
    let mut components: Vec<&str> = stem.split('/').filter(|c| !c.is_empty()).collect();
    if matches!(components.last(), Some(&"mod" | &"index" | &"__init__")) {
        components.pop();
    }
    components.ends_with(qualifier)
}

fn qualifier_matches(symbols: &SymbolTable, id: SymbolId, qualifier: &[&str]) -> bool {
    let parent_matches = symbols.parent(id).is_some_and(|parent| {
        Some(symbols.symbol(parent).name.as_str()) == qualifier.last().copied()
    });
    parent_matches || path_matches_qualifier(&symbols.symbol(id).path, qualifier)
}

/// Exact name, else the leaf of a qualified name narrowed to candidates whose
/// parent or file path matches the qualifier (none left drops the edge);
/// same-file definitions win; then the best definition priority. A tie at
/// the best priority is ambiguous: no target.
pub fn resolve_target(
    symbols: &SymbolTable,
    reexports: &ReexportIndex,
    name: &str,
    from_file: u32,
) -> Option<SymbolId> {
    let exact = symbols.find_by_name(name);
    let (leaf, qualifier) = if exact.is_empty() {
        split_qualified(name)
    } else {
        (name, Vec::new())
    };
    let mut candidates: Vec<SymbolId> = symbols
        .find_by_name(leaf)
        .iter()
        .copied()
        .filter(|id| is_definition(&symbols.symbol(*id).kind))
        .collect();
    if !qualifier.is_empty() {
        candidates.retain(|id| qualifier_matches(symbols, *id, &qualifier));
    }
    if candidates
        .iter()
        .any(|id| symbols.file_of(*id) == from_file)
    {
        candidates.retain(|id| symbols.file_of(*id) == from_file);
    }
    let Some(best) = candidates
        .iter()
        .map(|id| definition_priority(&symbols.symbol(*id).kind))
        .min()
    else {
        return super::reexports::resolve_reexport(symbols, reexports, name, from_file);
    };
    let mut top = candidates
        .iter()
        .filter(|id| definition_priority(&symbols.symbol(**id).kind) == best);
    let first = *top.next()?;
    top.next().is_none().then_some(first)
}
