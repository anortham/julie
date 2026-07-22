use std::cmp::Ordering;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use julie_extractors::{Symbol, SymbolKind};
use julie_index::search::{SearchFilter, SymbolSearchResult};
use julie_pipeline::embeddings::EmbeddingProvider;

use julie_context::ToolContext;

use super::backend::{ResolvedSearchBackend, SearchBackend};
use super::hint_formatter;
use super::line_mode;
use super::query;
use super::text_search;
use super::trace::{
    FilePatternDiagnostic, SearchExecutionKind, SearchExecutionResult, SearchHit, ZeroHitReason,
};

pub struct SearchExecutionParams<'a> {
    pub query: &'a str,
    pub language: &'a Option<String>,
    pub file_pattern: &'a Option<String>,
    pub limit: u32,
    pub context_lines: Option<u32>,
    pub exclude_tests: Option<bool>,
    pub backend: ResolvedSearchBackend,
}

#[derive(Debug, Clone)]
pub struct SearchExecutionWorkspace {
    pub workspace_id: String,
}

impl SearchExecutionWorkspace {
    pub fn primary(workspace_id: String) -> Self {
        Self { workspace_id }
    }

    pub fn target(workspace_id: String) -> Self {
        Self { workspace_id }
    }
}

pub async fn execute_search(
    params: SearchExecutionParams<'_>,
    workspaces: &[SearchExecutionWorkspace],
    handler: &dyn ToolContext,
) -> Result<SearchExecutionResult> {
    // Normalize empty/whitespace-only file_pattern to None so every caller
    // (FastSearchTool, dashboard route, compare bench, …) gets the same
    // "no filter" behavior instead of an empty-pattern match-nothing. This
    // runs once at the shared entry point; downstream stages must never
    // observe a blank file_pattern.
    let normalized_file_pattern: Option<String> = params.file_pattern.as_ref().and_then(|s| {
        if s.trim().is_empty() {
            None
        } else {
            Some(s.clone())
        }
    });

    let normalized_params = SearchExecutionParams {
        query: params.query,
        language: params.language,
        file_pattern: &normalized_file_pattern,
        limit: params.limit,
        context_lines: params.context_lines,
        exclude_tests: params.exclude_tests,
        backend: params.backend,
    };

    // T8 cutover: all traffic routes through the unified path.
    // The per-target execute_* functions (execute_definition_search,
    // execute_content_search, execute_file_search) still exist but are
    // unreachable from production callers; T9 will delete them.
    execute_search_unified(normalized_params, workspaces, handler).await
}

fn sort_hits_by_score_desc(hits: &mut [SearchHit]) {
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
}

// ---------------------------------------------------------------------------
// Phase 2 — unified execution path
// ---------------------------------------------------------------------------

