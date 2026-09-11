pub mod semantic;
pub mod types;
pub mod unified_pass;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use julie_context::ToolContext;
use julie_index::snapshot::Snapshot;

use crate::search::backend::SearchBackend;
use crate::search::hint_formatter;
use crate::search::line_mode;
use crate::search::query;
use crate::search::trace::{
    FilePatternDiagnostic, SearchExecutionKind, SearchExecutionResult, ZeroHitReason,
};

pub use self::types::{SearchExecutionParams, SearchExecutionWorkspace};

use self::semantic::{SymbolPassRequest, run_symbol_backend_pass, snapshot_has_embeddings};
use self::unified_pass::run_unified_pass;

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
        semantic_mode: params.semantic_mode,
        budget: params.budget,
    };

    let workspace = workspaces
        .first()
        .ok_or_else(|| anyhow::anyhow!("execute_search needs one workspace"))?;
    let snapshot = handler.snapshot(&workspace.target).await?;
    execute_search_unified(normalized_params, workspace, &snapshot, handler).await
}

// ---------------------------------------------------------------------------
// Phase 2 — unified execution path
// ---------------------------------------------------------------------------

/// Execute a unified BM25 search across all FTS fields, returning mixed-kind
/// [`SearchHit`]s.  No `doc_type` filter — symbol rows and file rows both
/// contribute to the result set.
pub async fn execute_search_unified(
    params: SearchExecutionParams<'_>,
    workspace: &SearchExecutionWorkspace,
    snapshot: &Arc<Snapshot>,
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
    let mut auto_symbol_pass_was_empty = false;
    let backend_fallback = if params.backend.value != SearchBackend::Lexical {
        // Only an explicit semantic/hybrid request may pay the provider
        // lazy-init wait; an auto-selected hybrid run must not block a plain
        // query on a degraded or starting provider.
        let provider = if params.backend.explicit {
            handler
                .ensure_embedding_provider(Duration::from_secs(3))
                .await
        } else {
            handler.embedding_provider().await
        };
        if let Some(provider) = provider {
            if snapshot_has_embeddings(snapshot) {
                let mode = params
                    .semantic_mode
                    .unwrap_or(julie_core::embeddings_contract::SemanticMode::Auto);
                let request = SymbolPassRequest {
                    backend: params.backend.value,
                    query: params.query,
                    language: params.language,
                    file_pattern: normalized_file_pattern.as_deref(),
                    limit: params.limit,
                    exclude_tests: effective_exclude_tests,
                    workspace_id: &workspace.workspace_id,
                    provider,
                    semantic_mode: mode,
                    budget: params.budget.clone().unwrap_or_default(),
                };
                let mut execution = run_symbol_backend_pass(request, snapshot).await?;
                execution.trace.or_disjunction_detected =
                    query::clean_or_disjunction_terms(params.query).is_some();
                if params.backend.explicit || !execution.hits.is_empty() {
                    return Ok(execution);
                }
                auto_symbol_pass_was_empty = true;
            } else if params.semantic_mode
                == Some(julie_core::embeddings_contract::SemanticMode::Required)
            {
                anyhow::bail!(
                    "SEMANTICS_NOT_READY: Workspaces lack ready embedding generation in Required mode"
                );
            }
        } else if params.semantic_mode
            == Some(julie_core::embeddings_contract::SemanticMode::Required)
        {
            anyhow::bail!("SEMANTICS_NOT_READY: Embedding provider unavailable in Required mode");
        }

        if !auto_symbol_pass_was_empty
            && params.semantic_mode == Some(julie_core::embeddings_contract::SemanticMode::Required)
        {
            anyhow::bail!(
                "SEMANTICS_NOT_READY: SemanticMode::Required cannot fall back to lexical search"
            );
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
        workspace,
        snapshot,
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
            workspace,
            snapshot,
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
        && snapshot_has_embeddings(snapshot)
    {
        let request = SymbolPassRequest {
            backend: SearchBackend::Semantic,
            query: params.query,
            language: params.language,
            file_pattern: None,
            limit: params.limit,
            exclude_tests: effective_exclude_tests,
            workspace_id: &workspace.workspace_id,
            provider,
            semantic_mode: params
                .semantic_mode
                .unwrap_or(julie_core::embeddings_contract::SemanticMode::Auto),
            budget: params.budget.clone().unwrap_or_default(),
        };
        let mut semantic_execution = run_symbol_backend_pass(request, snapshot).await?;
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
