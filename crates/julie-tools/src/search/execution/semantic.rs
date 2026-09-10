use std::sync::Arc;

use anyhow::Result;

use julie_core::Symbol;
use julie_core::embeddings_contract::{EmbeddingProvider, EmbeddingRequestBudget, SemanticMode};
use julie_extractors::SymbolKind;
use julie_index::search::hybrid::{
    compute_tagged_query_embedding_for_hybrid, hybrid_search_with_tagged_embedding, vector_search,
};
use julie_index::search::index::SearchFilter;
use julie_index::search::{SymbolSearchResult, SymbolSearchResults};
use julie_index::snapshot::Snapshot;

use super::types::sort_hits_by_score_desc;
use crate::search::backend::SearchBackend;
use crate::search::trace::{SearchExecutionKind, SearchExecutionResult, SearchHit};

const SEMANTIC_OVERFETCH_FACTOR: usize = 4;

/// True when the snapshot carries symbol vectors for the cosine scan.
pub(crate) fn snapshot_has_embeddings(snapshot: &Snapshot) -> bool {
    !snapshot.vectors().is_empty()
}

pub(crate) struct SymbolPassRequest<'a> {
    pub backend: SearchBackend,
    pub query: &'a str,
    pub language: &'a Option<String>,
    pub file_pattern: Option<&'a str>,
    pub limit: u32,
    pub exclude_tests: bool,
    pub workspace_id: &'a str,
    pub provider: Arc<dyn EmbeddingProvider>,
    pub semantic_mode: SemanticMode,
    pub budget: EmbeddingRequestBudget,
}

/// Symbol search over the snapshot's vector set: a pure cosine scan for the
/// semantic backend, RRF-merged with Tantivy for hybrid. The query embedding
/// is a sidecar round trip, so the whole pass runs on `spawn_blocking`.
pub(crate) async fn run_symbol_backend_pass(
    request: SymbolPassRequest<'_>,
    snapshot: &Arc<Snapshot>,
) -> Result<SearchExecutionResult> {
    let backend = request.backend;
    let limit = request.limit.max(1) as usize;
    let filter = SearchFilter {
        language: request.language.clone(),
        kind: None,
        file_pattern: request.file_pattern.map(str::to_string),
        exclude_tests: request.exclude_tests,
    };
    let workspace_id = request.workspace_id.to_string();
    let query = request.query.to_string();
    let provider = request.provider;
    let semantic_mode = request.semantic_mode;
    let budget = request.budget;
    let snapshot = Arc::clone(snapshot);

    let (mut hits, relaxed, total_results) =
        tokio::task::spawn_blocking(move || -> Result<(Vec<SearchHit>, bool, usize)> {
            let tagged = compute_tagged_query_embedding_for_hybrid(
                &query,
                Some(provider.as_ref()),
                &budget,
                semantic_mode,
            )?;
            let results = match backend {
                SearchBackend::Semantic => {
                    let results = match tagged {
                        Some(tagged) => vector_search(
                            &snapshot,
                            &tagged,
                            limit.saturating_mul(SEMANTIC_OVERFETCH_FACTOR),
                            semantic_mode,
                        )?
                        .into_iter()
                        .filter(|result| filter.matches_symbol_result(result))
                        .take(limit)
                        .collect(),
                        None => Vec::new(),
                    };
                    SymbolSearchResults {
                        results,
                        relaxed: false,
                    }
                }
                SearchBackend::Hybrid => hybrid_search_with_tagged_embedding(
                    &snapshot,
                    &query,
                    &filter,
                    limit,
                    tagged,
                    Some(julie_index::search::weights::SearchWeightProfile::fast_search()),
                    semantic_mode,
                )?,
                SearchBackend::Lexical => {
                    unreachable!("lexical backend is handled by run_unified_pass")
                }
            };
            let total = results.results.len();
            let hits = results
                .results
                .into_iter()
                .map(|result| symbol_result_to_hit(result, workspace_id.clone()))
                .collect();
            Ok((hits, results.relaxed, total))
        })
        .await??;

    sort_hits_by_score_desc(&mut hits);
    hits.truncate(limit);

    let strategy_id = match backend {
        SearchBackend::Semantic => "fast_search_semantic",
        SearchBackend::Hybrid => "fast_search_hybrid",
        SearchBackend::Lexical => "search_unified",
    };
    Ok(SearchExecutionResult::new(
        hits,
        relaxed,
        total_results,
        strategy_id,
        SearchExecutionKind::Definitions,
    ))
}

fn symbol_result_to_hit(result: SymbolSearchResult, workspace: String) -> SearchHit {
    let kind = SymbolKind::try_from_string(&result.kind).unwrap_or(SymbolKind::Variable);
    SearchHit::from_symbol(
        Symbol {
            extracted: julie_extractors::Symbol {
                id: result.id,
                name: result.name,
                kind,
                language: result.language,
                file_path: result.file_path,
                start_line: result.start_line,
                start_column: 0,
                end_line: 0,
                end_column: 0,
                start_byte: 0,
                end_byte: 0,
                signature: if result.signature.is_empty() {
                    None
                } else {
                    Some(result.signature)
                },
                doc_comment: if result.doc_comment.is_empty() {
                    None
                } else {
                    Some(result.doc_comment)
                },
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: Some(result.score),
                content_type: None,
                body_span: None,
                body_hash: None,
                annotations: Vec::new(),
            },
            code_context: None,
        },
        workspace,
    )
}
