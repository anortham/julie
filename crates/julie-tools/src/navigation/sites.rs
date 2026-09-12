//! Source sites of a graph edge: the relationship and identifier rows of the
//! referencing symbol that name the target.

use std::collections::HashMap;

use julie_extractors::{IdentifierKind, NormalizedSpan, RelationshipKind};
use julie_facts::rows::Span;
use julie_index::graph::{Graph, SymbolId};
use serde_json::{Value, json};

use super::resolution::qualified_leaf;

/// One canonical source occurrence where the target is named.
pub struct Site {
    pub id: String,
    pub line: u32,
    pub span: Option<NormalizedSpan>,
    pub exact: bool,
    pub kind: RelationshipKind,
    /// The identifier kind at this site, when an identifier row exists; the
    /// `reference_kind` filter matches against it.
    pub identifier_kind: Option<IdentifierKind>,
    pub confidence: f32,
    pub metadata: Option<HashMap<String, Value>>,
}

fn normalized(span: Span) -> NormalizedSpan {
    NormalizedSpan {
        start_line: span.start_line,
        start_column: span.start_col,
        end_line: span.end_line,
        end_column: span.end_col,
        start_byte: span.start_byte,
        end_byte: span.end_byte,
    }
}

fn with_provenance(
    metadata: Option<HashMap<String, Value>>,
    provenance: &'static str,
) -> Option<HashMap<String, Value>> {
    let mut metadata = metadata.unwrap_or_default();
    metadata.insert("reference_site_provenance".into(), json!(provenance));
    Some(metadata)
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
        .identifiers
        .iter()
        .filter(|row| {
            row.containing_ordinal == Some(from_row.ordinal)
                && qualified_leaf(&row.name) == to_row.name
        })
        .map(|row| Site {
            id: row.id.clone(),
            line: row.span.start_line,
            span: Some(normalized(row.span)),
            exact: true,
            kind: identifier_relationship_kind(&row.kind),
            identifier_kind: Some(row.kind.clone()),
            confidence: row.confidence,
            metadata: with_provenance(None, "identifier"),
        })
        .collect();

    for relationship in rows
        .relationships
        .iter()
        .filter(|row| row.from_ordinal == Some(from_row.ordinal))
        .filter(|row| match row.to_ordinal {
            Some(ordinal) => same_file && ordinal == to_row.ordinal,
            None => qualified_leaf(&row.to_name) == to_row.name,
        })
    {
        let span = relationship.span.map(normalized);
        if let Some(site) = sites.iter_mut().find(|site| {
            span.as_ref().is_some_and(|relationship_span| {
                site.span.as_ref() == Some(relationship_span)
                    || (!relationship.reference_site_is_exact
                        && site.kind == relationship.kind
                        && site.span.as_ref().is_some_and(|identifier_span| {
                            relationship_span.start_byte <= identifier_span.start_byte
                                && relationship_span.end_byte >= identifier_span.end_byte
                        }))
            })
                || (span.is_none()
                    && site.line == relationship.line_number
                    && site.kind == relationship.kind)
        }) {
            site.kind = relationship.kind.clone();
            site.confidence = site.confidence.max(relationship.confidence);
            site.metadata =
                with_provenance(relationship.metadata.clone(), "identifier+relationship");
            continue;
        }
        sites.push(Site {
            id: format!(
                "relationship_{}_{}_{}",
                relationship.path, relationship.blob_hash, relationship.ordinal
            ),
            line: relationship.line_number,
            span,
            exact: relationship.reference_site_is_exact && relationship.span.is_some(),
            kind: relationship.kind.clone(),
            identifier_kind: None,
            confidence: relationship.confidence,
            metadata: with_provenance(relationship.metadata.clone(), "relationship"),
        });
    }

    sites.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then_with(|| {
                a.span
                    .as_ref()
                    .map(|span| (span.start_byte, span.end_byte))
                    .cmp(
                        &b.span
                            .as_ref()
                            .map(|span| (span.start_byte, span.end_byte)),
                    )
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    sites.dedup_by(|a, b| a.id == b.id);
    sites
}
