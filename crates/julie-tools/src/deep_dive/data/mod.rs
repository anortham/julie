//! Data gathering for deep_dive tool
//!
//! Collects symbol context from the snapshot: graph neighbors, children,
//! implementations, the checkout text for bodies, and one facts query for
//! complexity.

pub mod graph;
pub mod similarity;
pub mod types;

use std::collections::HashMap;

use anyhow::Result;
use tracing::debug;

use julie_core::Symbol;
use julie_extractors::{RelationshipKind, SymbolKind};
use julie_index::graph::SymbolId;
use julie_index::snapshot::Snapshot;

use self::similarity::build_similar;
use crate::snapshot_rows::{symbol_from_row, to_symbol};

pub use self::types::{RefEntry, SimilarEntry, SymbolContext};
pub use crate::navigation::resolution::find_symbols as find_symbol;

type FileTexts = HashMap<String, Option<String>>;

fn file_text<'a>(
    snapshot: &Snapshot,
    texts: &'a mut FileTexts,
    path: &str,
) -> Result<Option<&'a str>> {
    if !texts.contains_key(path) {
        texts.insert(path.to_string(), snapshot.file_text(path)?);
    }
    Ok(texts[path].as_deref())
}

fn attach_bodies(snapshot: &Snapshot, texts: &mut FileTexts, refs: &mut [RefEntry]) -> Result<()> {
    for symbol in refs.iter_mut().filter_map(|entry| entry.symbol.as_mut()) {
        let body = file_text(snapshot, texts, &symbol.file_path)?.and_then(|text| {
            text.get(symbol.start_byte as usize..symbol.end_byte as usize)
                .map(str::to_string)
        });
        symbol.code_context = body;
    }
    Ok(())
}

fn calls(refs: &[RefEntry]) -> usize {
    refs.iter()
        .filter(|entry| matches!(entry.kind, RelationshipKind::Calls))
        .count()
}

fn sorted_symbols(graph: &julie_index::graph::Graph, ids: Vec<SymbolId>) -> Vec<Symbol> {
    let mut ids = ids;
    ids.sort_by(|a, b| {
        let (a, b) = (graph.symbol(*a), graph.symbol(*b));
        a.path
            .cmp(&b.path)
            .then(a.span.start_line.cmp(&b.span.start_line))
    });
    ids.into_iter().map(|id| to_symbol(graph, id)).collect()
}

/// Build full context for a symbol.
///
/// Refs are always enriched with symbol names. `depth` controls body display:
/// - "overview": no code bodies
/// - "context": primary symbol body (30 lines)
/// - "full": primary symbol body (100 lines) + ref bodies
pub fn build_symbol_context(
    snapshot: &Snapshot,
    id: SymbolId,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
) -> Result<SymbolContext> {
    build_symbol_context_with_semantics(snapshot, id, depth, incoming_cap, outgoing_cap, true)
}

pub fn build_symbol_context_with_semantics(
    snapshot: &Snapshot,
    id: SymbolId,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
    allow_similarity: bool,
) -> Result<SymbolContext> {
    let graph = snapshot.graph();
    let row = graph.symbol(id);
    let mut texts = FileTexts::new();

    let mut incoming = graph::incoming_refs(graph, id);
    let incoming_total = incoming.len();
    let incoming_calls_total = calls(&incoming);
    incoming.truncate(incoming_cap);

    let mut outgoing = graph::outgoing_refs(graph, id);
    let outgoing_total = outgoing.len();
    let outgoing_calls_total = calls(&outgoing);
    outgoing.truncate(outgoing_cap);

    if depth == "full" {
        attach_bodies(snapshot, &mut texts, &mut incoming)?;
        attach_bodies(snapshot, &mut texts, &mut outgoing)?;
    }

    debug!(
        "deep_dive: {} incoming (of {}), {} outgoing (of {})",
        incoming.len(),
        incoming_total,
        outgoing.len(),
        outgoing_total
    );

    let children = if is_container_kind(&row.kind) {
        sorted_symbols(graph, graph.children(id).collect())
    } else {
        vec![]
    };

    let implementations = if matches!(row.kind, SymbolKind::Interface | SymbolKind::Trait) {
        sorted_symbols(graph, graph.implementations(id).collect())
    } else {
        vec![]
    };

    let symbol = if depth == "overview" {
        to_symbol(graph, id)
    } else {
        symbol_from_row(row, file_text(snapshot, &mut texts, &row.path)?)
    };
    let complexity = snapshot
        .facts()?
        .reader()
        .complexity_for_symbol(&row.id)?
        .into_iter()
        .next();

    let test_refs = if depth == "full" || depth == "context" {
        graph::test_refs(graph, id)
    } else {
        vec![]
    };

    let similar = if allow_similarity && (depth == "full" || depth == "context") {
        build_similar(snapshot, id)
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
