use std::sync::Arc;

use anyhow::Result;

use julie_context::ToolContext;
use julie_core::Symbol;
use julie_extractors::SymbolKind;
use julie_index::search::{SearchFilter, SymbolSearchResult};
use julie_pipeline::embeddings::{EmbeddingProvider, EmbeddingRequestBudget};

use super::types::{SearchExecutionWorkspace, sort_hits_by_score_desc};
use crate::search::backend::SearchBackend;
use crate::search::trace::{SearchExecutionKind, SearchExecutionResult, SearchHit};

pub(crate) async fn workspaces_have_embeddings(
    workspaces: &[SearchExecutionWorkspace],
    handler: &dyn ToolContext,
) -> Result<bool> {
    for workspace in workspaces {
        let db = handler
            .get_pooled_database_for_workspace(&workspace.workspace_id)
            .await?;
        let ready = tokio::task::spawn_blocking(move || -> Result<bool> {
            Ok(db.get_latest_ready_generation()?.is_some())
        })
        .await??;
        if ready {
            return Ok(true);
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_symbol_backend_pass(
    backend: SearchBackend,
    query: &str,
    language: &Option<String>,
    file_pattern: Option<&str>,
    limit: u32,
    effective_exclude_tests: bool,
    workspaces: &[SearchExecutionWorkspace],
    handler: &dyn ToolContext,
    provider: Arc<dyn EmbeddingProvider>,
    semantic_mode: julie_core::embeddings_contract::SemanticMode,
    budget: &EmbeddingRequestBudget,
) -> Result<SearchExecutionResult> {
    let mut hits = Vec::new();
    let mut relaxed = false;
    let mut total_results = 0usize;
    let limit_usize = limit.max(1) as usize;

    for workspace in workspaces {
        let filter = SearchFilter {
            language: language.clone(),
            kind: None,
            file_pattern: file_pattern.map(str::to_string),
            exclude_tests: effective_exclude_tests,
        };
        let db = handler
            .get_pooled_database_for_workspace(&workspace.workspace_id)
            .await?;
        let search_index = if backend == SearchBackend::Hybrid {
            handler
                .get_search_index_for_workspace(&workspace.workspace_id)
                .await?
        } else {
            None
        };
        let workspace_id = workspace.workspace_id.clone();
        let query = query.to_string();
        let provider = Arc::clone(&provider);
        let budget = budget.clone();

        let (mut workspace_hits, workspace_relaxed, workspace_total) =
            tokio::task::spawn_blocking(move || -> Result<(Vec<SearchHit>, bool, usize)> {
                let symbol_results = match backend {
                    SearchBackend::Semantic => run_semantic_symbol_search(
                        &query,
                        &filter,
                        limit_usize,
                        &db,
                        provider.as_ref(),
                        &budget,
                        semantic_mode,
                    )?,
                    SearchBackend::Hybrid => {
                        let si_arc = search_index.ok_or_else(|| {
                            anyhow::anyhow!(
                                "Search index not initialized for workspace '{}'",
                                workspace_id
                            )
                        })?;
                        // Compute embedding before hybrid search. The sidecar RPC
                        // can take up to 30 s; keep that off the Tantivy search path.
                        let precomputed_embedding =
                            julie_index::search::hybrid::compute_tagged_query_embedding_for_hybrid(
                                &query,
                                Some(provider.as_ref()),
                                &budget,
                                &db,
                                semantic_mode,
                            )?;
                        let index = si_arc;
                        julie_index::search::hybrid::hybrid_search_with_tagged_embedding(
                            &query,
                            &filter,
                            limit_usize,
                            &index,
                            &db,
                            precomputed_embedding,
                            Some(julie_index::search::weights::SearchWeightProfile::fast_search()),
                            semantic_mode,
                        )?
                    }
                    SearchBackend::Lexical => {
                        unreachable!("lexical backend is handled by run_unified_pass")
                    }
                };
                let total = symbol_results.results.len();
                let hits = symbol_results
                    .results
                    .into_iter()
                    .map(|result| symbol_result_to_hit(result, workspace_id.clone()))
                    .collect();
                Ok((hits, symbol_results.relaxed, total))
            })
            .await??;

        hits.append(&mut workspace_hits);
        relaxed |= workspace_relaxed;
        total_results += workspace_total;
    }

    sort_hits_by_score_desc(&mut hits);
    hits.truncate(limit_usize);

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

fn run_semantic_symbol_search(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    db: &julie_core::database::SymbolDatabase,
    provider: &dyn EmbeddingProvider,
    budget: &EmbeddingRequestBudget,
    semantic_mode: julie_core::embeddings_contract::SemanticMode,
) -> Result<julie_index::search::SymbolSearchResults> {
    let key = match provider.encoder_identity().and_then(|id| id.storage_key()) {
        Ok(k) => k,
        Err(e) => {
            if semantic_mode == julie_core::embeddings_contract::SemanticMode::Required {
                return Err(e.context("SEMANTICS_NOT_READY: Encoder identity unavailable"));
            }
            return Ok(julie_index::search::SymbolSearchResults {
                results: Vec::new(),
                relaxed: false,
            });
        }
    };

    let query_vector = match provider.embed_query(query, budget) {
        Ok(v) => v,
        Err(e) => {
            budget.check_budget()?;
            if semantic_mode == julie_core::embeddings_contract::SemanticMode::Required {
                return Err(e.context("SEMANTICS_NOT_READY: embed_query failed"));
            }
            return Ok(julie_index::search::SymbolSearchResults {
                results: Vec::new(),
                relaxed: false,
            });
        }
    };
    let knn_limit = limit.saturating_mul(4).max(limit);

    let results = db.with_read_transaction(|tx_db| {
        let rev = tx_db.get_latest_canonical_revision_number()?.unwrap_or(0);
        if !tx_db.embedding_generation_ready(&key, rev)? {
            if semantic_mode == julie_core::embeddings_contract::SemanticMode::Required {
                anyhow::bail!(
                    "SEMANTICS_NOT_READY: generation for encoder '{}' at rev {} is not ready",
                    key,
                    rev
                );
            }
            return Ok(Vec::new());
        }

        let knn_hits = tx_db.knn_search(&query_vector, knn_limit)?;
        let mut res: Vec<_> = julie_index::search::hybrid::knn_to_search_results(&knn_hits, tx_db)?
            .into_iter()
            .filter(|result| filter.matches_symbol_result(result))
            .collect();
        res.truncate(limit);
        Ok(res)
    })?;

    Ok(julie_index::search::SymbolSearchResults {
        results,
        relaxed: false,
    })
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