/// Execute a unified BM25 search across all FTS fields, returning mixed-kind
/// [`SearchHit`]s.  No `doc_type` filter — symbol rows and file rows both
/// contribute to the result set.
pub async fn execute_search_unified(
    params: SearchExecutionParams<'_>,
    workspaces: &[SearchExecutionWorkspace],
    handler: &dyn ToolContext,
) -> Result<SearchExecutionResult> {
    // Normalize empty/whitespace-only file_pattern to None so callers that
    // bypass `execute_search` (e.g., `FastSearchTool::execute_with_trace`)
    // get the same "no filter" behaviour as the rest of the pipeline.
    let normalized_file_pattern: Option<String> = params.file_pattern.as_ref().and_then(|s| {
        if s.trim().is_empty() {
            None
        } else {
            Some(s.clone())
        }
    });

    // T8 follow-up: apply the NL-default-exclude-tests rule that
    // `execute_content_search` used to provide.  When the caller passes
    // `exclude_tests: None`, default to excluding tests if the query looks
    // natural-language-like AND the caller did not scope to a test
    // file_pattern.  Explicit `exclude_tests: Some(_)` always wins.
    let effective_exclude_tests = line_mode::effective_content_exclude_tests(
        params.query,
        &normalized_file_pattern,
        params.exclude_tests,
    );
    let backend_fallback = if params.backend.value != SearchBackend::Lexical {
        if let Some(provider) = handler
            .ensure_embedding_provider(Duration::from_secs(3))
            .await
        {
            if workspaces_have_embeddings(workspaces, handler).await? {
                let mut execution = run_symbol_backend_pass(
                    params.backend.value,
                    params.query,
                    params.language,
                    normalized_file_pattern.as_deref(),
                    params.limit,
                    effective_exclude_tests,
                    workspaces,
                    handler,
                    provider,
                )
                .await?;
                execution.trace.or_disjunction_detected =
                    query::clean_or_disjunction_terms(params.query).is_some();
                return Ok(execution);
            }
        }
        params.backend.explicit
    } else {
        false
    };

    // First pass: run the unified search with the caller's file_pattern.
    let first = run_unified_pass(
        params.query,
        params.language,
        normalized_file_pattern.as_deref(),
        params.limit,
        effective_exclude_tests,
        workspaces,
        handler,
    )
    .await?;

    let mut execution = SearchExecutionResult::new(
        first.hits,
        first.relaxed,
        first.total_results,
        "search_unified",
        SearchExecutionKind::Definitions,
    );

    // Stamp OR-disjunction detection on every run (matches the legacy
    // execute_content_search behaviour so callers and telemetry see the same
    // signal regardless of hit count).
    execution.trace.or_disjunction_detected =
        query::clean_or_disjunction_terms(params.query).is_some();
    execution.trace.backend_fallback = backend_fallback;

    // Zero-hit attribution (first filter wins): file_pattern drops candidates
    // before test-exclude does, so attribute to FilePatternFiltered when the
    // pattern dropped every candidate that the index produced.  Otherwise
    // attribute to TestFiltered when the exclude-tests filter ate the rest.
    let mut zero_hit_reason: Option<ZeroHitReason> = None;
    let mut file_pattern_diagnostic: Option<FilePatternDiagnostic> = None;
    if execution.hits.is_empty() {
        if normalized_file_pattern.is_some()
            && first.pre_file_pattern_filter_total > 0
            && first.pre_test_filter_total == 0
        {
            zero_hit_reason = Some(ZeroHitReason::FilePatternFiltered);
            file_pattern_diagnostic = Some(FilePatternDiagnostic::NoInScopeCandidates);
        } else if effective_exclude_tests && first.pre_test_filter_total > 0 {
            zero_hit_reason = Some(ZeroHitReason::TestFiltered);
        }
    }

    // Scope rescue: when the scoped miss is a real out-of-scope request
    // (NoInScopeCandidates) and the pattern is not a whitespace-separated
    // multi-glob mistake, re-run the unified search without the file_pattern.
    // If the unscoped run yields hits, surface them with the scope-relaxed
    // markers so callers see "0 in scope; here is what exists outside scope".
    let should_rescue = zero_hit_reason == Some(ZeroHitReason::FilePatternFiltered)
        && file_pattern_diagnostic == Some(FilePatternDiagnostic::NoInScopeCandidates)
        && normalized_file_pattern
            .as_deref()
            .is_some_and(|pattern| !query::looks_like_whitespace_separated_globs(pattern));

    if should_rescue {
        let rescue = run_unified_pass(
            params.query,
            params.language,
            None,
            params.limit,
            effective_exclude_tests,
            workspaces,
            handler,
        )
        .await?;

        if !rescue.hits.is_empty() {
            execution.hits = rescue.hits;
            execution.relaxed = rescue.relaxed;
            execution.total_results = rescue.total_results;
            execution.trace.refresh_hits(&execution.hits);
            execution.trace.scope_relaxed = true;
            execution.trace.scope_rescue_count = 1;
            execution.trace.original_file_pattern = normalized_file_pattern.clone();
            execution.trace.original_zero_hit_reason = zero_hit_reason.take();
            // Clear in-scope diagnostics — the public trace now describes the
            // rescued (out-of-scope) result set rather than the scoped miss.
            file_pattern_diagnostic = None;
        }
    }

    if execution.hits.is_empty()
        && should_try_semantic_zero_hit_fallback(&params, normalized_file_pattern.as_deref())
        && let Some(provider) = handler.embedding_provider().await
        && workspaces_have_embeddings(workspaces, handler).await?
    {
        let mut semantic_execution = run_symbol_backend_pass(
            SearchBackend::Semantic,
            params.query,
            params.language,
            None,
            params.limit,
            effective_exclude_tests,
            workspaces,
            handler,
            provider,
        )
        .await?;
        if !semantic_execution.hits.is_empty() {
            semantic_execution.trace.strategy_id = "fast_search_semantic_fallback".to_string();
            semantic_execution.trace.or_disjunction_detected =
                execution.trace.or_disjunction_detected;
            semantic_execution
                .trace
                .refresh_hits(&semantic_execution.hits);
            return Ok(semantic_execution);
        }
    }

    // Persist surviving zero-hit attribution on the trace.
    execution.trace.zero_hit_reason = zero_hit_reason;
    execution.trace.file_pattern_diagnostic = file_pattern_diagnostic.clone();

    // When the run still ends with zero hits, run the content zero-hit hint
    // formatter so MCP callers receive a targeted recovery hint instead of
    // the generic "no results" message.  The hint_kind on the trace lets the
    // rendering layer pick the right text block.
    if execution.hits.is_empty()
        && let Some((hint_kind, _hint_text)) = hint_formatter::build_content_zero_hit_hint(
            params.query,
            normalized_file_pattern.as_deref(),
            params.language.as_deref(),
            params.exclude_tests,
            execution.trace.zero_hit_reason.as_ref(),
            file_pattern_diagnostic.as_ref(),
        )
    {
        execution.trace.target_hint =
            super::trace::target_hint_label(&hint_kind).map(str::to_string);
        execution.trace.hint_kind = Some(hint_kind);
    }

    Ok(execution)
}

