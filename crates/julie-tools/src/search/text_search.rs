//! Text-based search using Tantivy with code-aware tokenization, read from
//! one immutable snapshot.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use julie_context::ToolContext;
use julie_core::Symbol;
use julie_extractors::SymbolKind;
use julie_facts::rows::SymbolRow;
use julie_index::search::SearchFilter;
use julie_index::search::index::{SearchView, UnifiedHit};
use julie_index::snapshot::Snapshot;

use crate::navigation::resolution::WorkspaceTarget;

/// Search-pipeline fixture for unit tests: `search_symbols` plus the NL path
/// prior, over a hand-built `SearchIndex`.
///
/// Returns `(symbols, relaxed, pre_truncation_total)`.
#[cfg(any(test, feature = "test-support"))]
pub fn definition_search_with_index_for_test(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    index: &julie_index::search::index::SearchIndex,
) -> Result<(Vec<Symbol>, bool, usize)> {
    let tantivy_limit = limit.saturating_mul(20).max(500);
    let mut symbol_results = index.search_symbols(query, filter, tantivy_limit)?;
    julie_index::search::scoring::apply_nl_path_prior(&mut symbol_results.results, query);
    symbol_results.results.truncate(limit);
    let symbols: Vec<Symbol> = symbol_results
        .results
        .into_iter()
        .map(|h| {
            symbol_from_parts(
                h.id,
                h.name,
                &h.kind,
                h.language,
                h.file_path,
                h.start_line,
                h.signature,
                h.doc_comment,
                h.score,
            )
        })
        .collect();
    let total = symbols.len();
    Ok((symbols, symbol_results.relaxed, total))
}

