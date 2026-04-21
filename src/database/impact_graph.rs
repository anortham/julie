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
use crate::extractors::RelationshipKind;

/// Kinds of identifier references that surface incoming edges for graph walks.
/// Must match the filter used by callers (previously inlined at
/// `src/tools/get_context/pipeline.rs:135-158`).
pub const IMPACT_IDENTIFIER_KINDS: &[&str] = &["type_usage", "import", "call"];

/// Expand a set of seed symbols into identifier-based incoming edges.
///
/// Returns `(containing_symbol_id, relationship_kind)` pairs. The direction is
/// always incoming (the containing symbol references the seed); callers tag
/// direction themselves if they need it.
///
/// For each identifier whose `name` matches one of `seed_symbol_names` and
/// whose `kind` is in `IMPACT_IDENTIFIER_KINDS`, emit an edge pointing from
/// the containing symbol to the seed. Identifiers whose containing symbol is
/// already in `seed_symbol_ids` are excluded so we don't synthesize self-edges.
///
/// Duplicate `containing_symbol_id`s are preserved in the output; callers
/// deduplicate using their own "first-seen wins" policy (see
/// `HashMap::or_insert_with`). Relationships from the `relationships` table
/// should be merged first so they take priority over identifier-derived ones.
pub fn identifier_incoming_edges(
    db: &SymbolDatabase,
    seed_symbol_names: &[String],
    seed_symbol_ids: &HashSet<String>,
) -> Result<Vec<(String, RelationshipKind)>> {
    if seed_symbol_names.is_empty() {
        return Ok(Vec::new());
    }

    let identifier_refs = db.get_identifiers_by_names(seed_symbol_names)?;
    let mut edges = Vec::with_capacity(identifier_refs.len());

    for iref in identifier_refs {
        if !IMPACT_IDENTIFIER_KINDS.contains(&iref.kind.as_str()) {
            continue;
        }
        let Some(container_id) = iref.containing_symbol_id else {
            continue;
        };
        if seed_symbol_ids.contains(&container_id) {
            continue;
        }
        edges.push((container_id, RelationshipKind::References));
    }

    Ok(edges)
}
