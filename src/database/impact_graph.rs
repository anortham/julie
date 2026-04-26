//! Shared identifier-expansion helper for impact/context graph walks.
//!
//! The `relationships` table misses real references in this codebase — for
//! TypeScript (and similar languages) type usages, calls, and imports live
//! only in the `identifiers` table. Both `walk_impacts` (blast_radius) and
//! `expand_graph_from_ids` (get_context) need to fold identifier-based edges
//! into their frontier; keeping that logic in one place ensures they stay in
//! sync.

use std::collections::HashSet;

use anyhow::Result;

use super::SymbolDatabase;
use crate::extractors::{RelationshipKind, Symbol};

/// Kinds of identifier references that surface incoming edges for graph walks.
/// Must match the filter used by callers (previously inlined at
/// `src/tools/get_context/pipeline.rs:135-158`).
pub const IMPACT_IDENTIFIER_KINDS: &[&str] = &["type_usage", "import", "call"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierIncomingEdge {
    pub container_id: String,
    pub relationship_kind: RelationshipKind,
    pub target_symbol_id: Option<String>,
}

/// Expand a set of seed symbols into identifier-based incoming edges.
///
/// Returns incoming edges with the originating symbol, normalized relationship
/// kind, and the resolved frontier target when it can be determined.
///
/// For each identifier whose `name` matches one of the frontier symbol names
/// and whose `kind` is in `IMPACT_IDENTIFIER_KINDS`, emit an edge pointing
/// from the containing symbol to the frontier. Identifiers whose containing
/// symbol is already excluded are skipped so we don't synthesize self-edges or
/// revisit known nodes.
///
/// Duplicate `containing_symbol_id`s are preserved in the output; callers
/// deduplicate using their own "first-seen wins" policy (see
/// `HashMap::or_insert_with`). Relationships from the `relationships` table
/// should be merged first so they take priority over identifier-derived ones.
pub fn identifier_incoming_edges(
    db: &SymbolDatabase,
    frontier_symbols: &[Symbol],
    excluded_container_ids: &HashSet<String>,
) -> Result<Vec<IdentifierIncomingEdge>> {
    if frontier_symbols.is_empty() {
        return Ok(Vec::new());
    }

    let frontier_names: Vec<String> = frontier_symbols
        .iter()
        .map(|symbol| symbol.name.clone())
        .collect();
    let identifier_refs = db.get_identifiers_by_names_kinds_excluding_containers(
        &frontier_names,
        IMPACT_IDENTIFIER_KINDS,
        excluded_container_ids,
    )?;
    let mut edges = Vec::with_capacity(identifier_refs.len());

    for iref in identifier_refs {
        let Some(relationship_kind) = identifier_relationship_kind(&iref.kind) else {
            continue;
        };
        let Some(container_id) = iref.containing_symbol_id else {
            continue;
        };
        if excluded_container_ids.contains(&container_id) {
            continue;
        }

        let target_symbol_id =
            resolve_frontier_target(&iref.name, iref.target_symbol_id.as_ref(), frontier_symbols);
        if iref.target_symbol_id.is_some() && target_symbol_id.is_none() {
            continue;
        }

        edges.push(IdentifierIncomingEdge {
            container_id,
            relationship_kind,
            target_symbol_id,
        });
    }

    Ok(edges)
}

fn identifier_relationship_kind(kind: &str) -> Option<RelationshipKind> {
    if !IMPACT_IDENTIFIER_KINDS.contains(&kind) {
        return None;
    }

    match kind {
        "call" => Some(RelationshipKind::Calls),
        "import" => Some(RelationshipKind::Imports),
        "type_usage" => Some(RelationshipKind::References),
        _ => None,
    }
}

fn resolve_frontier_target(
    identifier_name: &str,
    target_symbol_id: Option<&String>,
    frontier_symbols: &[Symbol],
) -> Option<String> {
    if let Some(target_symbol_id) = target_symbol_id {
        return frontier_symbols
            .iter()
            .find(|symbol| symbol.id == *target_symbol_id)
            .map(|symbol| symbol.id.clone());
    }

    let mut matches = frontier_symbols
        .iter()
        .filter(|symbol| identifier_matches_symbol(identifier_name, &symbol.name));
    let first_match = matches.next()?;
    if matches.next().is_some() {
        return None;
    }

    Some(first_match.id.clone())
}

fn identifier_matches_symbol(identifier_name: &str, symbol_name: &str) -> bool {
    if identifier_name == symbol_name {
        return true;
    }

    identifier_name
        .strip_prefix(symbol_name)
        .is_some_and(|suffix| suffix.starts_with("::") || suffix.starts_with('.'))
}
