//! Graph and facts symbol rows shaped as the `julie_core::Symbol` the tool
//! formatters take.

use julie_core::Symbol;
use julie_extractors::NormalizedSpan;
use julie_facts::rows::{Span, SymbolRow};
use julie_index::graph::{Graph, SymbolId};

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

/// A `Symbol` for one facts row. `code_context` is the row's span sliced from
/// `text` when the checkout still matches the facts.
pub fn symbol_from_row(row: &SymbolRow, text: Option<&str>) -> Symbol {
    let code_context = text.and_then(|text| {
        text.get(row.span.start_byte as usize..row.span.end_byte as usize)
            .map(str::to_string)
    });
    Symbol {
        extracted: julie_extractors::Symbol {
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
        },
        code_context,
    }
}

/// The graph row of `id` as a `Symbol` without code context.
pub fn to_symbol(graph: &Graph, id: SymbolId) -> Symbol {
    symbol_from_row(graph.symbol(id), None)
}