fn should_try_semantic_zero_hit_fallback(
    params: &SearchExecutionParams<'_>,
    normalized_file_pattern: Option<&str>,
) -> bool {
    params.backend.value == SearchBackend::Lexical
        && !params.backend.explicit
        && normalized_file_pattern.is_none()
        && !params.query.trim().is_empty()
        && !query::looks_like_file_or_path_query(params.query)
        && query::looks_like_identifier_probe_query(params.query)
}

/// Inner helper: run the unified Tantivy/SQLite search across all workspaces
/// with a single `file_pattern` value, apply test-exclude filtering, and
/// return both the hits and the pre-filter counts the caller needs to
/// attribute zero-hit runs.  Splitting this out lets `execute_search_unified`
/// call the same pipeline twice (scoped + unscoped) for scope rescue.
struct UnifiedPassResult {
    hits: Vec<SearchHit>,
    relaxed: bool,
    total_results: usize,
    /// Total raw-hit count across all workspaces *before* the file_pattern
    /// filter.  Used to attribute FilePatternFiltered when this is non-zero
    /// but `pre_test_filter_total` is zero.
    pre_file_pattern_filter_total: usize,
    /// Total candidates that *survived* the file_pattern filter and entered
    /// the test-exclude stage.  Used to attribute TestFiltered when this is
    /// non-zero but the final hits vector is empty.
    pre_test_filter_total: usize,
}

