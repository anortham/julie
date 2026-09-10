use julie_core::Symbol;
use julie_index::graph::{Graph, SymbolId};

/// Every symbol of the current graph as the `Symbol` the metadata formatter
/// reads, without code context.
pub(crate) fn graph_symbols(graph: &Graph) -> Vec<(SymbolId, Symbol)> {
    (0..graph.len() as u32)
        .map(SymbolId)
        .map(|id| {
            let row = graph.symbol(id);
            let symbol = Symbol {
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
                    body_span: None,
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
                code_context: None,
            };
            (id, symbol)
        })
        .collect()
}
