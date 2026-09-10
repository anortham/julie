//! Symbol rows from the snapshot graph, shaped as `julie_core::Symbol` for the
//! filtering, body-extraction, and formatting stages.

use julie_core::Symbol;
use julie_extractors::NormalizedSpan;
use julie_facts::rows::{Span, SymbolRow};
use julie_index::snapshot::Snapshot;

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

fn symbol_from_row(row: &SymbolRow) -> Symbol {
    julie_extractors::Symbol {
        id: row.id.clone(),
        name: row.name.clone(),
        kind: row.kind.clone(),
        language: row.language.clone(),
        file_path: row.path.clone(),
        start_line: row.span.start_line,
        start_column: row.span.start_col,
        end_line: row.span.end_line,
        end_column: row.span.end_col,
        start_byte: row.span.start_byte,
        end_byte: row.span.end_byte,
        body_span: row.body_span.map(normalized),
        body_hash: row.body_hash.clone(),
        signature: row.signature.clone(),
        doc_comment: row.doc_comment.clone(),
        visibility: row.visibility.clone(),
        parent_id: row
            .parent_ordinal
            .map(|ordinal| format!("{}:{ordinal}", row.blob_hash)),
        metadata: row.metadata.clone(),
        annotations: row.annotations.clone(),
        semantic_group: row.semantic_group.clone(),
        confidence: row.confidence,
        content_type: row.content_type.clone(),
    }
    .into()
}

/// Every symbol stored for `path`, in source order.
pub(super) fn symbols_in_path(snapshot: &Snapshot, path: &str) -> Vec<Symbol> {
    let graph = snapshot.graph();
    let mut symbols: Vec<Symbol> = graph
        .symbols_in_path(path)
        .iter()
        .map(|id| symbol_from_row(graph.symbol(*id)))
        .collect();
    symbols.sort_by_key(|symbol| (symbol.start_line, symbol.start_column));
    symbols
}