async fn workspaces_have_embeddings(
    workspaces: &[SearchExecutionWorkspace],
    handler: &dyn ToolContext,
) -> Result<bool> {
    for workspace in workspaces {
        let db = handler
            .get_pooled_database_for_workspace(&workspace.workspace_id)
            .await?;
        let count = tokio::task::spawn_blocking(move || db.embedding_count()).await??;
        if count > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
async fn run_symbol_backend_pass(
    backend: SearchBackend,
    query: &str,
    language: &Option<String>,
    file_pattern: Option<&str>,
    limit: u32,
    effective_exclude_tests: bool,
    workspaces: &[SearchExecutionWorkspace],
    handler: &dyn ToolContext,
    provider: Arc<dyn EmbeddingProvider>,
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

        let (mut workspace_hits, workspace_relaxed, workspace_total) =
            tokio::task::spawn_blocking(move || -> Result<(Vec<SearchHit>, bool, usize)> {
                let symbol_results = match backend {
                    SearchBackend::Semantic => run_semantic_symbol_search(
                        &query,
                        &filter,
                        limit_usize,
                        &db,
                        provider.as_ref(),
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
                            julie_index::search::hybrid::compute_query_embedding_for_hybrid(
                                &query,
                                Some(provider.as_ref()),
                            );
                        let index = si_arc;
                        julie_index::search::hybrid::hybrid_search_with_embedding(
                            &query,
                            &filter,
                            limit_usize,
                            &index,
                            &db,
                            precomputed_embedding,
                            Some(julie_index::search::weights::SearchWeightProfile::fast_search()),
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
) -> Result<julie_index::search::SymbolSearchResults> {
    let query_vector = provider.embed_query(query)?;
    let knn_hits = db.knn_search(&query_vector, limit.saturating_mul(4).max(limit))?;
    let mut results: Vec<_> = julie_index::search::hybrid::knn_to_search_results(&knn_hits, db)?
        .into_iter()
        .filter(|result| filter.matches_symbol_result(result))
        .collect();
    results.truncate(limit);

    Ok(julie_index::search::SymbolSearchResults {
        results,
        relaxed: false,
    })
}

fn symbol_result_to_hit(result: SymbolSearchResult, workspace: String) -> SearchHit {
    let kind = SymbolKind::try_from_string(&result.kind).unwrap_or(SymbolKind::Variable);
    SearchHit::from_symbol(
        Symbol {
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
            code_context: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        workspace,
    )
}

#[allow(clippy::too_many_arguments)]
async fn run_unified_pass(
    query: &str,
    language: &Option<String>,
    file_pattern: Option<&str>,
    limit: u32,
    effective_exclude_tests: bool,
    workspaces: &[SearchExecutionWorkspace],
    handler: &dyn ToolContext,
) -> Result<UnifiedPassResult> {
    use julie_index::search::SearchFilter;

    let mut hits = Vec::new();
    let mut relaxed = false;
    let mut total_results = 0usize;
    let mut pre_test_filter_total = 0usize;
    let mut pre_file_pattern_filter_total = 0usize;

    for workspace in workspaces {
        let filter = SearchFilter {
            language: language.clone(),
            kind: None,
            file_pattern: file_pattern.map(str::to_string),
            exclude_tests: effective_exclude_tests,
        };

        // Use `unified_search_hits` (returns raw UnifiedHit) rather than
        // `unified_search_impl` (converts to Symbol) so the "file" kind is
        // preserved end-to-end in the SearchHit.
        //
        // Apply language/file/test filters in the candidate source. The index
        // applies these after Tantivy materialization, so scoped zero-hit cases
        // get one wider scoped retry before any unscoped rescue is considered.
        let raw_fetch_limit = limit.saturating_mul(4).max(50);

        let (mut raw_hits, mut workspace_relaxed, mut workspace_total) =
            text_search::unified_search_hits(
                query,
                &filter,
                raw_fetch_limit,
                Some(vec![workspace.workspace_id.clone()]),
                handler,
            )
            .await?;

        if raw_hits.is_empty() && file_pattern.is_some() {
            let retry_fetch_limit = raw_fetch_limit.saturating_mul(20).max(1_000);
            if retry_fetch_limit > raw_fetch_limit {
                let (retry_hits, retry_relaxed, retry_total) = text_search::unified_search_hits(
                    query,
                    &filter,
                    retry_fetch_limit,
                    Some(vec![workspace.workspace_id.clone()]),
                    handler,
                )
                .await?;
                raw_hits = retry_hits;
                workspace_relaxed |= retry_relaxed;
                workspace_total = retry_total;
            }
        }

        relaxed |= workspace_relaxed;
        total_results += workspace_total;

        for raw_hit in raw_hits {
            hits.push(SearchHit::from_unified_hit(
                raw_hit,
                workspace.workspace_id.clone(),
            ));
        }

        if hits.is_empty() && (file_pattern.is_some() || effective_exclude_tests) {
            let diagnostic_filter = SearchFilter {
                language: language.clone(),
                kind: None,
                file_pattern: None,
                exclude_tests: false,
            };
            let diagnostic_fetch_limit = raw_fetch_limit.saturating_mul(20).max(1_000);
            let (diagnostic_hits, diagnostic_relaxed, _) = text_search::unified_search_hits(
                query,
                &diagnostic_filter,
                diagnostic_fetch_limit,
                Some(vec![workspace.workspace_id.clone()]),
                handler,
            )
            .await?;
            relaxed |= diagnostic_relaxed;
            pre_file_pattern_filter_total += diagnostic_hits.len();

            for raw_hit in diagnostic_hits {
                if let Some(pattern) = file_pattern
                    && !crate::search::matches_glob_pattern(&raw_hit.file_path, pattern)
                {
                    continue;
                }
                pre_test_filter_total += 1;
                if effective_exclude_tests
                    && (julie_index::search::scoring::is_test_path(&raw_hit.file_path)
                        || raw_hit.role == "test")
                {
                    continue;
                }
            }
        }
    }

    sort_hits_by_score_desc(&mut hits);
    hits.truncate(limit.max(1) as usize);

    Ok(UnifiedPassResult {
        hits,
        relaxed,
        total_results,
        pre_file_pattern_filter_total,
        pre_test_filter_total,
    })
}