#[allow(clippy::too_many_arguments)]
fn symbol_from_parts(
    id: String,
    name: String,
    kind: &str,
    language: String,
    file_path: String,
    start_line: u32,
    signature: String,
    doc_comment: String,
    score: f32,
) -> Symbol {
    Symbol {
        extracted: julie_extractors::Symbol {
            id,
            name,
            kind: SymbolKind::try_from_string(kind).unwrap_or(SymbolKind::Variable),
            language,
            file_path,
            start_line,
            signature: (!signature.is_empty()).then_some(signature),
            doc_comment: (!doc_comment.is_empty()).then_some(doc_comment),
            start_column: 0,
            end_line: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: Some(score),
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        code_context: None,
    }
}

fn unified_hit_to_symbol(hit: UnifiedHit) -> Symbol {
    symbol_from_parts(
        hit.id,
        hit.name,
        &hit.kind,
        hit.language,
        hit.file_path,
        hit.start_line,
        hit.signature,
        hit.doc_comment,
        hit.tantivy_score,
    )
}

fn normalized_span(span: julie_facts::rows::Span) -> julie_extractors::NormalizedSpan {
    julie_extractors::NormalizedSpan {
        start_line: span.start_line,
        start_column: span.start_col,
        end_line: span.end_line,
        end_column: span.end_col,
        start_byte: span.start_byte,
        end_byte: span.end_byte,
    }
}

/// A `julie_core::Symbol` for one facts row. `code_context` is the row's span
/// sliced from `text` when the checkout still matches the facts.
pub(crate) fn symbol_from_row(row: &SymbolRow, text: Option<&str>) -> Symbol {
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
            body_span: row.body_span.clone().map(normalized_span),
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

/// Fill `code_context`, `visibility`, `metadata`, `body_span`, and `body_hash`
/// from the graph row behind each search hit. Tantivy only stores a truncated
/// body, so the full symbol text comes from the checkout file.
pub(crate) fn hydrate_symbols(snapshot: &Snapshot, symbols: &mut [Symbol]) {
    let graph = snapshot.graph();
    let mut texts: HashMap<String, Option<String>> = HashMap::new();
    for symbol in symbols {
        let Some(row) = graph
            .symbols_in_path(&symbol.file_path)
            .iter()
            .map(|id| graph.symbol(*id))
            .find(|row| row.id == symbol.id)
        else {
            continue;
        };
        let text = texts
            .entry(row.path.clone())
            .or_insert_with(|| snapshot.file_text(&row.path).ok().flatten());
        let full = symbol_from_row(row, text.as_deref());
        symbol.code_context = full.code_context;
        symbol.visibility = full.extracted.visibility;
        symbol.metadata = full.extracted.metadata;
        symbol.body_span = full.extracted.body_span;
        symbol.body_hash = full.extracted.body_hash;
    }
}

/// Kind of result row to retain after the unified search.  None = keep all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedKindFilter {
    /// Drop file rows (`kind == "file"`); keep symbol rows.
    SymbolsOnly,
    /// Drop symbol rows; keep only file rows.
    FilesOnly,
}

/// Unified BM25 search returning hits as symbols.
/// Returns `(hits_as_symbols, relaxed, total_count)` where `relaxed` is true
/// when the AND query fell back to OR mode.
pub async fn unified_search_impl(
    query: &str,
    filter: &SearchFilter,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    handler: &dyn ToolContext,
) -> Result<(Vec<Symbol>, bool, usize)> {
    unified_search_impl_with_kind_filter(query, filter, limit, workspace_ids, None, handler).await
}

/// Like [`unified_search_impl`] with an optional [`UnifiedKindFilter`] applied
/// before reranking.
pub async fn unified_search_impl_with_kind_filter(
    query: &str,
    filter: &SearchFilter,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    kind_filter: Option<UnifiedKindFilter>,
    handler: &dyn ToolContext,
) -> Result<(Vec<Symbol>, bool, usize)> {
    super::nl_embeddings::maybe_initialize_embeddings_for_nl_definitions(query, handler).await;

    let current_primary_id = handler.current_workspace_id();
    let target = match workspace_ids.and_then(|ids| ids.into_iter().next()) {
        Some(id) if current_primary_id.as_deref() != Some(id.as_str()) => {
            WorkspaceTarget::Target(id)
        }
        _ => WorkspaceTarget::Primary,
    };
    let snapshot = handler.snapshot(&target).await?;

    let query = query.to_string();
    let filter = filter.clone();
    let limit = limit as usize;
    let files_only = kind_filter.map(|kf| matches!(kf, UnifiedKindFilter::FilesOnly));
    tokio::task::spawn_blocking(move || -> Result<(Vec<Symbol>, bool, usize)> {
        let view = SearchView::new(snapshot.searcher(), snapshot.fields());
        let (hits, relaxed) = match files_only {
            Some(flag) => view.search_unified_kind_filtered(&query, &filter, limit, flag)?,
            None => view.search_unified_with_meta(&query, &filter, limit)?,
        };
        let count = hits.len();
        let mut symbols: Vec<Symbol> = hits.into_iter().map(unified_hit_to_symbol).collect();
        hydrate_symbols(&snapshot, &mut symbols);
        Ok((symbols, relaxed, count))
    })
    .await?
}

/// Raw [`UnifiedHit`]s from the snapshot's searcher, so the `"file"` kind
/// survives all the way to [`crate::search::SearchHit`].
pub async fn unified_search_hits(
    query: &str,
    filter: &SearchFilter,
    limit: u32,
    snapshot: &Arc<Snapshot>,
) -> Result<(Vec<UnifiedHit>, bool, usize)> {
    let snapshot = Arc::clone(snapshot);
    let query = query.to_string();
    let filter = filter.clone();
    let limit = limit as usize;
    tokio::task::spawn_blocking(move || -> Result<(Vec<UnifiedHit>, bool, usize)> {
        let (hits, relaxed) = SearchView::new(snapshot.searcher(), snapshot.fields())
            .search_unified_with_meta(&query, &filter, limit)?;
        let count = hits.len();
        Ok((hits, relaxed, count))
    })
    .await?
}

/// Test-only entry point kept for the dogfood suite and the top-crate tests.
/// `search_target` selects the kind filter; everything routes through the
/// unified path.
#[cfg(any(test, feature = "test-support"))]
#[allow(clippy::too_many_arguments)]
pub async fn text_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    search_target: &str,
    _context_lines: Option<u32>,
    exclude_tests: Option<bool>,
    handler: &dyn ToolContext,
) -> Result<(Vec<Symbol>, bool, usize)> {
    let mut filter = SearchFilter::default();
    if let Some(lang) = language {
        filter.language = Some(lang.clone());
    }
    if let Some(pat) = file_pattern {
        filter.file_pattern = Some(pat.clone());
    }
    if exclude_tests == Some(true) {
        filter.exclude_tests = true;
    }

    let kind_filter = match search_target {
        "definitions" => Some(UnifiedKindFilter::SymbolsOnly),
        "content" | "files" | "paths" => Some(UnifiedKindFilter::FilesOnly),
        _ => None,
    };

    unified_search_impl_with_kind_filter(query, &filter, limit, workspace_ids, kind_filter, handler)
        .await
}
