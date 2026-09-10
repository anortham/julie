//! Source sites of a graph edge: the relationship and identifier rows of the
//! referencing symbol that name the target, one per line.

use julie_extractors::{IdentifierKind, RelationshipKind};
use julie_index::graph::{Graph, SymbolId};

use super::resolution::qualified_leaf;

/// One line in the referencing symbol where the target is named.
pub struct Site {
    pub line: u32,
    pub kind: RelationshipKind,
    /// The identifier kind on this line, when an identifier row exists; the
    /// `reference_kind` filter matches against it.
    pub identifier_kind: Option<IdentifierKind>,
    pub confidence: f32,
}

pub fn identifier_relationship_kind(kind: &IdentifierKind) -> RelationshipKind {
    match kind {
        IdentifierKind::Call => RelationshipKind::Calls,
        IdentifierKind::TypeUsage => RelationshipKind::Uses,
        IdentifierKind::MemberAccess | IdentifierKind::VariableRef => RelationshipKind::References,
    }
}

/// The `reference_kind` filter value that names `kind`.
pub fn identifier_kind_name(kind: &IdentifierKind) -> &'static str {
    match kind {
        IdentifierKind::Call => "call",
        IdentifierKind::VariableRef => "variable_ref",
        IdentifierKind::TypeUsage => "type_usage",
        IdentifierKind::MemberAccess => "member_access",
    }
}

/// Lines in `from` that name `to`. A relationship row and an identifier row
/// on the same line merge into one site that keeps the relationship's kind
/// and confidence and the identifier's kind for filtering.
pub fn reference_sites(graph: &Graph, from: SymbolId, to: SymbolId) -> Vec<Site> {
    let from_row = graph.symbol(from);
    let to_row = graph.symbol(to);
    let Some(rows) = graph.file_rows(&from_row.path) else {
        return Vec::new();
    };
    let same_file = from_row.path == to_row.path;

    let mut sites: Vec<Site> = rows
        .relationships
        .iter()
        .filter(|row| row.from_ordinal == Some(from_row.ordinal))
        .filter(|row| match row.to_ordinal {
            Some(ordinal) => same_file && ordinal == to_row.ordinal,
            None => qualified_leaf(&row.to_name) == to_row.name,
        })
        .map(|row| Site {
            line: row.line_number,
            kind: row.kind.clone(),
            identifier_kind: None,
            confidence: row.confidence,
        })
        .collect();

    let identifiers = rows.identifiers.iter().filter(|row| {
        row.containing_ordinal == Some(from_row.ordinal) && qualified_leaf(&row.name) == to_row.name
    });
    for identifier in identifiers {
        match sites
            .iter_mut()
            .find(|site| site.line == identifier.span.start_line)
        {
            Some(site) => {
                site.identifier_kind
                    .get_or_insert_with(|| identifier.kind.clone());
            }
            None => sites.push(Site {
                line: identifier.span.start_line,
                kind: identifier_relationship_kind(&identifier.kind),
                identifier_kind: Some(identifier.kind.clone()),
                confidence: identifier.confidence,
            }),
        }
    }

    sites.sort_by_key(|site| site.line);
    sites.dedup_by_key(|site| site.line);
    sites
}
