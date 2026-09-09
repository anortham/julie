//! Data gathering for deep_dive tool
//!
//! Collects symbol context from SQLite: relationships, children, types.
//! All queries use existing indexed data — no new indexing required.

pub mod graph;
pub mod lookup;
pub mod similarity;
pub mod types;

use anyhow::Result;
use tracing::debug;

use julie_core::Symbol;
use julie_core::database::SymbolDatabase;
use julie_core::shared::NOISE_CALLEE_NAMES;
use julie_extractors::{Relationship, RelationshipKind, SymbolKind};

use self::graph::{build_test_refs, enrich_refs, merge_identifier_refs};
use self::similarity::build_similar;

// Public re-exports
pub use self::lookup::find_symbol;
pub use self::types::{RefEntry, SimilarEntry, SymbolContext};

/// Build full context for a symbol.
///
/// Refs are always enriched with symbol names. `depth` controls body display:
/// - "overview": no code bodies
/// - "context": primary symbol body (30 lines)
/// - "full": primary symbol body (100 lines) + ref bodies
pub fn build_symbol_context(
    db: &SymbolDatabase,
    symbol: &Symbol,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
) -> Result<SymbolContext> {
    build_symbol_context_with_semantics(db, symbol, depth, incoming_cap, outgoing_cap, true)
}

pub fn build_symbol_context_with_semantics(
    db: &SymbolDatabase,
    symbol: &Symbol,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
    allow_similarity: bool,
) -> Result<SymbolContext> {
    let symbol_ids = vec![symbol.id.clone()];
    let needs_body_enrichment = depth != "overview";

    // === Incoming references (who references this symbol) ===
    let raw_incoming = db.get_relationships_to_symbols(&symbol_ids)?;
    let incoming_total = raw_incoming.len();
    let incoming_calls_total = raw_incoming
        .iter()
        .filter(|rel| matches!(rel.kind, RelationshipKind::Calls))
        .count();

    let incoming_rels: Vec<&Relationship> = raw_incoming.iter().take(incoming_cap).collect();
    let mut incoming: Vec<RefEntry> = incoming_rels
        .iter()
        .map(|rel| RefEntry {
            kind: rel.kind.clone(),
            file_path: rel.file_path.clone(),
            line_number: rel.line_number,
            symbol: None,
        })
        .collect();

    // Always enrich refs — symbol names are useful at every depth level
    {
        let symbol_ids: Vec<String> = incoming_rels
            .iter()
            .map(|rel| rel.from_symbol_id.clone())
            .collect();
        enrich_refs(db, &mut incoming, &symbol_ids)?;
    }

    // === Outgoing references (what this symbol calls/uses) ===
    let raw_outgoing = db.get_outgoing_relationships(&symbol.id)?;
    let outgoing_total = raw_outgoing.len();
    let outgoing_calls_total = raw_outgoing
        .iter()
        .filter(|rel| matches!(rel.kind, RelationshipKind::Calls))
        .count();

    let outgoing_rels: Vec<&Relationship> = raw_outgoing.iter().take(outgoing_cap).collect();
    let mut outgoing: Vec<RefEntry> = outgoing_rels
        .iter()
        .map(|rel| RefEntry {
            kind: rel.kind.clone(),
            file_path: rel.file_path.clone(),
            line_number: rel.line_number,
            symbol: None,
        })
        .collect();

    {
        let symbol_ids: Vec<String> = outgoing_rels
            .iter()
            .map(|rel| rel.to_symbol_id.clone())
            .collect();
        enrich_refs(db, &mut outgoing, &symbol_ids)?;
    }

    // Filter noise callees — common names like `new`, `len`, `from` that
    // resolve to wrong symbols because they're too ambiguous
    let pre_filter_len = outgoing.len();
    outgoing.retain(|r| {
        let name = r.symbol.as_ref().map(|s| s.name.as_str()).unwrap_or("");
        !NOISE_CALLEE_NAMES.contains(&name)
    });
    let outgoing_total = outgoing_total.saturating_sub(pre_filter_len - outgoing.len());

    // === Identifier fallback: catch refs that relationships miss ===
    let (incoming, incoming_total, incoming_calls_total) = merge_identifier_refs(
        db,
        symbol,
        incoming,
        incoming_total,
        incoming_calls_total,
        incoming_cap,
    )?;

    debug!(
        "deep_dive: {} incoming (of {}), {} outgoing (of {})",
        incoming.len(),
        incoming_total,
        outgoing.len(),
        outgoing_total
    );

    // === Children (methods, fields for struct/class/trait/enum/module) ===
    let children = if is_container_kind(&symbol.kind) {
        db.get_children_by_parent_id(&symbol.id)?
    } else {
        vec![]
    };

    // === Implementations (for trait/interface) ===
    let implementations = if matches!(symbol.kind, SymbolKind::Interface | SymbolKind::Trait) {
        db.find_type_implementations(&symbol.name, Some(&symbol.language))
            .unwrap_or_default()
    } else {
        vec![]
    };

    // === Primary symbol enrichment (code_context at context/full) ===
    let symbol = if needs_body_enrichment && symbol.code_context.is_none() {
        db.get_symbol_by_id(&symbol.id)?
            .unwrap_or_else(|| symbol.clone())
    } else {
        symbol.clone()
    };
    let complexity = db.get_complexity_metric_for_symbol(&symbol.id)?;

    // === Test locations (context and full depth) ===
    let test_refs = if depth == "full" || depth == "context" {
        build_test_refs(db, &symbol)?
    } else {
        vec![]
    };

    // === Semantically similar symbols (context and full depth) ===
    let similar = if allow_similarity && (depth == "full" || depth == "context") {
        build_similar(db, &symbol)?
    } else {
        vec![]
    };

    Ok(SymbolContext {
        symbol,
        complexity,
        incoming,
        incoming_total,
        incoming_calls_total,
        outgoing,
        outgoing_total,
        outgoing_calls_total,
        children,
        implementations,
        test_refs,
        similar,
    })
}

fn is_container_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class
            | SymbolKind::Interface
            | SymbolKind::Trait
            | SymbolKind::Enum
            | SymbolKind::Module
            | SymbolKind::Namespace
    )
}
